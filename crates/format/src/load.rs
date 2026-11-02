//! Package to `GameDef` (docs/plans/sim-core.md 8.1, 8.2). WP7a owns this file, including the
//! calls to the WP7b steps (`physics::load`, `sounds::load`, `paths::load`, `timelines::load`,
//! `params::check`), whose signatures WP0 fixed; WP7b owns only their bodies. The manifest's
//! `[engine]` and `[timing]` sections are WP7a's too (`manifest.rs`).
//!
//! Steps (plan 8.2): 1 manifest and `[engine]`; 2 sprites and masks; 3 kinds; 4 rooms; 5 ranks;
//! 6 variable ids; 7 (WP7b) physics, params, timelines, paths, sounds; 8 cross-references;
//! 9 `gameplay_hash`.
//!
//! `LoadMode::Trace` accepts the research converter's current output: what a converter request
//! (CR1-CR15, docs/format/v0.md "Converter requests") will fix becomes a warning that names the
//! request, and the loader falls back to the trace map (`tests/trace-map.toml`) where it holds
//! the missing data. `LoadMode::Strict` turns the same problems into errors.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use iwbtg_sim::builtins::{self, ParamSpec, ParamType};
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, GroupId, KindDef, KindId, Mask, MaskId, MaskSet, MaskShape,
    NameId, ParamValue, Params, PhysicsProfile, PlacedInstance, RankEntry, RankTable, ResetModel,
    RestartModel, RoomAction, RoomDef, RoomId, RoomRole, ScriptDef, ScriptId, SoundDef, SpriteDef, SpriteId,
    VariantSel, ViewDef,
};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::num::FloatModel;
use iwbtg_sim::timing::{TimingDef, TimingTrigger};
use iwbtg_sim::world::Value;
use toml::Spanned;

use crate::check::check;
use crate::diag::{Diagnostic, Severity, has_errors};
use crate::files::PackageFiles;
use crate::ids::check_id;
use crate::manifest::{ENGINE_ROLES, MANIFEST_PATH, Manifest, TIMING_TRIGGERS};
use crate::masks::{ALPHA_IN_MASK, AlphaRule};
use crate::objects::{self, ObjectFile, kind_classes, legacy_role_classes, param_value};
use crate::order::{OrderEntry, rank_of, resolve_ranks};
use crate::rooms::{ActionFile, InstanceFile, RoomFile, parse_room};
use crate::sprites::{MAX_IMAGE_SIDE, load_sprite_at, parse_sidecar};
use crate::tracemap::{TRACE_MAP_PATH, TraceMap, TraceParam, TraceStatus, load_trace_map};
use crate::vars::intern_vars;
use crate::{params, paths, physics, sounds, timelines};

/// How strict loading is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadMode {
    /// Every kind must be mapped; any error fails the load.
    Strict,
    /// Trace runs on converter output.
    Trace {
        /// Unmapped script kinds load as inert (no events, mask still collides) with a warning.
        unmapped_kinds_inert: bool,
    },
}

impl LoadMode {
    /// True for `Trace`.
    pub const fn is_trace(self) -> bool {
        matches!(self, LoadMode::Trace { .. })
    }
}

/// Everything a load produced: the definition (when it could be built), every diagnostic in step
/// order, and the `gameplay_hash` (only when there is no error).
#[derive(Clone, Debug)]
pub struct LoadReport {
    /// The definition; `None` when an error stopped the load early.
    pub def: Option<GameDef>,
    /// Errors and warnings.
    pub diagnostics: Vec<Diagnostic>,
    /// BLAKE3 over the canonical parsed form ([`gameplay_hash`]).
    pub gameplay_hash: Option<[u8; 32]>,
}

impl LoadReport {
    /// True when the package loaded without errors.
    pub fn ok(&self) -> bool {
        self.def.is_some() && !has_errors(&self.diagnostics)
    }
}

/// Parses and checks a package into the definition the sim runs. Warnings are dropped; use
/// [`load_report`] to see them.
pub fn load_game(files: &PackageFiles, mode: LoadMode) -> Result<GameDef, Vec<Diagnostic>> {
    let report = load_report(files, mode);
    match report.def {
        Some(def) if !has_errors(&report.diagnostics) => Ok(def),
        _ => Err(report.diagnostics),
    }
}

/// Like [`load_game`], but keeps every diagnostic, warnings included (`iwbtg pkg check`).
pub fn load_report(files: &PackageFiles, mode: LoadMode) -> LoadReport {
    let mut diags = check(files);
    let manifest = match read_text(files, MANIFEST_PATH, &mut Vec::new()).map(Manifest::parse) {
        Some(Ok(m)) if !has_errors(&diags) => m,
        _ => {
            return LoadReport {
                def: None,
                diagnostics: diags,
                gameplay_hash: None,
            };
        }
    };
    let mut l = Loader::new(files, mode, &manifest);
    let def = l.run();
    diags.append(&mut l.diags);
    l.emit_notes(&mut diags);
    l.emit_pending(&mut diags);
    let gameplay_hash = match &def {
        Some(def) if !has_errors(&diags) => Some(gameplay_hash(files, &manifest, def)),
        _ => None,
    };
    LoadReport {
        def,
        diagnostics: diags,
        gameplay_hash,
    }
}

// ---------------------------------------------------------------------------------------------
// Converter requests named by Trace-mode warnings (docs/format/v0.md, "Converter requests").

/// What each converter request asks for, as printed in its warning.
fn request_text(cr: &str) -> &'static str {
    match cr {
        "CR1, CR2" => {
            "objects with a script that does not exist load inert (no events; their mask still \
             collides); map them to a built-in (rely objBLOCK to `block`, warpStart to \
             `difficulty_warp`) or write the script"
        }
        "CR3" => "engine objects must become built-ins or be dropped, never `script =`",
        "CR4" => {
            "object_order must list every placed and spawnable kind, engine kinds included; \
             these got a rank after every listed entry"
        }
        "CR5" => "warp instances have no `room_to`: their creation code was not converted",
        "CR6" => "rooms have no `role`; they load as play rooms",
        "CR7" => "[engine] new_game_room is missing; new games start in start_room",
        "CR8" => {
            "the kid's mask comes from the legacy physics.toml `mask`; give the `kid` kind a mask in [defaults.kid]"
        }
        "CR10" => "kind names are not built-ins of plan 6.1; they load inert",
        "CR11" => "objects use the legacy `roles` key; write `counts_as`",
        "CR12" => {
            "bare built-ins have no [defaults] sprite in game.toml; their sprites came from \
             tests/trace-map.toml"
        }
        "CR13" => "rooms have no `file_speed`; it came from tests/trace-map.toml",
        "CR14" => "game.toml has no `room_order`; it came from tests/trace-map.toml",
        "CR15" => "sprite strips are wider or taller than 4096 px",
        _ => "",
    }
}

// ---------------------------------------------------------------------------------------------
// Calls into work packages that may not be merged yet.

/// Runs a call into another work package. Until that package merges, its WP0 stub panics with
/// `not implemented: WPn: <function>`; this records the name and returns `None`, so the rest of
/// the package still loads (with defaults) and `iwbtg pkg check` stays useful. Any other panic is
/// passed on. Temporary scaffolding: WP11 removes it once every loader dependency is merged.
/// On targets without unwinding (wasm32-unknown-unknown) the call is made directly.
pub(crate) fn pending_call<T>(pending: &mut BTreeSet<String>, f: impl FnOnce() -> T) -> Option<T> {
    #[cfg(panic = "unwind")]
    {
        quiet_stub_panics();
        IN_PENDING_CALL.with(|c| c.set(c.get() + 1));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        IN_PENDING_CALL.with(|c| c.set(c.get() - 1));
        match result {
            Ok(v) => Some(v),
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied());
                match msg.and_then(stub_name) {
                    Some(name) => {
                        pending.insert(name.to_owned());
                        None
                    }
                    None => std::panic::resume_unwind(payload),
                }
            }
        }
    }
    #[cfg(not(panic = "unwind"))]
    {
        let _ = pending;
        Some(f())
    }
}

/// `WP7b: physics::load` from a WP0 stub's panic message.
#[cfg(panic = "unwind")]
fn stub_name(msg: &str) -> Option<&str> {
    msg.strip_prefix("not implemented: ")
        .filter(|rest| rest.starts_with("WP"))
}

#[cfg(panic = "unwind")]
std::thread_local! {
    /// How deep this thread is inside [`pending_call`]; the quiet hook only acts above 0.
    static IN_PENDING_CALL: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Keeps WP0 stub panics caught by [`pending_call`] off stderr. A stub panic anywhere else (a
/// tool running the sim into an unmerged work package) and every other panic print as usual.
#[cfg(panic = "unwind")]
fn quiet_stub_panics() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let caught = IN_PENDING_CALL.with(|c| c.get() > 0);
            if !caught || info.payload_as_str().and_then(stub_name).is_none() {
                previous(info);
            }
        }));
    });
}

// ---------------------------------------------------------------------------------------------

/// A sprite's masks as loaded, before they are appended to `GameDef::masks`.
struct SpriteEntry {
    id: SpriteId,
    def: SpriteDef,
}

/// A room file with its source, for positions.
struct ParsedRoom<'f> {
    path: String,
    text: &'f str,
    file: RoomFile,
}

/// Params a variant matches: (key, value) pairs.
type Selector = Vec<(String, ParamValue)>;

/// One variant default: the params it matches, its sprite and its mask.
type Variant = (Selector, Option<SpriteId>, Option<MaskId>);

/// Default sprite and mask of a bare built-in, per variant.
#[derive(Clone, Debug, Default)]
struct Defaults {
    sprite: Option<SpriteId>,
    mask: Option<MaskId>,
    /// (params to match, sprite, mask).
    variants: Vec<Variant>,
}

