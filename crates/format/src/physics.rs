//! `physics.toml` to `PhysicsProfileDraft`, then `kid::presets::apply`; legacy drafts map to
//! their family preset with a warning (docs/plans/sim-core.md 8.2 step 7, 5.4). WP0 creates the
//! file; WP7b owns it.
//!
//! A file in the form of `docs/format/physics.md` is read strictly (unknown keys are errors with
//! a position). A legacy draft, the form the research converter wrote before the profile was
//! specified (`rules`, `jumps`, `step_sets`, `air_jump_restore`, plain numbers), is not read for
//! its values: it maps to the family preset its `rules` names (`classic` is `A1`) and the load
//! warns, naming the keys it ignored. `[tricks]` marked `required` are checked against the
//! switches the profile has ([`check_tricks`]).

use std::collections::BTreeMap;

use iwbtg_sim::def::physics::WalkRead;
use iwbtg_sim::def::physics::{KidSoundNames, KidSpriteNames, PhysicsProfileDraft};
use iwbtg_sim::def::{KidFamily, PhysicsProfile};
use iwbtg_sim::kid::presets;
use toml::Spanned;

use crate::diag::Diagnostic;
use crate::files::PackageFiles;
use crate::manifest::{MANIFEST_PATH, TrickSetting};

/// Keys only a legacy draft has (the current form has tables, not these).
const LEGACY_KEYS: [&str; 4] = ["rules", "jumps", "step_sets", "air_jump_restore"];

/// What `physics.toml` gave: the profile, and the sprite names the loader resolves to ids
/// (`apply` leaves `profile.sprites` empty).
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsLoad {
    /// The profile, preset applied.
    pub profile: PhysicsProfile,
    /// `[sprites]` as written.
    pub sprites: KidSpriteNames,
    /// `[sounds]` as written.
    pub sounds: KidSoundNames,
}

/// Reads the profile file at `path` (step 7). `trace` is `LoadMode::Trace`: a legacy `rules` that
/// names no family falls back to A1 with a warning instead of an error. `None` when it cannot be read; every problem is a
/// diagnostic, and a legacy draft maps to its family preset with a warning. `load::load_game`
/// (WP7a) calls this.
pub fn load(
    files: &PackageFiles,
    path: &str,
    trace: bool,
    diags: &mut Vec<Diagnostic>,
) -> Option<PhysicsLoad> {
    let bytes = files.get(path)?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => {
            let valid = std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap_or("");
            diags.push(Diagnostic::error_at(
                path,
                valid,
                valid.len(),
                "file is not valid UTF-8",
            ));
            return None;
        }
    };
    let mut draft = match toml::from_str::<PhysicsProfileDraft>(text) {
        Ok(d) => d,
        Err(e) => return legacy_or_error(path, text, &e, trace, diags),
    };
    let sprites = draft.sprites.clone().unwrap_or_default();
    let sounds = draft.sounds.clone().unwrap_or_default();
    let profile = presets::apply(&mut draft);
    Some(PhysicsLoad {
        profile,
        sprites,
        sounds,
    })
}

/// A parse failure is either a legacy draft (map it, warn) or a mistake (error at its position).
fn legacy_or_error(
    path: &str,
    text: &str,
    err: &toml::de::Error,
    trace: bool,
    diags: &mut Vec<Diagnostic>,
) -> Option<PhysicsLoad> {
    let at = err.span().map_or(0, |s| s.start);
    let Ok(table) = toml::from_str::<toml::Table>(text) else {
        diags.push(Diagnostic::error_at(path, text, at, err.message().to_owned()));
        return None;
    };
    if !LEGACY_KEYS.iter().any(|k| table.contains_key(*k)) {
        diags.push(Diagnostic::error_at(path, text, at, err.message().to_owned()));
        return None;
    }
    let family = match table.get("rules").and_then(toml::Value::as_str) {
        Some("classic") | None => KidFamily::A1,
        Some(other) if trace => {
            diags.push(Diagnostic::warning_at(
                path,
                text,
                text.find(other).unwrap_or(0),
                format!("legacy `rules = \"{other}\"` names no family; the A1 preset is used"),
            ));
            KidFamily::A1
        }
        Some(other) => {
            diags.push(Diagnostic::error_at(
                path,
                text,
                text.find(other).unwrap_or(0),
                format!("legacy `rules = \"{other}\"` names no family; only `classic` (A1) maps"),
            ));
            return None;
        }
    };
    let ignored: Vec<&str> = table
        .keys()
        .map(String::as_str)
        .filter(|k| *k != "preset")
        .collect();
    diags.push(Diagnostic::warning_at(
        path,
        text,
        0,
        format!(
            "legacy physics profile mapped to the {family:?} preset; its values are not used (ignored keys: {}). \
             Write the form of docs/format/physics.md",
            ignored.join(", ")
        ),
    ));
    let mut draft = PhysicsProfileDraft {
        preset: Some(family),
        ..PhysicsProfileDraft::default()
    };
    let profile = presets::apply(&mut draft);
    Some(PhysicsLoad {
        profile,
        sprites: KidSpriteNames::default(),
        sounds: KidSoundNames::default(),
    })
}

/// Whether the profile has a trick's switch on. `None`: the trick is emergent or has no switch
/// the checker knows, so `required` cannot be wrong. `Some(false)`: the switch is off or does not
/// exist yet (the second text says which).
fn switch_state(id: &str, p: &PhysicsProfile) -> Option<(bool, &'static str)> {
    // `game.toml` ids use only a-z, 0-9 and `_`: the plan's `trick.x` and `input.x` are `x`.
    let id = id
        .strip_prefix("trick.")
        .or_else(|| id.strip_prefix("input."))
        .unwrap_or(id);
    match id {
        // Built into the input layer.
        "dual_shift" => Some((true, "")),
        "restart_glitch" => Some((
            p.input.walk == WalkRead::DirectLatch,
            "needs `[input] walk = \"direct_latch\"`",
        )),
        // No switch in the profile, or the client has no such layer (plan 5.4).
        "ad_align" => Some((false, "has no switch in the profile yet (`misc.ad_nudge`)")),
        "disable_cancels" => Some((false, "has no switch in the profile yet (`jump.cancel`)")),
        "sub_frame_tap" => Some((false, "has no switch in the profile yet (`input.tap_fix`)")),
        "cactus_phantom_release" | "numpad_cancel" | "coyote_time" | "jump_buffer" | "vdiet" => {
            Some((false, "is never on in this client (plan 5.4)"))
        }
        _ => None,
    }
}

/// The `required` trick validator (plan 5.4): a trick marked `required` needs its switch on.
/// `manifest_text` is the text of `game.toml`, for positions.
pub fn check_tricks(
    tricks: &BTreeMap<Spanned<String>, Spanned<TrickSetting>>,
    profile: &PhysicsProfile,
    manifest_text: &str,
    diags: &mut Vec<Diagnostic>,
) {
    for (id, setting) in tricks {
        if *setting.get_ref() != TrickSetting::Required {
            continue;
        }
        if let Some((false, why)) = switch_state(id.get_ref(), profile) {
            diags.push(Diagnostic::error_at(
                MANIFEST_PATH,
                manifest_text,
                setting.span().start,
                format!(
                    "trick `{}` is marked required but its switch is off: it {why}",
                    id.get_ref()
                ),
            ));
        }
    }
}
