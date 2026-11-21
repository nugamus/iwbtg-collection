//! `physics.toml` to `PhysicsProfileDraft`, then `kid::presets::apply`; legacy drafts map to
//! their family preset with a warning (docs/format/physics.md). WP0 creates the
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
use iwbtg_sim::def::physics::{KidScriptDraft, KidSoundNames, KidSpriteNames, PhysicsProfileDraft};
use iwbtg_sim::def::{KidFamily, PhysicsProfile};
use iwbtg_sim::kid::presets;
use toml::Spanned;

use crate::diag::Diagnostic;
use crate::engine_pack;
use crate::files::PackageFiles;
use crate::manifest::{MANIFEST_PATH, Manifest, TrickSetting};

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
    /// `[script]` as written: `file` and `vars` (decision 0004). `None` for a legacy draft.
    pub script: Option<KidScriptDraft>,
    /// A legacy draft: it names no sprites, so the loader looks for `player_idle`,
    /// `player_running`, `player_jump`, `player_fall` and `player_sliding` (CR8).
    pub legacy: bool,
}

/// How a package's resolved physics profile stands against its engine pack's family preset
/// (#802; docs/content.md "Engines": a game whose physics differ from its pack's original is
/// marked custom physics).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicsStanding {
    /// The engine pack `game.toml` names; `None` for a self-contained package.
    pub pack: Option<String>,
    /// The family preset the profile was compared with.
    pub family: KidFamily,
    /// The fields that differ, in profile order (`numbers.gravity`, `events.step`).
    pub differs: Vec<String>,
    /// Room profiles `physics/<id>.toml` that differ from the game's profile, in id order
    /// (#970). Any makes the package a medley and custom physics.
    pub rooms: Vec<RoomPhysicsStanding>,
}

/// A room profile `physics/<id>.toml` that differs from the game's resolved profile (#970,
/// docs/format/engine.md "Custom physics").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomPhysicsStanding {
    /// The profile id: the file name without `.toml`.
    pub id: String,
    /// The fields that differ, in profile order.
    pub differs: Vec<String>,
}

impl PhysicsStanding {
    /// True when the game's profile differs from the pack's family preset, or a room profile
    /// differs from the game's profile (a medley).
    pub fn is_custom(&self) -> bool {
        !self.differs.is_empty() || !self.rooms.is_empty()
    }

    /// What a matching profile prints: the pack id, or `<family> preset` without a pack.
    pub fn name(&self) -> String {
        match &self.pack {
            Some(pack) => pack.clone(),
            None => format!("{:?} preset", self.family),
        }
    }
}

/// Compares the game's resolved profile with the family preset of the pack
/// `manifest.engine_pack` names (`kid::presets::defaults`), and every room profile with the
/// game's resolved profile (#970). Without a pack the profile's own `preset` is the family.
/// `Real` equality is bitwise, so this compares exactly the numbers the sim runs. `rooms` holds
/// each loaded `physics/<id>.toml` with its id; a room profile that matches the game's file is
/// not passed (the loader shares the game's profile for it).
pub fn standing(
    files: &PackageFiles,
    manifest: &Manifest,
    profile: &PhysicsProfile,
    rooms: &[(&str, &PhysicsProfile)],
) -> PhysicsStanding {
    let pack = manifest.engine_pack.as_ref().map(|p| p.get_ref().clone());
    let family = pack
        .as_deref()
        .and_then(|pack| engine_pack::family(files, pack))
        .unwrap_or(profile.preset);
    let base = presets::defaults(family);
    let rooms = rooms
        .iter()
        .filter_map(|(id, room)| {
            let differs = differing_fields(room, profile);
            (!differs.is_empty()).then(|| RoomPhysicsStanding {
                id: (*id).to_owned(),
                differs,
            })
        })
        .collect();
    PhysicsStanding {
        pack,
        family,
        differs: differing_fields(profile, &base),
        rooms,
    }
}

/// A section's fields compared one by one. A section that differs without a listed field
/// differing (a field added later) still counts, as the section name.
macro_rules! differ_section {
    ($out:expr, $name:literal, $actual:expr, $base:expr, [$($field:ident),* $(,)?]) => {{
        let mut listed = false;
        $(
            if $actual.$field != $base.$field {
                $out.push(concat!($name, ".", stringify!($field)).to_owned());
                listed = true;
            }
        )*
        if !listed && $actual != $base {
            $out.push($name.to_owned());
        }
    }};
}