struct Loader<'a> {
    files: &'a PackageFiles,
    mode: LoadMode,
    m: &'a Manifest,
    manifest_text: &'a str,
    diags: Vec<Diagnostic>,
    /// Converter request id to the items it covers.
    notes: BTreeMap<&'static str, BTreeSet<String>>,
    /// WP0 stubs reached.
    pending: BTreeSet<String>,
    trace: Option<TraceMap>,
    sprite_ids: BTreeMap<String, SpriteEntry>,
    masks: Vec<Mask>,
    kinds: Vec<KindDef>,
    kind_ids: BTreeMap<String, KindId>,
    /// Package objects that were dropped (CR3).
    dropped: BTreeSet<String>,
    /// Package objects whose file had an error (later references to them stay quiet).
    failed: BTreeSet<String>,
    /// Defaults per bare built-in kind.
    defaults: BTreeMap<KindId, Defaults>,
    /// Derived variant kinds: (bare kind, variant index) to kind.
    derived: BTreeMap<(KindId, usize), KindId>,
    /// For derived kinds: the bare kind whose `object_order` entries rank them.
    rank_kind: BTreeMap<KindId, KindId>,
    scripts: Vec<ScriptDef>,
    room_ids: BTreeMap<String, RoomId>,
    sounds: Vec<SoundDef>,
    spec_cache: BTreeMap<Builtin, Option<&'static [ParamSpec]>>,
    ranks: RankTable,
    /// Kinds that got a rank after the listed entries (Trace, CR4).
    appended: BTreeSet<KindId>,
    names: Vec<String>,
    /// Package-defined class names, in first-use order (`ClassMask::package`).
    package_classes: Vec<String>,
    groups: Vec<String>,
}

impl<'a> Loader<'a> {
    fn new(files: &'a PackageFiles, mode: LoadMode, m: &'a Manifest) -> Loader<'a> {
        Loader {
            files,
            mode,
            m,
            manifest_text: read_text(files, MANIFEST_PATH, &mut Vec::new()).unwrap_or(""),
            diags: Vec::new(),
            notes: BTreeMap::new(),
            pending: BTreeSet::new(),
            trace: None,
            sprite_ids: BTreeMap::new(),
            masks: Vec::new(),
            kinds: Vec::new(),
            kind_ids: BTreeMap::new(),
            dropped: BTreeSet::new(),
            failed: BTreeSet::new(),
            defaults: BTreeMap::new(),
            derived: BTreeMap::new(),
            rank_kind: BTreeMap::new(),
            scripts: Vec::new(),
            room_ids: BTreeMap::new(),
            sounds: Vec::new(),
            spec_cache: BTreeMap::new(),
            ranks: RankTable::default(),
            appended: BTreeSet::new(),
            names: Vec::new(),
            package_classes: Vec::new(),
            groups: Vec::new(),
        }
    }

    fn trace_mode(&self) -> bool {
        self.mode.is_trace()
    }

    fn error(&mut self, file: &str, text: &str, span: Range<usize>, msg: impl Into<String>) {
        self.diags.push(Diagnostic::error_at(file, text, span.start, msg));
    }

    fn manifest_error(&mut self, span: Range<usize>, msg: impl Into<String>) {
        let text = self.manifest_text;
        self.error(MANIFEST_PATH, text, span, msg);
    }

    /// Like [`Loader::note`], in `LoadMode::Trace` only: a hand-written package may rely on the
    /// default.
    fn trace_note(&mut self, cr: &'static str, item: String) {
        if self.trace_mode() {
            self.note(cr, item);
        }
    }

    /// Records an item for a converter request's warning (one warning per request).
    fn note(&mut self, cr: &'static str, item: String) {
        self.notes.entry(cr).or_default().insert(item);
    }

    fn emit_notes(&mut self, out: &mut Vec<Diagnostic>) {
        let mut notes: Vec<(&&'static str, &BTreeSet<String>)> = self.notes.iter().collect();
        // CR2 before CR10: by the number after "CR".
        notes.sort_by_key(|(cr, _)| {
            cr.trim_start_matches("CR")
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(u32::MAX)
        });
        for (cr, items) in notes {
            let shown: Vec<&str> = items.iter().take(12).map(String::as_str).collect();
            let more = items.len().saturating_sub(shown.len());
            let tail = if more > 0 {
                format!(" (and {more} more)")
            } else {
                String::new()
            };
            out.push(Diagnostic {
                file: MANIFEST_PATH.to_owned(),
                line: 1,
                column: 1,
                severity: Severity::Warning,
                message: format!(
                    "converter request {cr}: {} ({} item{}): {}{tail}",
                    request_text(cr),
                    items.len(),
                    if items.len() == 1 { "" } else { "s" },
                    shown.join(", ")
                ),
            });
        }
    }

    fn emit_pending(&self, out: &mut Vec<Diagnostic>) {
        if self.pending.is_empty() {
            return;
        }
        let mut by_wp: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for name in &self.pending {
            let (wp, func) = name.split_once(": ").unwrap_or(("WP?", name));
            by_wp.entry(wp).or_default().push(func);
        }
        let parts: Vec<String> = by_wp
            .iter()
            .map(|(wp, funcs)| {
                let shown: Vec<&str> = funcs.iter().take(4).copied().collect();
                let more = funcs.len().saturating_sub(shown.len());
                let tail = if more > 0 {
                    format!(", and {more} more")
                } else {
                    String::new()
                };
                format!("{wp} ({}{tail})", shown.join(", "))
            })
            .collect();
        out.push(Diagnostic {
            file: MANIFEST_PATH.to_owned(),
            line: 1,
            column: 1,
            severity: Severity::Warning,
            message: format!(
                "this build lacks loader steps of work packages not merged yet, so their defaults \
                 were used and the definition is incomplete: {}",
                parts.join("; ")
            ),
        });
    }

    fn run(&mut self) -> Option<GameDef> {
        if self.trace_mode()
            && let Some(text) = read_text(self.files, TRACE_MAP_PATH, &mut Vec::new())
        {
            match load_trace_map(text) {
                Ok(t) => self.trace = Some(t),
                Err(mut d) => {
                    d.severity = Severity::Warning;
                    self.diags.push(d);
                }
            }
        }
        // Step 2.
        self.load_sprites();
        // Step 4, first half: parse rooms and fix their ids, so params can name rooms.
        let rooms = self.parse_rooms();
        // Step 7 (WP7b): sounds before kinds, so params can name sounds.
        let mut sounds = Vec::new();
        let trace = self.trace_mode();
        let mut d = Vec::new();
        if let Some(s) = pending_call(&mut self.pending, || sounds::load(self.files, trace, &mut d)) {
            sounds = s;
        }
        self.diags.append(&mut d);
        self.sounds = sounds;
        // Step 3.
        self.builtin_kinds();
        self.load_objects();
        self.resolve_kind_params();
        // Step 5, first half: object_order.
        self.load_order();
        // Step 7 (WP7b): physics.
        let mut d = Vec::new();
        let physics_path = self.m.physics.get_ref().clone();
        let loaded = physics::load(self.files, &physics_path, self.trace_mode(), &mut d);
        self.diags.append(&mut d);
        let physics = match loaded {
            Some(l) => self.finish_physics(l, &physics_path),
            None => PhysicsProfile::default(),
        };
        let mut d = Vec::new();
        physics::check_tricks(&self.m.tricks, &physics, self.manifest_text, &mut d);
        self.diags.append(&mut d);
        if self.trace_mode() {
            self.legacy_kid_mask();
        }
        // Step 6: variable ids, from every name the loader sees.
        let vars = self.intern(&rooms);
        // Step 4, second half, and step 5 for placed instances.
        let mut room_defs = self.build_rooms(&rooms, &vars);
        // Engine kinds, boot and the spawnable-kind rank check (step 5, CR4).
        let engine_kinds = self.engine_kinds();
        let boot = self.boot(&engine_kinds);
        self.spawnable_ranks(&room_defs, &engine_kinds);
        // Events per kind, now that the physics profile is known.
        self.event_sets(&physics);
        let timing = self.timing();
        let mut d = Vec::new();
        let timelines =
            pending_call(&mut self.pending, || timelines::load(self.files, &mut d)).unwrap_or_default();
        let paths = pending_call(&mut self.pending, || paths::load(self.files, &mut d)).unwrap_or_default();
        self.diags.append(&mut d);
        // Step 8: cross-references not already resolved above.
        self.warp_targets(&room_defs);
        if self.trace_mode() {
            self.inert_twins(&mut room_defs);
        }

        let engine = self.m.engine.as_ref().map(Spanned::get_ref);
        let start_room = self.room_ref(self.m.start_room.get_ref(), self.m.start_room.span());
        let new_game_room = match engine.and_then(|e| e.new_game_room.as_ref()) {
            Some(r) => self.room_ref(r.get_ref(), r.span()),
            None => {
                self.trace_note("CR7", self.m.start_room.get_ref().clone());
                start_room
            }
        };
        let def = GameDef {
            id: self.m.id.get_ref().clone(),
            tick_rate: self.m.tick_rate(),
            max_ticks: *self.m.max_ticks.get_ref(),
            runner: *self.m.runner.get_ref(),
            engine_rev: *self.m.engine_rev.get_ref(),
            float_model: match engine.and_then(|e| e.float_model) {
                None | Some(crate::manifest::FloatModelName::F64) => FloatModel::F64,
            },
            uninit_reads_zero: engine.and_then(|e| e.uninit_reads_zero).unwrap_or(true),
            sprites: self.sprites_in_order(),
            masks: std::mem::take(&mut self.masks),
            kinds: std::mem::take(&mut self.kinds),
            rooms: room_defs,
            start_room: start_room?,
            new_game_room: new_game_room?,
            boot,
            ranks: std::mem::take(&mut self.ranks),
            engine_kinds,
            var_names: vars,
            names: std::mem::take(&mut self.names),
            groups: std::mem::take(&mut self.groups),
            physics,
            restart: engine
                .and_then(|e| e.restart.as_ref())
                .map_or_else(RestartModel::default, |r| r.model()),
            reset: engine
                .and_then(|e| e.reset.as_ref())
                .map_or_else(ResetModel::default, |r| r.model()),
            sounds: std::mem::take(&mut self.sounds),
            timelines,
            paths,
            scripts: std::mem::take(&mut self.scripts),
            timing: timing?,
        };
        // Step 7 (WP7b): params against the built-ins' specs, on the built definition.
        let mut d = Vec::new();
        let trace = self.trace_mode();
        let _ = pending_call(&mut self.pending, || params::check(&def, trace, &mut d));
        self.diags.append(&mut d);
        // Scripts: structure, calls, name literals and lints (WP9.3).
        for issue in iwbtg_sim::script::validate::validate(&def) {
            let file = def
                .scripts
                .get(issue.script)
                .map_or("scripts", |s| s.name.as_str());
            self.diags.push(Diagnostic {
                file: file.to_owned(),
                line: issue.line,
                column: issue.column,
                severity: match issue.severity {
                    iwbtg_sim::script::validate::Severity::Error => Severity::Error,
                    iwbtg_sim::script::validate::Severity::Warning => Severity::Warning,
                },
                message: issue.message,
            });
        }
        Some(def)
    }

    // -- Step 2: sprites ----------------------------------------------------------------------

    fn load_sprites(&mut self) {
        // A package sprite is keyed by its id; a bundled engine pack sprite
        // (`engine/<pack>/sprites/<id>.toml`) by `engine:<pack>/<id>`, the way packages refer to it.
        let sidecars: Vec<(String, String, String, Option<String>)> = self
            .files
            .iter()
            .filter_map(|(p, _)| {
                if let Some(stem) = p.strip_prefix("sprites/").and_then(|s| s.strip_suffix(".toml")) {
                    return (!stem.contains('/'))
                        .then(|| (stem.to_owned(), stem.to_owned(), p.to_owned(), None));
                }
                let (pack, stem) = crate::engine_pack::sprite_sidecar(p)?;
                Some((
                    format!("{}{pack}/{stem}", crate::ids::ENGINE_PREFIX),
                    stem.to_owned(),
                    p.to_owned(),
                    Some(pack.to_owned()),
                ))
            })
            .collect();
        let mut aliases = Vec::new();
        let mut oversized = Vec::new();
        for (key, id, path, pack) in sidecars {
            let Some(text) = read_text(self.files, &path, &mut self.diags) else {
                continue;
            };
            if let Err(e) = check_id(&id) {
                self.diags
                    .push(Diagnostic::file_error(&path, format!("sprite {e}")));
                continue;
            }
            let (sidecar, spans) = match parse_sidecar(&path, text) {
                Ok(s) => s,
                Err(d) => {
                    self.diags.push(d);
                    continue;
                }
            };
            let png_path = match &pack {
                Some(pack) => format!("{}sprites/{id}.png", crate::engine_pack::pack_dir(pack)),
                None => format!("sprites/{id}.png"),
            };
            let png = self.files.get(&png_path);
            if png.is_none() && sidecar.frames > 0 && sidecar.collision.shape().is_some() {
                self.error(
                    &path,
                    text,
                    spans.frames.clone(),
                    format!("sprite `{id}` has no `{png_path}`"),
                );
                continue;
            }
            let strip_w = u64::from(sidecar.frame_size.0) * u64::from(sidecar.frames);
            if strip_w > u64::from(MAX_IMAGE_SIDE) || sidecar.frame_size.1 > MAX_IMAGE_SIDE {
                if self.trace_mode() {
                    oversized.push(key.clone());
                } else {
                    self.error(
                        &path,
                        text,
                        spans.frames.clone(),
                        format!(
                            "the strip is {strip_w} x {} px; images are at most {MAX_IMAGE_SIDE} px a side",
                            sidecar.frame_size.1
                        ),
                    );
                    continue;
                }
            }
            match load_sprite_at(&png_path[..png_path.len() - 4], png.unwrap_or(&[]), &sidecar) {
                Ok((mut def, masks)) => {
                    let offset = u32::try_from(self.masks.len()).unwrap_or(u32::MAX);
                    def.masks = offset_masks(&def.masks, offset);
                    self.masks.extend(masks);
                    let sid = SpriteId(u32::try_from(self.sprite_ids.len()).unwrap_or(u32::MAX));
                    if let Some(alias) = sidecar.mask.clone() {
                        // A pack sprite's mask alias names a sprite of the same pack.
                        let alias = match &pack {
                            Some(pack) => format!("{}{pack}/{alias}", crate::ids::ENGINE_PREFIX),
                            None => alias,
                        };
                        aliases.push((
                            key.clone(),
                            alias,
                            path.clone(),
                            spans.mask.clone().unwrap_or(0..0),
                        ));
                    }
                    self.sprite_ids.insert(key.clone(), SpriteEntry { id: sid, def });
                }
                Err(d) => self.diags.push(d),
            }
        }
        // Sprite ids follow the key order of the map (package ids and `engine:` keys together).
        for (n, entry) in self.sprite_ids.values_mut().enumerate() {
            entry.id = SpriteId(u32::try_from(n).unwrap_or(u32::MAX));
        }
        for (id, alias, path, span) in aliases {
            match self.sprite_ids.get(&alias).map(|e| e.def.masks.clone()) {
                Some(masks) => {
                    if let Some(e) = self.sprite_ids.get_mut(&id) {
                        e.def.masks = masks;
                    }
                }
                None => {
                    let text = read_text(self.files, &path, &mut Vec::new()).unwrap_or("");
                    self.error(&path, text, span, format!("mask sprite `{alias}` does not exist"));
                }
            }
        }
        for id in oversized {
            self.note("CR15", id);
        }
    }

    fn sprites_in_order(&mut self) -> Vec<SpriteDef> {
        std::mem::take(&mut self.sprite_ids)
            .into_values()
            .map(|e| e.def)
            .collect()
    }

    /// A sprite by id, or an error at `span` of `file`.
    fn sprite_ref(&mut self, name: &str, file: &str, text: &str, span: Range<usize>) -> Option<SpriteId> {
        match self.sprite_ids.get(name) {
            Some(e) => Some(e.id),
            None if name.starts_with(crate::ids::ENGINE_PREFIX) => {
                let msg = match crate::ids::EngineRef::parse(name) {
                    Some(Ok(r))
                        if self
                            .files
                            .contains(&format!("{}engine.toml", crate::engine_pack::pack_dir(r.pack))) =>
                    {
                        format!("`{name}`: the engine pack `{}` has no sprite `{}`", r.pack, r.id)
                    }
                    Some(Ok(r)) => format!(
                        "`{name}`: the engine pack `{}` is not in the package (`iwbtg pkg check` bundles it from `../_engines/{}`)",
                        r.pack, r.pack
                    ),
                    Some(Err(e)) => e,
                    None => format!("sprite `{name}` does not exist"),
                };
                self.error(file, text, span, msg);
                None
            }
            None => {
                self.error(file, text, span, format!("sprite `{name}` does not exist"));
                None
            }
        }
    }

    /// The mask a sprite gives an object-level `mask`: its shared mask, or its first frame's.
    fn mask_of(&self, sprite: SpriteId) -> Option<MaskId> {
        let e = self.sprite_ids.values().find(|e| e.id == sprite)?;
        match &e.def.masks {
            MaskSet::Shared(m) => Some(*m),
            MaskSet::PerFrame(v) => v.first().copied(),
        }
    }

    // -- Step 3: kinds -------------------------------------------------------------------------

    fn push_kind(&mut self, kind: KindDef) -> KindId {
        let id = KindId(u32::try_from(self.kinds.len()).unwrap_or(u32::MAX));
        self.kind_ids.insert(kind.name.clone(), id);
        self.kinds.push(kind);
        id
    }

    fn kind_mut(&mut self, id: KindId) -> Option<&mut KindDef> {
        self.kinds.get_mut(id.index()?)
    }

    /// One kind per package-nameable built-in, in `Builtin::ALL` order, with its defaults.
    fn builtin_kinds(&mut self) {
        for b in Builtin::ALL {
            if Builtin::from_name(b.name()).is_none() {
                continue;
            }
            let mut k = KindDef::new(b.name(), b);
            k.classes = kind_classes(b, &Params::default(), ClassMask::NONE);
            k.solid = objects::default_solid(b);
            self.push_kind(k);
        }
        let manifest = self.m;
        let text = self.manifest_text;
        for (name, d) in &manifest.defaults {
            let Some(&kid) = self.kind_ids.get(name.get_ref()) else {
                self.manifest_error(
                    name.span(),
                    format!("[defaults.{}]: not a built-in kind", name.get_ref()),
                );
                continue;
            };
            let d = d.get_ref();
            let mut out = Defaults::default();
            if let Some(s) = &d.sprite {
                out.sprite = self.sprite_ref(s.get_ref(), MANIFEST_PATH, text, s.span());
            }
            if let Some(s) = &d.mask {
                out.mask = self
                    .sprite_ref(s.get_ref(), MANIFEST_PATH, text, s.span())
                    .and_then(|s| self.mask_of(s));
            }
            for v in &d.variants {
                let mut when = Vec::new();
                let (mut sprite, mut mask) = (None, None);
                for (key, value) in v.get_ref() {
                    match (key.as_str(), value) {
                        ("sprite", toml::Value::String(s)) => {
                            sprite = self.sprite_ref(s, MANIFEST_PATH, text, v.span())
                        }
                        ("mask", toml::Value::String(s)) => {
                            mask = self
                                .sprite_ref(s, MANIFEST_PATH, text, v.span())
                                .and_then(|s| self.mask_of(s));
                        }
                        _ => match param_value(value) {
                            Ok(p) => when.push((key.clone(), p)),
                            Err(e) => self.manifest_error(v.span(), format!("variant param `{key}`: {e}")),
                        },
                    }
                }
                out.variants.push((when, sprite, mask));
            }
            self.apply_defaults(kid, out);
        }
        if self.trace_mode() {
            self.trace_defaults();
        }
    }

    fn apply_defaults(&mut self, kind: KindId, d: Defaults) {
        if let Some(k) = self.kind_mut(kind) {
            k.sprite = d.sprite.or(k.sprite);
            k.mask = d.mask.or(k.mask);
        }
        self.defaults.insert(kind, d);
    }

    /// Trace mode: default sprites of bare built-ins from the trace map's `builtin` rows (CR12).
    fn trace_defaults(&mut self) {
        let Some(trace) = &self.trace else {
            return;
        };
        let mut rows: Vec<(u32, String, Selector, String)> = trace
            .objects
            .values()
            .filter(|o| o.status == TraceStatus::Builtin && !o.kind.starts_with('@'))
            .filter_map(|o| {
                let sprite = o.sprite.clone()?;
                let params = o
                    .params
                    .iter()
                    .map(|(k, v)| (k.clone(), trace_param(v)))
                    .collect();
                Some((o.index.unwrap_or(u32::MAX), o.kind.clone(), params, sprite))
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut found: BTreeMap<KindId, Defaults> = BTreeMap::new();
        for (_, kind, params, sprite) in rows {
            let Some(&kid) = self.kind_ids.get(&kind) else {
                continue;
            };
            if self.defaults.contains_key(&kid) {
                continue;
            }
            let Some(sid) = self.sprite_ids.get(&sprite).map(|e| e.id) else {
                continue;
            };
            let d = found.entry(kid).or_default();
            if params.is_empty() {
                d.sprite.get_or_insert(sid);
            } else if !d.variants.iter().any(|(w, _, _)| *w == params) {
                d.variants.push((params, Some(sid), None));
            }
        }
        for (kid, d) in found {
            let name = self
                .kinds
                .get(kid.index().unwrap_or(usize::MAX))
                .map(|k| k.name.clone());
            self.note("CR12", name.unwrap_or_default());
            self.apply_defaults(kid, d);
        }
    }

    /// Trace mode: the kid's mask from the legacy physics.toml `mask = { sprite = ... }` (CR8).
    /// Resolves the `[sprites]` names of a loaded profile to sprite ids.
    fn finish_physics(&mut self, l: physics::PhysicsLoad, path: &str) -> PhysicsProfile {
        let mut p = l.profile;
        let text = read_text(self.files, path, &mut Vec::new()).unwrap_or("");
        let pick = |this: &mut Self, name: &Option<String>| -> Option<SpriteId> {
            let name = name.as_ref()?;
            let at = text.find(name.as_str()).unwrap_or(0);
            this.sprite_ref(name, path, text, at..at + name.len())
        };
        if l.legacy {
            // A legacy draft names no kid sprites: the converter's names, when the package has them.
            let find = |this: &Self, name: &str| this.sprite_ids.get(name).map(|e| e.id);
            p.sprites.idle = find(self, "player_idle");
            p.sprites.running = find(self, "player_running");
            p.sprites.jump = find(self, "player_jump");
            p.sprites.fall = find(self, "player_fall");
            p.sprites.sliding = find(self, "player_sliding");
            return p;
        }
        p.sprites.idle = pick(self, &l.sprites.idle);
        p.sprites.running = pick(self, &l.sprites.running);
        p.sprites.jump = pick(self, &l.sprites.jump);
        p.sprites.fall = pick(self, &l.sprites.fall);
        p.sprites.sliding = pick(self, &l.sprites.sliding);
        // `[sounds]`: names resolved against the package's sounds.
        let names = l.sounds;
        let snd = |this: &mut Self, name: &Option<String>| -> Option<iwbtg_sim::def::SoundId> {
            let name = name.as_ref()?;
            let found = this
                .sounds
                .iter()
                .position(|d| d.name == *name)
                .and_then(|i| u32::try_from(i).ok());
            if found.is_none() {
                let at = text.find(name.as_str()).unwrap_or(0);
                this.error(
                    path,
                    text,
                    at..at + name.len(),
                    format!("sound `{name}` does not exist"),
                );
            }
            found.map(iwbtg_sim::def::SoundId)
        };
        p.sounds.jump = snd(self, &names.jump);
        p.sounds.double_jump = snd(self, &names.double_jump);
        p.sounds.shoot = snd(self, &names.shoot);
        p.sounds.vine_kick = snd(self, &names.vine_kick);
        p.sounds.death = snd(self, &names.death);
        p
    }

    fn legacy_kid_mask(&mut self) {
        let Some(&kid) = self.kind_ids.get(Builtin::Kid.name()) else {
            return;
        };
        if self
            .kinds
            .get(kid.index().unwrap_or(usize::MAX))
            .is_some_and(|k| k.mask.is_some())
        {
            return;
        }
        let Some(text) = read_text(self.files, self.m.physics.get_ref(), &mut Vec::new()) else {
            return;
        };
        let Ok(table) = toml::from_str::<toml::Table>(text) else {
            return;
        };
        let Some(sprite) = table
            .get("mask")
            .and_then(|m| m.get("sprite"))
            .and_then(toml::Value::as_str)
        else {
            return;
        };
        let Some(sid) = self.sprite_ids.get(sprite).map(|e| e.id) else {
            return;
        };
        let mask = self.mask_of(sid);
        if let Some(k) = self.kind_mut(kid) {
            k.mask = mask;
        }
        self.note("CR8", format!("kid mask `{sprite}`"));
    }

    fn load_objects(&mut self) {
        let paths: Vec<(String, String)> = self
            .files
            .iter()
            .filter_map(|(p, _)| {
                let stem = p.strip_prefix("objects/")?.strip_suffix(".toml")?;
                (!stem.contains('/')).then(|| (stem.to_owned(), p.to_owned()))
            })
            .collect();
        for (id, path) in paths {
            let Some(text) = read_text(self.files, &path, &mut self.diags) else {
                continue;
            };
            if let Err(e) = check_id(&id) {
                self.diags
                    .push(Diagnostic::file_error(&path, format!("object {e}")));
                continue;
            }
            match objects::parse_object(&id, &path, text) {
                Ok(o) => {
                    self.object_kind(&o, &path, text);
                    let name = format!("@{id}");
                    if !self.kind_ids.contains_key(&name) && !self.dropped.contains(&name) {
                        self.failed.insert(name);
                    }
                }
                Err(d) => self.diags.push(d),
            }
        }
    }

    /// The trace map's status for `@id`.
    fn trace_status(&self, kind: &str) -> Option<TraceStatus> {
        self.trace.as_ref()?.object_for_kind(kind).map(|(_, o)| o.status)
    }

    fn object_kind(&mut self, o: &ObjectFile, path: &str, text: &str) {
        let name = format!("@{}", o.id);
        let script_name = o.script.as_ref().map(|s| s.get_ref().as_str());
        // CR3: engine plumbing never loads as a script.
        let infra = matches!(script_name, Some("world" | "init"))
            || self.trace_status(&name) == Some(TraceStatus::Infra);
        let mut base = None;
        let mut script = None;
        if infra {
            let engine = [o.script.as_ref(), o.base.as_ref()]
                .into_iter()
                .flatten()
                .filter_map(|s| Builtin::from_name(s.get_ref()))
                .find(|b| engine_builtin(*b));
            match engine {
                Some(b) => {
                    self.note("CR3", format!("{name} as built-in `{}`", b.name()));
                    base = Some(b);
                }
                None => {
                    self.note("CR3", format!("{name} dropped"));
                    self.dropped.insert(name);
                    return;
                }
            }
        } else {
            if let Some(b) = &o.base {
                match Builtin::from_name(b.get_ref()) {
                    Some(builtin) => base = Some(builtin),
                    None if self.trace_mode() => {
                        self.note("CR10", format!("{name} base `{}`", b.get_ref()));
                        base = Some(Builtin::Inert);
                    }
                    None => {
                        self.error(
                            path,
                            text,
                            b.span(),
                            format!("`{}` is not a built-in kind", b.get_ref()),
                        );
                        return;
                    }
                }
            }
            if let Some(s) = &o.script {
                let script_path = format!("scripts/{}.rhai", s.get_ref());
                if let Err(e) = check_id(s.get_ref()) {
                    self.error(path, text, s.span(), format!("script {e}"));
                    return;
                }
                match read_text(self.files, &script_path, &mut Vec::new()) {
                    Some(source) if source.len() > MAX_SCRIPT_BYTES => {
                        self.error(path, text, s.span(), format!("`{script_path}` is over 256 KB"));
                        return;
                    }
                    Some(source) => {
                        let sid = ScriptId(u32::try_from(self.scripts.len()).unwrap_or(u32::MAX));
                        self.scripts.push(ScriptDef {
                            name: script_path,
                            source: source.to_owned(),
                        });
                        script = Some(sid);
                        base.get_or_insert(Builtin::Scripted);
                    }
                    None => match self.mode {
                        LoadMode::Trace {
                            unmapped_kinds_inert: true,
                        } => {
                            self.note("CR1, CR2", name.clone());
                            base = Some(Builtin::Inert);
                        }
                        _ => {
                            self.error(path, text, s.span(), format!("`{script_path}` does not exist"));
                            return;
                        }
                    },
                }
            }
        }
        let Some(base) = base else {
            self.diags
                .push(Diagnostic::file_error(path, "an object needs `base` or `script`"));
            return;
        };
        let mut kind = KindDef::new(name.clone(), base);
        kind.script = script;
        if let Some(s) = &o.sprite {
            kind.sprite = self.sprite_ref(s.get_ref(), path, text, s.span());
        }
        if let Some(s) = &o.mask {
            kind.mask = self
                .sprite_ref(s.get_ref(), path, text, s.span())
                .and_then(|s| self.mask_of(s));
        }
        kind.solid = o.solid.unwrap_or_else(|| objects::default_solid(kind.base));
        kind.visible = o.visible.unwrap_or(true);
        kind.depth = o.depth.as_ref().map_or(Real::ZERO, |d| Real::new(*d.get_ref()));
        kind.persistent = o.persistent.unwrap_or(false);
        for (key, value) in &o.params {
            match param_value(value.get_ref()) {
                Ok(p) => {
                    kind.params.0.insert(key.clone(), p);
                }
                Err(e) => self.error(path, text, value.span(), format!("param `{key}`: {e}")),
            }
        }
        let mut extra = ClassMask::NONE;
        for c in &o.counts_as {
            match self.class_named(c.get_ref()) {
                Some(m) => extra |= m,
                None => self.error(
                    path,
                    text,
                    c.span(),
                    format!("too many package classes for `{}` (at most 16)", c.get_ref()),
                ),
            }
        }
        if !o.roles.is_empty() {
            if self.trace_mode() {
                self.note("CR11", name.clone());
                for r in &o.roles {
                    extra |= legacy_role_classes(r.get_ref()).unwrap_or(ClassMask::NONE);
                }
            } else {
                self.error(
                    path,
                    text,
                    o.roles[0].span(),
                    "`roles` is not a v0 key; use `counts_as`",
                );
            }
        }
        kind.classes = kind_classes(base, &kind.params, extra);
        self.push_kind(kind);
    }

    /// A named class, or the next package class bit for a new name.
    fn class_named(&mut self, name: &str) -> Option<ClassMask> {
        if let Some(c) = ClassMask::from_name(name) {
            return Some(c);
        }
        let n = self
            .package_classes
            .iter()
            .position(|c| c == name)
            .unwrap_or_else(|| {
                self.package_classes.push(name.to_owned());
                self.package_classes.len() - 1
            });
        ClassMask::package(u32::try_from(n).ok()?)
    }

    /// Resolves kind params whose spec type names a room, kind or sound.
    fn resolve_kind_params(&mut self) {
        for i in 0..self.kinds.len() {
            let (base, params, name) = {
                let k = &self.kinds[i];
                (k.base, k.params.clone(), k.name.clone())
            };
            let file = name.strip_prefix('@').map(|id| format!("objects/{id}.toml"));
            let mut resolved = Params::default();
            for (key, value) in params.0 {
                let v = self.resolve_param(base, &key, value, file.as_deref().unwrap_or(MANIFEST_PATH));
                resolved.0.insert(key, v);
            }
            self.kinds[i].params = resolved;
        }
    }

    /// The param's spec type, if the base declares one.
    fn spec_type(&mut self, base: Builtin, key: &str) -> Option<ParamType> {
        let specs = match self.spec_cache.get(&base) {
            Some(s) => *s,
            None => {
                let s = pending_call(&mut self.pending, || builtins::param_specs(base));
                self.spec_cache.insert(base, s);
                s
            }
        };
        specs?.iter().find(|s| s.name == key).map(|s| s.ty)
    }

    /// Turns a text param into a room, kind or sound reference when the base's spec says so.
    fn resolve_param(&mut self, base: Builtin, key: &str, value: ParamValue, file: &str) -> ParamValue {
        let ParamValue::Str(s) = &value else {
            return value;
        };
        let found = match self.spec_type(base, key) {
            Some(ParamType::Room) => self.room_ids.get(s).copied().map(ParamValue::Room),
            Some(ParamType::Kind) => self.kind_by_ref(s).map(ParamValue::Kind),
            Some(ParamType::Sound) => self
                .sounds
                .iter()
                .position(|d| d.name == *s)
                .and_then(|i| u32::try_from(i).ok())
                .map(|i| ParamValue::Sound(iwbtg_sim::def::SoundId(i))),
            _ => return value,
        };
        match found {
            Some(v) => v,
            None => {
                self.diags.push(Diagnostic::file_error(
                    file,
                    format!("param `{key}`: `{s}` does not exist"),
                ));
                value
            }
        }
    }

    /// A kind by reference (`@id` or a bare built-in). In Trace mode an unknown bare name becomes
    /// an inert kind of that name (CR10).
    fn kind_by_ref(&mut self, name: &str) -> Option<KindId> {
        if let Some(&k) = self.kind_ids.get(name) {
            return Some(k);
        }
        if name.starts_with('@') || !self.trace_mode() || check_id(name).is_err() {
            return None;
        }
        self.note("CR10", name.to_owned());
        Some(self.push_kind(KindDef::new(name, Builtin::Inert)))
    }

    // -- Step 5: object_order ------------------------------------------------------------------

    fn load_order(&mut self) {
        let mut entries = Vec::new();
        let manifest = self.m;
        for item in manifest.object_order.get_ref() {
            let Some(name) = item.get_ref().kind() else {
                continue;
            };
            if self.dropped.contains(name) || self.failed.contains(name) {
                continue;
            }
            if self.kind_by_ref(name).is_none() {
                self.manifest_error(
                    item.span(),
                    format!("object_order names `{name}`, which is not a kind of this package"),
                );
                continue;
            }
            let variant = match item.get_ref().selector() {
                Ok(Some((key, value))) => match param_value(value) {
                    Ok(v) => Some(VariantSel {
                        key: key.to_owned(),
                        value: v,
                    }),
                    Err(e) => {
                        self.manifest_error(item.span(), e);
                        continue;
                    }
                },
                Ok(None) => None,
                Err(e) => {
                    self.manifest_error(item.span(), e);
                    continue;
                }
            };
            entries.push(OrderEntry {
                kind: name.to_owned(),
                variant,
            });
        }
        match resolve_ranks(&entries, &self.kinds) {
            Ok(t) => self.ranks = t,
            Err(d) => self.diags.push(d),
        }
    }

    /// The rank of an instance of `kind` with these params, or a CR4 rank after the listed
    /// entries (Trace) or an error (Strict, reported once per kind by the caller).
    fn rank_or_append(&mut self, kind: KindId, params: &[&Params]) -> Option<u16> {
        let rank_kind = self.rank_kind.get(&kind).copied().unwrap_or(kind);
        let kind_params = self
            .kinds
            .get(kind.index()?)
            .map(|k| k.params.clone())
            .unwrap_or_default();
        let mut all: Vec<&Params> = vec![&kind_params];
        all.extend_from_slice(params);
        if let Some(r) = rank_of(&self.ranks, rank_kind, &all) {
            if rank_kind != kind && !self.ranks.entries.iter().any(|e| e.kind == kind) {
                self.ranks.entries.push(RankEntry {
                    kind,
                    variant: None,
                    rank: r,
                });
            }
            return Some(r);
        }
        let name = self.kinds.get(kind.index()?)?.name.clone();
        if !self.trace_mode() {
            return None;
        }
        let next = self
            .ranks
            .entries
            .iter()
            .map(|e| e.rank)
            .max()
            .map_or(0, |r| r.saturating_add(1));
        self.ranks.entries.push(RankEntry {
            kind: rank_kind,
            variant: None,
            rank: next,
        });
        if rank_kind != kind {
            self.ranks.entries.push(RankEntry {
                kind,
                variant: None,
                rank: next,
            });
        }
        self.appended.insert(rank_kind);
        self.note("CR4", name);
        Some(next)
    }

    // -- Step 4: rooms -------------------------------------------------------------------------

    fn parse_rooms(&mut self) -> Vec<ParsedRoom<'a>> {
        let files = self.files;
        let mut rooms = Vec::new();
        for (p, _) in files.iter() {
            let Some(stem) = p.strip_prefix("rooms/").and_then(|s| s.strip_suffix(".toml")) else {
                continue;
            };
            if stem.contains('/') || stem.contains('.') {
                continue; // `<id>.tiles.toml`: decorative, never the sim's.
            }
            let Some(text) = read_text(files, p, &mut self.diags) else {
                continue;
            };
            if let Err(e) = check_id(stem) {
                self.diags.push(Diagnostic::file_error(p, format!("room {e}")));
                continue;
            }
            match parse_room(stem, p, text) {
                Ok(file) => rooms.push(ParsedRoom {
                    path: p.to_owned(),
                    text,
                    file,
                }),
                Err(d) => self.diags.push(d),
            }
        }
        // Room order: `room_order`, else (Trace) the trace map's original order, then path order.
        let mut order: Vec<String> = Vec::new();
        if let Some(list) = &self.m.room_order {
            for r in list.get_ref() {
                if !rooms.iter().any(|p| p.file.id == *r.get_ref()) {
                    self.manifest_error(
                        r.span(),
                        format!("room_order names `{}`, which has no room file", r.get_ref()),
                    );
                } else if order.contains(r.get_ref()) {
                    self.manifest_error(r.span(), format!("room_order lists `{}` twice", r.get_ref()));
                } else {
                    order.push(r.get_ref().clone());
                }
            }
        } else if let Some(trace) = &self.trace {
            let mut by_order: Vec<(u32, String)> = trace
                .rooms
                .values()
                .filter_map(|r| Some((r.order?, r.id.clone())))
                .filter(|(_, id)| rooms.iter().any(|p| p.file.id == *id))
                .collect();
            by_order.sort();
            by_order.dedup_by(|a, b| a.1 == b.1);
            if !by_order.is_empty() {
                self.note("CR14", format!("{} rooms", by_order.len()));
            }
            order.extend(by_order.into_iter().map(|(_, id)| id));
        }
        for r in &rooms {
            if !order.contains(&r.file.id) {
                order.push(r.file.id.clone());
            }
        }
        rooms.sort_by_key(|r| order.iter().position(|o| *o == r.file.id).unwrap_or(usize::MAX));
        for (i, r) in rooms.iter().enumerate() {
            self.room_ids
                .insert(r.file.id.clone(), RoomId(u32::try_from(i).unwrap_or(u32::MAX)));
        }
        rooms
    }

    fn room_ref(&mut self, name: &str, span: Range<usize>) -> Option<RoomId> {
        let r = self.room_ids.get(name).copied();
        if r.is_none() {
            self.manifest_error(span, format!("room `{name}` does not exist"));
        }
        r
    }

    /// Step 6: every variable name the loader can see, sorted (plan 1.1).
    fn intern(&self, rooms: &[ParsedRoom<'_>]) -> BTreeMap<String, iwbtg_sim::def::VarId> {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for k in &self.kinds {
            names.extend(k.params.0.keys().cloned());
        }
        for r in rooms {
            for i in &r.file.instances {
                names.extend(i.params.keys().cloned());
            }
            for a in &r.file.on_create {
                match a.get_ref() {
                    ActionFile::SetGlobal { var, .. } => {
                        names.insert(var.clone());
                    }
                    ActionFile::Spawn { params, .. } => names.extend(params.keys().cloned()),
                    ActionFile::LoadGame | ActionFile::DestroySelf => {}
                }
            }
        }
        intern_vars(names.into_iter()).ids
    }

    fn build_rooms(
        &mut self,
        rooms: &[ParsedRoom<'_>],
        vars: &BTreeMap<String, iwbtg_sim::def::VarId>,
    ) -> Vec<RoomDef> {
        let mut out = Vec::with_capacity(rooms.len());
        let mut missing_role = 0usize;
        let mut traced_speed = 0usize;
        let mut unranked: BTreeSet<KindId> = BTreeSet::new();
        for r in rooms {
            let f = &r.file;
            let (path, text) = (r.path.as_str(), r.text);
            let mut def = RoomDef::new(f.id.clone(), f.size.0, f.size.1);
            def.persistent = f.persistent;
            def.role = match f.role {
                Some(role) => role.role(),
                None => {
                    missing_role += 1;
                    RoomRole::Play
                }
            };
            def.file_speed = match &f.file_speed {
                Some(s) => *s.get_ref(),
                None => match self
                    .trace
                    .as_ref()
                    .and_then(|t| t.room_by_id(&f.id))
                    .and_then(|(_, t)| t.speed)
                {
                    Some(speed) if speed > 0 => {
                        traced_speed += 1;
                        speed
                    }
                    _ => self.m.tick_rate(),
                },
            };
            def.views_enabled = f.views_enabled;
            for (n, v) in f.views.iter().enumerate() {
                let view = self.view(v.get_ref(), path, text, v.span());
                if let Some(slot) = def.views.get_mut(n) {
                    *slot = view;
                }
            }
            let mut seen_names = BTreeSet::new();
            let mut dropped = 0usize;
            for inst in &f.instances {
                match self.placed(inst, path, text, &mut seen_names, &mut unranked) {
                    Placed::Instances(mut list) => def.instances.append(&mut list),
                    Placed::Dropped => dropped += inst.positions().count(),
                    Placed::Failed => {}
                }
            }
            if dropped > 0 {
                self.note("CR3", format!("{} instances removed from {}", dropped, f.id));
            }
            for a in &f.on_create {
                if let Some(action) = self.action(a.get_ref(), vars, path, text, a.span()) {
                    def.on_create.push(action);
                }
            }
            out.push(def);
        }
        if missing_role > 0 {
            self.trace_note("CR6", format!("{missing_role} rooms"));
        }
        if traced_speed > 0 {
            self.note("CR13", format!("{traced_speed} rooms"));
        }
        out
    }

    fn view(&mut self, v: &crate::rooms::ViewFile, path: &str, text: &str, span: Range<usize>) -> ViewDef {
        let r = |n: &crate::number::Number| Real::new(n.0);
        let [vx, vy, vw, vh] = &v.view;
        let port = v
            .port
            .as_ref()
            .map_or((Real::ZERO, Real::ZERO, r(vw), r(vh)), |[a, b, c, d]| {
                (r(a), r(b), r(c), r(d))
            });
        let follow = match &v.follow {
            None => None,
            Some(name) => {
                let k = self.kind_by_ref(name.get_ref());
                if k.is_none() {
                    self.error(
                        path,
                        text,
                        name.span(),
                        format!("view follows `{}`, which is not a kind", name.get_ref()),
                    );
                }
                k
            }
        };
        let _ = span;
        let (hb, vb) = v.border.as_ref().map_or((32.0, 32.0), |[a, b]| (a.0, b.0));
        let (hs, vs) = v.speed.as_ref().map_or((-1.0, -1.0), |[a, b]| (a.0, b.0));
        ViewDef {
            visible: v.visible,
            view: (r(vx), r(vy), r(vw), r(vh)),
            port,
            follow,
            hborder: Real::new(hb),
            vborder: Real::new(vb),
            hspeed: Real::new(hs),
            vspeed: Real::new(vs),
        }
    }

    /// The kind an instance uses: `@id`, a bare built-in or (bare built-in with matching variant
    /// defaults) a derived variant kind. `Err(true)` when the kind was dropped (CR3).
    fn instance_kind(&mut self, name: &str, params: &Params) -> Result<KindId, bool> {
        if self.dropped.contains(name) {
            return Err(true);
        }
        let Some(kind) = self.kind_by_ref(name) else {
            return Err(false);
        };
        let Some(defaults) = self.defaults.get(&kind) else {
            return Ok(kind);
        };
        let kind_params = self
            .kinds
            .get(kind.index().unwrap_or(usize::MAX))
            .map(|k| k.params.clone())
            .unwrap_or_default();
        let matches = |when: &[(String, ParamValue)]| {
            when.iter().all(|(k, v)| {
                params
                    .0
                    .get(k)
                    .or_else(|| kind_params.0.get(k))
                    .is_some_and(|p| param_matches(p, v))
            })
        };
        let Some((index, (when, sprite, mask))) = defaults
            .variants
            .iter()
            .enumerate()
            .find(|(_, (w, _, _))| matches(w))
        else {
            return Ok(kind);
        };
        if let Some(&d) = self.derived.get(&(kind, index)) {
            return Ok(d);
        }
        let (when, sprite, mask) = (when.clone(), *sprite, *mask);
        let Some(mut derived) = self.kinds.get(kind.index().unwrap_or(usize::MAX)).cloned() else {
            return Ok(kind);
        };
        let tag: Vec<String> = when
            .iter()
            .map(|(k, v)| format!("{k}={}", show_param(v)))
            .collect();
        derived.name = format!("{}[{}]", derived.name, tag.join(","));
        derived.sprite = sprite.or(derived.sprite);
        derived.mask = mask.or(derived.mask);
        for (k, v) in when {
            derived.params.0.insert(k, v);
        }
        derived.classes = kind_classes(derived.base, &derived.params, ClassMask::NONE);
        let id = self.push_kind(derived);
        self.derived.insert((kind, index), id);
        self.rank_kind.insert(id, kind);
        Ok(id)
    }

    fn placed(
        &mut self,
        inst: &InstanceFile,
        path: &str,
        text: &str,
        seen_names: &mut BTreeSet<String>,
        unranked: &mut BTreeSet<KindId>,
    ) -> Placed {
        let mut params = Params::default();
        for (key, value) in &inst.params {
            match param_value(value.get_ref()) {
                Ok(p) => {
                    params.0.insert(key.clone(), p);
                }
                Err(e) => {
                    self.error(path, text, value.span(), format!("param `{key}`: {e}"));
                    return Placed::Failed;
                }
            }
        }
        if self.failed.contains(inst.kind.get_ref()) {
            return Placed::Failed;
        }
        let kind = match self.instance_kind(inst.kind.get_ref(), &params) {
            Ok(k) => k,
            Err(true) => return Placed::Dropped,
            Err(false) => {
                self.error(
                    path,
                    text,
                    inst.kind.span(),
                    format!("`{}` is not a kind of this package", inst.kind.get_ref()),
                );
                return Placed::Failed;
            }
        };
        let base = self
            .kinds
            .get(kind.index().unwrap_or(usize::MAX))
            .map_or(Builtin::Inert, |k| k.base);
        let mut resolved = Params::default();
        for (key, value) in params.0 {
            let v = self.resolve_param(base, &key, value, path);
            resolved.0.insert(key, v);
        }
        let Some(rank) = self.rank_or_append(kind, &[&resolved]) else {
            if unranked.insert(kind) {
                let name = self
                    .kinds
                    .get(kind.index().unwrap_or(usize::MAX))
                    .map(|k| k.name.clone())
                    .unwrap_or_default();
                self.error(
                    path,
                    text,
                    inst.kind.span(),
                    format!("`{name}` is placed but object_order does not list it"),
                );
            }
            return Placed::Failed;
        };
        let name = match &inst.name {
            None => None,
            Some(n) => {
                if !seen_names.insert(n.get_ref().clone()) {
                    self.error(
                        path,
                        text,
                        n.span(),
                        format!("instance name `{}` is used twice in this room", n.get_ref()),
                    );
                }
                if inst.count() > 1 {
                    self.error(path, text, n.span(), "a named instance cannot have `run`");
                }
                Some(NameId(intern_in(&mut self.names, n.get_ref())))
            }
        };
        let group = inst
            .group
            .as_ref()
            .map(|g| GroupId(intern_in(&mut self.groups, g.get_ref())));
        let list = inst
            .positions()
            .map(|(x, y)| PlacedInstance {
                kind,
                rank,
                x: Real::new(x),
                y: Real::new(y),
                xscale: Real::new(inst.xscale),
                yscale: Real::new(inst.yscale),
                angle: Real::new(inst.angle),
                params: resolved.clone(),
                name,
                group,
            })
            .collect();
        Placed::Instances(list)
    }

    fn action(
        &mut self,
        a: &ActionFile,
        vars: &BTreeMap<String, iwbtg_sim::def::VarId>,
        path: &str,
        text: &str,
        span: Range<usize>,
    ) -> Option<RoomAction> {
        match a {
            ActionFile::SetGlobal { var, value } => {
                let value = match value {
                    toml::Value::String(s) => Value::Str(s.clone()),
                    toml::Value::Boolean(b) => Value::Real(if *b { Real::ONE } else { Real::ZERO }),
                    other => match objects::number(other) {
                        Ok(n) => Value::Real(Real::new(n)),
                        Err(e) => {
                            self.error(path, text, span, format!("set_global `{var}`: {e}"));
                            return None;
                        }
                    },
                };
                let var = *vars.get(var)?;
                Some(RoomAction::SetGlobal { var, value })
            }
            ActionFile::Spawn { kind, x, y, params } => {
                let Some(k) = self.kind_by_ref(kind).filter(|_| !self.dropped.contains(kind)) else {
                    self.error(
                        path,
                        text,
                        span,
                        format!("spawn: `{kind}` is not a kind of this package"),
                    );
                    return None;
                };
                let base = self.kinds.get(k.index()?)?.base;
                let mut p = Params::default();
                for (key, value) in params {
                    match param_value(value) {
                        Ok(v) => {
                            let v = self.resolve_param(base, key, v, path);
                            p.0.insert(key.clone(), v);
                        }
                        Err(e) => self.error(path, text, span.clone(), format!("spawn param `{key}`: {e}")),
                    }
                }
                Some(RoomAction::Spawn {
                    kind: k,
                    x: Real::new(x.0),
                    y: Real::new(y.0),
                    params: p,
                })
            }
            ActionFile::LoadGame => Some(RoomAction::LoadGame),
            ActionFile::DestroySelf => Some(RoomAction::DestroySelf),
        }
    }

    // -- Engine kinds, boot, spawnable ranks ---------------------------------------------------

    fn engine_kinds(&mut self) -> BTreeMap<EngineKind, KindId> {
        let manifest = self.m;
        let roles = manifest.engine.as_ref().map(|e| &e.get_ref().roles);
        if let Some(roles) = roles {
            for key in roles.keys() {
                if !ENGINE_ROLES.contains(&key.get_ref().as_str()) {
                    self.manifest_error(
                        key.span(),
                        format!(
                            "`{}` is not an engine role ({})",
                            key.get_ref(),
                            ENGINE_ROLES.join(", ")
                        ),
                    );
                }
            }
        }
        let mut out = BTreeMap::new();
        for (role, ek, builtin) in engine_roles() {
            let set = roles
                .and_then(|r| r.iter().find(|(k, _)| k.get_ref() == role))
                .map(|(_, v)| v);
            let kind = match set {
                Some(v) if v.get_ref() == "none" => None,
                Some(v) => {
                    let k = self
                        .kind_by_ref(v.get_ref())
                        .filter(|_| !self.dropped.contains(v.get_ref()));
                    if k.is_none() {
                        self.manifest_error(
                            v.span(),
                            format!("engine role `{role}`: `{}` is not a kind", v.get_ref()),
                        );
                    }
                    k
                }
                None => builtin.and_then(|b| self.default_engine_kind(b)),
            };
            if let Some(k) = kind {
                out.insert(ek, k);
            }
        }
        out
    }

    /// The kind an engine role uses by default: the one package object with that base (`@bullet`
    /// for `kid_bullet`, `@world` after CR3), else the earliest-ranked of several, else the bare
    /// built-in.
    fn default_engine_kind(&self, b: Builtin) -> Option<KindId> {
        let candidates: Vec<KindId> = self
            .kinds
            .iter()
            .enumerate()
            .filter(|(_, k)| k.base == b && k.name.starts_with('@'))
            .filter_map(|(i, _)| u32::try_from(i).ok().map(KindId))
            .collect();
        match candidates.as_slice() {
            [] => self.kind_ids.get(b.name()).copied(),
            [one] => Some(*one),
            many => many
                .iter()
                .copied()
                .min_by_key(|k| self.ranks.rank_for(*k, None).unwrap_or(u16::MAX)),
        }
    }

    fn boot(&mut self, engine: &BTreeMap<EngineKind, KindId>) -> Vec<KindId> {
        let manifest = self.m;
        match manifest.engine.as_ref().and_then(|e| e.get_ref().boot.as_ref()) {
            Some(list) => list
                .iter()
                .filter_map(|b| {
                    let k = self.kind_by_ref(b.get_ref());
                    if k.is_none() {
                        self.manifest_error(b.span(), format!("boot kind `{}` does not exist", b.get_ref()));
                    }
                    k
                })
                .collect(),
            None => engine.get(&EngineKind::World).copied().into_iter().collect(),
        }
    }

    /// CR4: engine kinds, boot kinds, kinds that room actions spawn and kind-typed params must
    /// have a rank.
    fn spawnable_ranks(&mut self, rooms: &[RoomDef], engine: &BTreeMap<EngineKind, KindId>) {
        let mut need: BTreeSet<KindId> = engine.values().copied().collect();
        for r in rooms {
            for a in &r.on_create {
                if let RoomAction::Spawn { kind, .. } = a {
                    need.insert(*kind);
                }
            }
            for i in &r.instances {
                need.extend(i.params.0.values().filter_map(kind_param));
            }
        }
        for k in &self.kinds {
            need.extend(k.params.0.values().filter_map(kind_param));
        }
        if let Some(boot) = self.m.engine.as_ref().and_then(|e| e.get_ref().boot.as_ref()) {
            need.extend(
                boot.iter()
                    .filter_map(|b| self.kind_ids.get(b.get_ref()).copied()),
            );
        }
        for kind in need {
            if self.ranks.entries.iter().any(|e| e.kind == kind) {
                continue;
            }
            if self.rank_or_append(kind, &[]).is_none() {
                let name = self
                    .kinds
                    .get(kind.index().unwrap_or(usize::MAX))
                    .map(|k| k.name.clone())
                    .unwrap_or_default();
                let span = self.m.object_order.span();
                self.manifest_error(
                    span,
                    format!("`{name}` can be spawned but object_order does not list it (CR4)"),
                );
            }
        }
    }

    fn event_sets(&mut self, physics: &PhysicsProfile) {
        for i in 0..self.kinds.len() {
            let kind = self.kinds[i].clone();
            let set = if kind.base == Builtin::Kid {
                pending_call(&mut self.pending, || iwbtg_sim::kid::event_set(physics))
            } else {
                pending_call(&mut self.pending, || builtins::event_set(kind.base, &kind))
            };
            if let Some(set) = set {
                self.kinds[i].events = set;
            }
        }
    }

    // -- [timing] ------------------------------------------------------------------------------

    fn timing(&mut self) -> Option<TimingDef> {
        let t = self.m.timing.get_ref();
        let start = self.trigger(&t.start.trigger, t.start.room.as_ref());
        let end = self.trigger(&t.end.trigger, t.end.room.as_ref());
        let mut stages = Vec::new();
        for s in &t.stages {
            stages.push(self.trigger(&s.trigger, s.room.as_ref()));
        }
        Some(TimingDef {
            start: start?,
            end: end?,
            stages: stages.into_iter().collect::<Option<Vec<_>>>()?,
        })
    }

    fn trigger(&mut self, name: &Spanned<String>, room: Option<&Spanned<String>>) -> Option<TimingTrigger> {
        match (name.get_ref().as_str(), room) {
            ("run_start", None) => Some(TimingTrigger::RunStart),
            ("run_end", None) => Some(TimingTrigger::RunEnd),
            ("enter_room", Some(r)) => self.room_ref(r.get_ref(), r.span()).map(TimingTrigger::EnterRoom),
            ("enter_room", None) => {
                self.manifest_error(name.span(), "`enter_room` needs `room = \"<room id>\"`");
                None
            }
            ("run_start" | "run_end", Some(r)) => {
                self.manifest_error(r.span(), format!("`{}` takes no `room`", name.get_ref()));
                None
            }
            (other, _) => {
                self.manifest_error(
                    name.span(),
                    format!(
                        "timing trigger `{other}` is not in the vocabulary ({})",
                        TIMING_TRIGGERS.join(", ")
                    ),
                );
                None
            }
        }
    }

    // -- Step 8 ----------------------------------------------------------------------------------

    /// Trace mode: a placed instance whose built-in refuses its params (a warp with no
    /// `room_to`, CR5) would stop the sim at room start. It becomes an instance of an inert twin
    /// of its kind (same sprite, mask, solid and classes; no events, no params), as an unmapped
    /// kind does. `params::check` already warns about the params.
    fn inert_twins(&mut self, rooms: &mut [RoomDef]) {
        let mut twins: BTreeMap<KindId, KindId> = BTreeMap::new();
        for room in rooms.iter_mut() {
            for inst in &mut room.instances {
                let Some(kind) = self.kinds.get(inst.kind.index().unwrap_or(usize::MAX)) else {
                    continue;
                };
                if matches!(
                    kind.base,
                    Builtin::Kid | Builtin::Inert | Builtin::Scripted | Builtin::Probe
                ) {
                    continue;
                }
                let mut merged = kind.params.clone();
                for (k, v) in &inst.params.0 {
                    merged.0.insert(k.clone(), v.clone());
                }
                let base = kind.base;
                let refused = matches!(
                    pending_call(&mut self.pending, || builtins::init_state(base, &merged)),
                    Some(Err(_))
                );
                if !refused {
                    continue;
                }
                let twin = match twins.get(&inst.kind) {
                    Some(t) => *t,
                    None => {
                        let mut t = kind.clone();
                        t.name = format!("{}[inert]", t.name);
                        t.base = Builtin::Inert;
                        t.params = Params::default();
                        t.events = iwbtg_sim::events::EventSet::default();
                        let id = self.push_kind(t);
                        twins.insert(inst.kind, id);
                        id
                    }
                };
                inst.kind = twin;
                inst.params = Params::default();
            }
        }
    }

    /// CR5: warps whose creation code (`roomTo`) was not converted have no `room_to`.
    fn warp_targets(&mut self, rooms: &[RoomDef]) {
        for r in rooms {
            let missing = r
                .instances
                .iter()
                .filter(|i| {
                    self.kinds
                        .get(i.kind.index().unwrap_or(usize::MAX))
                        .is_some_and(|k| {
                            k.base == Builtin::Warp
                                && !k.params.0.contains_key("room_to")
                                && !i.params.0.contains_key("room_to")
                        })
                })
                .count();
            if missing > 0 {
                self.trace_note("CR5", format!("{} in {}", missing, r.name));
            }
        }
    }
}

/// Script size limit (docs/format/v0.md, Scripts).
const MAX_SCRIPT_BYTES: usize = 256 * 1024;

enum Placed {
    Instances(Vec<PlacedInstance>),
    Dropped,
    Failed,
}

/// Built-ins that are engine roles (CR3 maps infra objects onto these).
fn engine_builtin(b: Builtin) -> bool {
    engine_roles().iter().any(|(_, _, eb)| *eb == Some(b))
}

/// Engine role name, sim role and default built-in.
fn engine_roles() -> [(&'static str, EngineKind, Option<Builtin>); 9] {
    [
        ("world", EngineKind::World, Some(Builtin::World)),
        ("kid", EngineKind::Kid, Some(Builtin::Kid)),
        ("kid_flipped", EngineKind::KidFlipped, None),
        ("kid_bullet", EngineKind::KidBullet, Some(Builtin::KidBullet)),
        ("bow", EngineKind::Bow, Some(Builtin::Bow)),
        ("blood", EngineKind::Blood, Some(Builtin::Blood)),
        (
            "blood_emitter",
            EngineKind::BloodEmitter,
            Some(Builtin::BloodEmitter),
        ),
        ("game_over", EngineKind::GameOver, Some(Builtin::GameOver)),
        ("playerface", EngineKind::Playerface, Some(Builtin::Playerface)),
    ]
}

fn kind_param(p: &ParamValue) -> Option<KindId> {
    match p {
        ParamValue::Kind(k) => Some(*k),
        _ => None,
    }
}

/// Variant matching: reals by bits, so `2` and `2.0` match.
fn param_matches(a: &ParamValue, b: &ParamValue) -> bool {
    match (a, b) {
        (ParamValue::Real(x), ParamValue::Real(y)) => x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

fn show_param(p: &ParamValue) -> String {
    match p {
        ParamValue::Real(r) => format!("{}", r.get()),
        ParamValue::Bool(b) => b.to_string(),
        ParamValue::Str(s) => s.clone(),
        ParamValue::Room(r) => format!("room#{}", r.0),
        ParamValue::Kind(k) => format!("kind#{}", k.0),
        ParamValue::Sound(s) => format!("sound#{}", s.0),
    }
}

fn trace_param(p: &TraceParam) -> ParamValue {
    match p {
        TraceParam::Str(s) => ParamValue::Str(s.clone()),
        TraceParam::Bool(b) => ParamValue::Bool(*b),
        // Trace-map integers are small (variant numbers), so the conversion is exact.
        TraceParam::Int(i) => ParamValue::Real(Real::new(*i as f64)),
        TraceParam::Float(bits) => ParamValue::Real(Real::from_bits(*bits)),
    }
}

/// The index of `name` in `table`, appending it on first use.
fn intern_in(table: &mut Vec<String>, name: &str) -> u32 {
    let i = table.iter().position(|n| n == name).unwrap_or_else(|| {
        table.push(name.to_owned());
        table.len() - 1
    });
    u32::try_from(i).unwrap_or(u32::MAX)
}

fn offset_masks(set: &MaskSet, offset: u32) -> MaskSet {
    let shift = |m: &MaskId| MaskId(m.0.saturating_add(offset));
    match set {
        MaskSet::Shared(m) => MaskSet::Shared(shift(m)),
        MaskSet::PerFrame(v) => MaskSet::PerFrame(v.iter().map(shift).collect()),
    }
}

/// UTF-8 byte order mark.
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// The text of a package file without a leading byte order mark. A missing or non-UTF-8 file
/// gives `None`; a non-UTF-8 file also pushes an error.
fn read_text<'f>(files: &'f PackageFiles, path: &str, out: &mut Vec<Diagnostic>) -> Option<&'f str> {
    let bytes = files.get(path)?;
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(t) => Some(t),
        Err(e) => {
            let valid = std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap_or("");
            out.push(Diagnostic::error_at(
                path,
                valid,
                valid.len(),
                "file is not valid UTF-8",
            ));
            None
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Step 9: gameplay_hash.

/// `game.toml` keys that cannot change the sim (docs/format/v0.md, Versioning).
const COSMETIC_MANIFEST_KEYS: [&str; 7] = [
    "title",
    "version",
    "source_version",
    "creators",
    "source",
    "third_party",
    "music",
];

/// BLAKE3 over the canonical parsed form of everything that can change the sim
/// (docs/format/v0.md, Versioning; architecture "Versions"): the gameplay fields of `game.toml`,
/// the physics profile, objects, rooms (not tiles), timelines, paths, sound and sprite sidecars,
/// scripts, the built mask bits, and the mask rule ([`ALPHA_IN_MASK`]). Data files hash as their
/// parsed values with sorted keys, so comments and formatting do not count; scripts hash as text
/// with LF line ends.
pub fn gameplay_hash(files: &PackageFiles, manifest: &Manifest, def: &GameDef) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"iwbtg-gameplay-hash v0\0");
    h.update(&def.engine_rev.to_le_bytes());
    h.update(match ALPHA_IN_MASK {
        AlphaRule::Greater => b"alpha>",
        AlphaRule::GreaterOrEqual => b"alpha>=",
    });
    let physics = manifest.physics.get_ref().as_str();
    for (path, bytes) in files.iter() {
        let data = path == MANIFEST_PATH
            || path == physics
            || crate::engine_pack::is_gameplay_file(path)
            || [
                "objects/",
                "timelines/",
                "paths/",
                "sprites/",
                "sounds/",
                "music/",
            ]
            .iter()
            .any(|d| path.starts_with(d))
            || (path.starts_with("rooms/") && !path.ends_with(".tiles.toml"));
        let script = path.starts_with("scripts/") && path.ends_with(".rhai");
        if script {
            let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
            hash_bytes(&mut h, path.as_bytes());
            hash_bytes(&mut h, text.as_bytes());
        } else if data && path.ends_with(".toml") {
            let text = String::from_utf8_lossy(bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes)).into_owned();
            let Ok(mut table) = toml::from_str::<toml::Table>(&text) else {
                continue;
            };
            if path == MANIFEST_PATH {
                for k in COSMETIC_MANIFEST_KEYS {
                    table.remove(k);
                }
            }
            hash_bytes(&mut h, path.as_bytes());
            canonical(&mut h, &toml::Value::Table(table));
        }
    }
    h.update(&(def.masks.len() as u64).to_le_bytes());
    for m in &def.masks {
        for e in [m.bbox.left, m.bbox.top, m.bbox.right, m.bbox.bottom] {
            h.update(&e.to_le_bytes());
        }
        match &m.shape {
            MaskShape::Rect => {
                h.update(b"r");
            }
            MaskShape::Empty => {
                h.update(b"e");
            }
            MaskShape::Bits(b) => {
                h.update(b"b");
                h.update(&b.width().to_le_bytes());
                h.update(&b.height().to_le_bytes());
                let mut byte = 0u8;
                let mut n = 0u32;
                for y in 0..i64::from(b.height()) {
                    for x in 0..i64::from(b.width()) {
                        byte = byte << 1 | u8::from(b.get(x, y));
                        n += 1;
                        if n == 8 {
                            h.update(&[byte]);
                            (byte, n) = (0, 0);
                        }
                    }
                }
                h.update(&[byte, u8::try_from(n).unwrap_or(0)]);
            }
        }
    }
    *h.finalize().as_bytes()
}

fn hash_bytes(h: &mut blake3::Hasher, b: &[u8]) {
    h.update(&(b.len() as u64).to_le_bytes());
    h.update(b);
}

/// A TOML value in a fixed binary form: type tag, then content; tables with sorted keys; floats
/// as bits.
fn canonical(h: &mut blake3::Hasher, v: &toml::Value) {
    match v {
        toml::Value::String(s) => {
            h.update(b"s");
            hash_bytes(h, s.as_bytes());
        }
        toml::Value::Integer(i) => {
            h.update(b"i");
            h.update(&i.to_le_bytes());
        }
        toml::Value::Float(f) => {
            h.update(b"f");
            h.update(&f.to_bits().to_le_bytes());
        }
        toml::Value::Boolean(b) => {
            h.update(if *b { b"T" } else { b"F" });
        }
        toml::Value::Datetime(d) => {
            h.update(b"d");
            hash_bytes(h, d.to_string().as_bytes());
        }
        toml::Value::Array(a) => {
            h.update(b"a");
            h.update(&(a.len() as u64).to_le_bytes());
            for item in a {
                canonical(h, item);
            }
        }
        toml::Value::Table(t) => {
            h.update(b"t");
            h.update(&(t.len() as u64).to_le_bytes());
            let mut keys: Vec<&String> = t.keys().collect();
            keys.sort();
            for k in keys {
                hash_bytes(h, k.as_bytes());
                if let Some(v) = t.get(k) {
                    canonical(h, v);
                }
            }
        }
    }
}

#[cfg(all(test, panic = "unwind"))]
mod tests {
    use super::*;

    #[test]
    fn stub_names() {
        assert_eq!(
            stub_name("not implemented: WP7b: physics::load"),
            Some("WP7b: physics::load")
        );
        assert_eq!(stub_name("not implemented"), None);
        assert_eq!(stub_name("index out of bounds"), None);
    }

    #[cfg(panic = "unwind")]
    #[test]
    fn pending_call_catches_only_stubs() {
        let mut pending = BTreeSet::new();
        let r: Option<u8> = pending_call(&mut pending, || unimplemented!("WP9: script::x"));
        assert_eq!(r, None);
        assert!(pending.contains("WP9: script::x"));
        assert_eq!(pending_call(&mut pending, || 5), Some(5));
        let other = std::panic::catch_unwind(|| {
            let mut p = BTreeSet::new();
            pending_call(&mut p, || panic!("a real bug"))
        });
        assert!(other.is_err());
    }
}