/// The fields of `actual` that differ from `base`, in profile order. `preset`, `sprites` and
/// `sounds` are not physics: the pack preset has no assets, and every package resolves its own.
fn differing_fields(actual: &PhysicsProfile, base: &PhysicsProfile) -> Vec<String> {
    let mut out = Vec::new();
    differ_section!(
        out,
        "numbers",
        actual.numbers,
        base.numbers,
        [
            jumps,
            gravity,
            run_speed,
            max_vspeed,
            max_fall_speed,
            release_mult,
            sprite_threshold,
            image_speed_idle,
            image_speed_run,
            air_wall_probe,
        ]
    );
    differ_section!(out, "input", actual.input, base.input, [walk, vine_keys]);
    differ_section!(
        out,
        "events",
        actual.events,
        base.events,
        [
            create, room_start, step, end_step, collisions, alarms, keys, boundary,
        ]
    );
    differ_section!(
        out,
        "jump",
        actual.jump,
        base.jump,
        [
            ground_classes,
            ground_also,
            ground_sets_djump,
            release_twice_while_tjump,
            air,
            water_air,
            max_jumps,
            inf_jump,
            blocked_by_pause_press,
            cancel,
        ]
    );
    differ_section!(out, "fall_cap", actual.fall_cap, base.fall_cap, [rule]);
    differ_section!(
        out,
        "gravity",
        actual.gravity,
        base.gravity,
        [reapply, block_landing_zeroes]
    );
    differ_section!(
        out,
        "platform",
        actual.platform,
        base.platform,
        [hold_probe, rule, snap, take, reversed_above, reversed_snap,]
    );
    differ_section!(
        out,
        "vine",
        actual.vine,
        base.vine,
        [
            walk_ignores_vines,
            model,
            left_below,
            right_equal,
            slide,
            kick,
            drop,
            probe_down_fixed,
        ]
    );
    // `[water]` has no fields yet; if it gains one, the section's own equality still catches it.
    if actual.water != base.water {
        out.push("water".to_owned());
    }
    differ_section!(
        out,
        "ice",
        actual.ice,
        base.ice,
        [model, probe, read, clamp, temp_probe, temp_decay, temp_zero_below,]
    );
    differ_section!(
        out,
        "slope",
        actual.slope,
        base.slope,
        [
            block_step,
            step,
            mirror_contact,
            downhill,
            jump_probe,
            air_sprite_probe,
            keep_hspeed,
        ]
    );
    differ_section!(out, "ladder", actual.ladder, base.ladder, [entry]);
    differ_section!(
        out,
        "flip",
        actual.flip,
        base.flip,
        [model, solidblock_no_refresh]
    );
    differ_section!(
        out,
        "death",
        actual.death,
        base.death,
        [
            order,
            select_room_reset,
            stop_music,
            first_kid,
            killer_check_menu_rooms,
        ]
    );
    differ_section!(out, "bullet", actual.bullet, base.bullet, [max, speed, offset]);
    differ_section!(out, "save", actual.save, base.save, [position, fixed, first_kid]);
    differ_section!(
        out,
        "misc",
        actual.misc,
        base.misc,
        [fade_in, fade_step, idle_needs_block]
    );
    // A profile with slots or vars differs from the pack preset, which has neither.
    differ_section!(out, "script", actual.script, base.script, [file, vars]);
    out
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
    // The loader resolves the script file name to an id after `apply`; keep the name here.
    let script = draft.script.clone();
    let profile = presets::apply(&mut draft);
    Some(PhysicsLoad {
        profile,
        sprites,
        sounds,
        script,
        legacy: false,
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
        script: None,
        legacy: true,
    })
}

/// Whether the profile has a trick's switch on. `None`: the trick is emergent or has no switch
/// the checker knows, so `required` cannot be wrong. `Some(false)`: the switch is off or does not
/// exist yet (the second text says which).
fn switch_state(id: &str, p: &PhysicsProfile) -> Option<(bool, &'static str)> {
    // Keys are the research ids without `trick.`, `input.` or `bug.` (`crate::tricks`); every
    // `switch` key of that table has a case here (tests::every_switch_key_has_a_case).
    match id {
        // Built into the input layer.
        "dual_shift" => Some((true, "")),
        "restart_glitch" => Some((
            p.input.walk == WalkRead::DirectLatch,
            "needs `[input] walk = \"direct_latch\"`",
        )),
        // No switch in the profile, or the client has no such layer (docs/architecture/sim/kid.md).
        "ad_align" => Some((false, "has no switch in the profile yet (`misc.ad_nudge`)")),
        "disable_cancels" => Some((false, "has no switch in the profile yet (`jump.cancel`)")),
        "sub_frame_tap" => Some((false, "has no switch in the profile yet (`input.tap_fix`)")),
        "cactus_phantom_release"
        | "numpad_cancel"
        | "coyote_time"
        | "jump_buffer"
        | "vdiet"
        | "water_full_jump_1f"
        | "water1_low_check"
        | "restart_f2_same_frame" => {
            Some((false, "is never on in this client (docs/architecture/sim/kid.md)"))
        }
        _ => None,
    }
}

/// The `required` trick validator (docs/architecture/sim/kid.md): a trick marked `required` needs its
/// switch on. `manifest_text` is the text of `game.toml`, for positions.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tricks::{TRICKS, TrickType};

    /// The loader accepts the standard names and resolves them to exactly the profile the legacy
    /// names resolve to, so no value (and therefore no `gameplay_hash` of an unchanged file)
    /// moves.
    #[test]
    fn standard_names_load_to_the_same_profile_as_the_legacy_names() {
        let profile = |text: &str| {
            let mut files = PackageFiles::new();
            files
                .insert("physics.toml", text.as_bytes().to_vec())
                .expect("insert");
            let mut diags = Vec::new();
            let loaded = load(&files, "physics.toml", false, &mut diags).expect("loads");
            assert!(diags.is_empty(), "{} diagnostics", diags.len());
            loaded.profile
        };
        let legacy = profile(
            "preset = \"A1\"\n[numbers]\nmax_fall_speed = 5.0\n\
             [vine]\nmodel = \"distance_sym_early\"\n",
        );
        let standard = profile(
            "preset = \"A1\"\n[numbers]\nmax_fall = 5.0\n\
             [vines]\nmodel = \"symmetric_early\"\n",
        );
        assert_eq!(legacy, standard);
    }

    #[test]
    fn every_switch_key_has_a_case() {
        let mut draft: PhysicsProfileDraft = toml::from_str("preset = \"A1\"\n").expect("preset A1 parses");
        let profile = presets::apply(&mut draft);
        for (key, ty) in TRICKS {
            let has_case = switch_state(key, &profile).is_some();
            assert_eq!(has_case, *ty == TrickType::Switch, "{key}");
        }
    }
}
