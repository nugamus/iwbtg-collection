//! Package to `GameDef` (docs/format/v0.md). WP7a owns this file, including the
//! calls to the WP7b steps (`physics::load`, `sounds::load`, `paths::load`, `timelines::load`,
//! `params::check`), whose signatures WP0 fixed; WP7b owns only their bodies. The manifest's
//! `[engine]` and `[timing]` sections are WP7a's too (`manifest.rs`).
//!
//! Steps (docs/format/v0.md): 1 manifest and `[engine]`; 2 sprites and masks; 3 kinds; 4 rooms; 5 ranks;
//! 6 variable ids; 7 (WP7b) physics, params, timelines, paths, sounds; 8 cross-references;
//! 9 `gameplay_hash`.
//!
//! This file holds `Loader`, `run` (the step order) and the shared helpers; the steps live in one
//! child module per file type (#792), each an `impl Loader` block: `sprites`, `kinds` (objects,
//! params, derived kinds), `kid` (physics profiles), `ranks` (`object_order`, engine kinds),
//! `rooms`, `game` (`[timing]`, maps, `[stripped]`) and `hash` (`gameplay_hash`).
//!
//! `LoadMode::Trace` accepts the research converter's current output: what a converter request
//! (CR1-CR15, docs/format/v0.md "Converter requests") will fix becomes a warning that names the
//! request, and the loader falls back to the trace map (`tests/trace-map.toml`) where it holds
//! the missing data. `LoadMode::Strict` turns the same problems into errors.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use iwbtg_sim::builtins::{self, ParamSpec, ParamType};
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, GlobalIndex, GroupId, KindDef, KindId, KindVariant, Mask,
    MaskId, MaskSet, MaskShape, NameId, ParamValue, Params, PhysicsProfile, PlacedInstance, RankEntry,
    RankTable, ResetModel, RestartModel, RoomAction, RoomDef, RoomId, RoomRole, RoomWhen, ScriptDef,
    ScriptId, SoundDef, SoundId, SpriteDef, SpriteId, VariantSel, ViewDef, variant_matches,
};
use iwbtg_sim::input::Action;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::rules::num::FloatModel;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::stripped::{
    DialogueRegion, RankedStart as SimRankedStart, Region, RegionCondition, StrippedDef,
};
use iwbtg_sim::timing::{TimingDef, TimingTrigger};
use toml::Spanned;

use crate::check::check;
use crate::diag::{Diagnostic, Severity, has_errors};
use crate::files::PackageFiles;
use crate::ids::check_id;
use crate::manifest::{
    DEFAULT_MAP_ID, ENGINE_ROLES, EngineSection, MANIFEST_PATH, Manifest, RankedStart, TIMING_TRIGGERS,
    global_value,
};
use crate::masks::{ALPHA_IN_MASK, AlphaRule};
use crate::objects::{self, ObjectFile, kind_classes, legacy_role_classes, param_value};
use crate::order::{OrderEntry, rank_of, resolve_ranks};
use crate::rooms::{ActionFile, IndexFile, InstanceFile, MAX_GM_ID, RoomFile, WhenFile, parse_room};
use crate::sprites::{MAX_IMAGE_SIDE, load_sprite_at, parse_sidecar};
use crate::tracemap::{TRACE_MAP_PATH, TraceMap, TraceParam, TraceStatus, load_trace_map};
use crate::vars::intern_vars;
use crate::{params, particles, paths, physics, sounds, timelines};

mod game;
mod hash;
mod kid;
mod kinds;
mod ranks;
mod rooms;
mod sprites;

pub use hash::gameplay_hash;

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
    /// Every map's timing chain, in `[[maps]]` order (one `main` map without `[[maps]]`); maps
    /// whose triggers do not resolve are left out (their errors are in `diagnostics`).
    pub maps: Vec<MapTiming>,
    /// How the game's physics stands against its engine pack's family preset (#802). `None` when
    /// no definition was built. A catalog or game page reads this for the custom-physics mark.
    pub physics: Option<physics::PhysicsStanding>,
}

/// One map's resolved timing chain (docs/format/game.md, Maps).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapTiming {
    /// Map id.
    pub id: String,
    /// Start, stages and end.
    pub timing: TimingDef,
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
                maps: Vec::new(),
                physics: None,
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
    let physics_mark = def.as_ref().map(|def| {
        // Room profiles in id order, with the game's own file left out (`None`): rooms naming it
        // share the game's profile (#970).
        let rooms: Vec<(&str, &PhysicsProfile)> = l
            .room_physics_ids
            .iter()
            .filter_map(|(id, index)| Some((id.as_str(), &def.room_physics[usize::from((*index)?)])))
            .collect();
        physics::standing(files, &manifest, &def.physics, &rooms)
    });
    LoadReport {
        def,
        diagnostics: diags,
        gameplay_hash,
        maps: std::mem::take(&mut l.maps),
        physics: physics_mark,
    }
}

// ---------------------------------------------------------------------------------------------
// Converter requests named by Trace-mode warnings (docs/format/v0.md, "Converter requests").

/// What each converter request asks for, as printed in its warning.
/// Each map with its chain and its ranked start, in `[[maps]]` order (#505). A map whose chain
/// did not resolve is left out (its errors are diagnostics, so no `GameDef` is returned anyway).
fn map_defs(
    maps: &[MapTiming],
    mut starts: BTreeMap<String, iwbtg_sim::stripped::RankedStart>,
) -> Vec<iwbtg_sim::def::MapDef> {
    maps.iter()
        .map(|m| iwbtg_sim::def::MapDef {
            id: m.id.clone(),
            timing: m.timing.clone(),
            start: starts.remove(&m.id).unwrap_or_default(),
        })
        .collect()
}

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
    /// `particles/<id>.toml`, loaded after sprites so kind params can name them.
    particles: Vec<iwbtg_sim::def::PartSystemDef>,
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
    /// Kinds derived for a placed instance whose params pick other classes or events (#659,
    /// #853): (kind, the picking params as `k=v,...`) to kind.
    class_derived: BTreeMap<(KindId, String), KindId>,
    /// A package object's `counts_as` (and legacy role) classes, which its derived kinds keep.
    kind_extra: BTreeMap<KindId, ClassMask>,
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
    /// `collides_with` per kind name: the classes its script's `collide(other)` listens to.
    collides: BTreeMap<String, Vec<ClassMask>>,
    groups: Vec<String>,
    /// Resolved map timing chains.
    maps: Vec<MapTiming>,
    /// Room profiles, `physics/<id>.toml` (#33); `GameDef::room_physics`.
    room_physics: Vec<PhysicsProfile>,
    /// Room profile id to its index in `room_physics`; `None` for the file that is the game's
    /// own profile (`game.toml` `physics`), which rooms naming it share.
    room_physics_ids: BTreeMap<String, Option<u16>>,
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
            particles: Vec::new(),
            masks: Vec::new(),
            kinds: Vec::new(),
            kind_ids: BTreeMap::new(),
            dropped: BTreeSet::new(),
            failed: BTreeSet::new(),
            defaults: BTreeMap::new(),
            derived: BTreeMap::new(),
            rank_kind: BTreeMap::new(),
            class_derived: BTreeMap::new(),
            kind_extra: BTreeMap::new(),
            scripts: Vec::new(),
            room_ids: BTreeMap::new(),
            sounds: Vec::new(),
            spec_cache: BTreeMap::new(),
            ranks: RankTable::default(),
            appended: BTreeSet::new(),
            names: Vec::new(),
            package_classes: Vec::new(),
            collides: BTreeMap::new(),
            groups: Vec::new(),
            maps: Vec::new(),
            room_physics: Vec::new(),
            room_physics_ids: BTreeMap::new(),
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

    /// `[engine].first_runtime_id` (#680), checked: at most [`MAX_GM_ID`] and above every
    /// placed `gm_id`, since runtime ids follow the file's last-instance-id, which is never below
    /// a placed id (CLONE_SPEC, Instances, "Placed instance ids").
    fn first_runtime_id(&mut self, engine: Option<&EngineSection>, rooms: &[RoomDef]) -> Option<u32> {
        let first = engine?.first_runtime_id.as_ref()?;
        let (span, first) = (first.span(), *first.get_ref());
        if first > MAX_GM_ID {
            self.manifest_error(
                span.clone(),
                format!("`first_runtime_id` must be at most {MAX_GM_ID}"),
            );
        }
        let highest = rooms
            .iter()
            .flat_map(|r| r.instances.iter().filter_map(|i| i.gm_id.map(|id| (id, &r.name))))
            .max_by_key(|(id, _)| *id);
        if let Some((id, room)) = highest
            && id >= first
        {
            self.manifest_error(
                span,
                format!(
                    "`first_runtime_id` {first} must be above every placed `gm_id` (room `{room}` places {id})"
                ),
            );
        }
        Some(first)
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
        // Particle systems (#765), before kinds so `particle_system` params can name them.
        self.load_particles();
        // Room art the sim never sees: tile files and tileset sidecars.
        crate::art::check(self.files, &mut self.diags);
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
        self.load_room_physics(&physics);
        // Step 6: variable ids, from every name the loader sees.
        let vars = self.intern(&rooms);
        // Step 4, second half, and step 5 for placed instances.
        let mut room_defs = self.build_rooms(&rooms, &vars);
        // `[defaults]` variants no room places get their kinds now, for script spawns (#701).
        let variants = self.all_variants();
        // Engine kinds, boot and the spawnable-kind rank check (step 5, CR4).
        let engine_kinds = self.engine_kinds();
        let boot = self.boot(&engine_kinds);
        self.spawnable_ranks(&room_defs, &engine_kinds);
        // Events per kind, now that the physics profile is known.
        self.event_sets(&physics);
        let timing = self.timing();
        self.maps = self.map_timings(timing.as_ref());
        let mut d = Vec::new();
        let timelines =
            pending_call(&mut self.pending, || timelines::load(self.files, &mut d)).unwrap_or_default();
        let paths = pending_call(&mut self.pending, || paths::load(self.files, &mut d)).unwrap_or_default();
        self.diags.append(&mut d);
        // Step 8: cross-references not already resolved above.
        self.warp_targets(&room_defs);
        self.stripped_refs(&rooms);
        let (stripped, map_starts) = self.stripped_def(&vars);
        let maps = map_defs(&self.maps, map_starts);
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
        let mut def = GameDef {
            id: self.m.id.get_ref().clone(),
            tick_rate: self.m.tick_rate(),
            max_ticks: *self.m.max_ticks.get_ref(),
            runner: *self.m.runner.get_ref(),
            engine_rev: *self.m.engine_rev.get_ref(),
            float_model: match engine.and_then(|e| e.float_model) {
                None | Some(crate::manifest::FloatModelName::F64) => FloatModel::F64,
            },
            uninit_reads_zero: engine.and_then(|e| e.uninit_reads_zero).unwrap_or(true),
            first_runtime_id: self.first_runtime_id(engine, &room_defs),
            sprites: self.sprites_in_order(),
            masks: std::mem::take(&mut self.masks),
            kinds: std::mem::take(&mut self.kinds),
            variants,
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
            room_physics: std::mem::take(&mut self.room_physics),
            restart: engine
                .and_then(|e| e.restart.as_ref())
                .map_or_else(RestartModel::default, |r| r.model()),
            reset: engine
                .and_then(|e| e.reset.as_ref())
                .map_or_else(ResetModel::default, |r| r.model()),
            sounds: std::mem::take(&mut self.sounds),
            timelines,
            paths,
            particle_systems: std::mem::take(&mut self.particles),
            scripts: std::mem::take(&mut self.scripts),
            timing: timing?,
            maps,
            stripped,
        };
        self.merge_script_events(&mut def);
        // Step 7: timing rooms a run can reach (#504).
        self.reachable_timing_rooms(&def);
        // Step 7: kinds that tie for a collision handler (#737).
        self.handler_ties(&def);
        // Step 7 (WP7b): params against the built-ins' specs, on the built definition.
        let mut d = Vec::new();
        let trace = self.trace_mode();
        let _ = pending_call(&mut self.pending, || params::check(&def, trace, &mut d));
        self.diags.append(&mut d);
        // Scripts: structure, calls, name literals and lints (WP9.3).
        for issue in iwbtg_sim::script::validate::validate(&def) {
            let (line, column, severity) = (issue.line, issue.column, issue.severity);
            let file = def
                .scripts
                .get(issue.script)
                .map_or("scripts", |s| s.name.as_str());
            self.diags.push(Diagnostic {
                file: file.to_owned(),
                line,
                column,
                severity: match severity {
                    iwbtg_sim::script::validate::Severity::Error => Severity::Error,
                    iwbtg_sim::script::validate::Severity::Warning => Severity::Warning,
                },
                message: script_issue_message(self.files, issue),
            });
        }
        Some(def)
    }

    // -- Step 2: sprites ----------------------------------------------------------------------

    /// Reads `particles/<id>.toml` (#765); sprite ids must be known.
    fn load_particles(&mut self) {
        let files = self.files;
        let sprite_ids = &self.sprite_ids;
        let mut sprite_diags = Vec::new();
        let mut d = Vec::new();
        let mut sprite = |name: &str, file: &str, text: &str, span: Range<usize>| match sprite_ids.get(name) {
            Some(e) => Some(e.id),
            None => {
                let msg = if name.starts_with(crate::ids::ENGINE_PREFIX) {
                    crate::engine_pack::unresolved(files, name, "sprite")
                } else {
                    format!("sprite `{name}` does not exist")
                };
                sprite_diags.push(Diagnostic::error_at(file, text, span.start, msg));
                None
            }
        };
        self.particles = particles::load(files, &mut sprite, &mut d);
        self.diags.append(&mut sprite_diags);
        self.diags.append(&mut d);
    }
}

/// Script size limit (docs/format/scripts.md).
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
fn engine_roles() -> [(&'static str, EngineKind, Option<Builtin>); 10] {
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
        ("death_sound", EngineKind::DeathSound, None),
    ]
}

fn kind_param(p: &ParamValue) -> Option<KindId> {
    match p {
        ParamValue::Kind(k) => Some(*k),
        _ => None,
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
        ParamValue::Particles(p) => format!("particles#{}", p.0),
        ParamValue::List(l) => format!("[{}]", l.iter().map(show_param).collect::<Vec<_>>().join(", ")),
    }
}

fn trace_param(p: &TraceParam) -> ParamValue {
    match p {
        TraceParam::Str(s) => ParamValue::Str(s.clone()),
        TraceParam::Bool(b) => ParamValue::Bool(*b),
        // Trace-map integers are small (variant numbers), so the conversion is exact.
        TraceParam::Int(i) => ParamValue::Real(Real::new(*i as f64)),
        TraceParam::Float(bits) => ParamValue::Real(Real::from_bits(*bits)),
        TraceParam::List(items) => ParamValue::List(items.iter().map(trace_param).collect()),
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

/// The message of a script validator issue. A sound or music name that starts with `engine:`
/// gets the engine pack error, the same text as the physics `[sounds]` (#373).
fn script_issue_message(files: &PackageFiles, issue: iwbtg_sim::script::validate::Issue) -> String {
    match &issue.unknown {
        Some((what, name)) if name.starts_with(crate::ids::ENGINE_PREFIX) => {
            crate::engine_pack::unresolved(files, name, what)
        }
        _ => issue.message,
    }
}

#[cfg(all(test, panic = "unwind"))]
mod tests {
    use super::*;

    fn unknown_issue(name: &str) -> iwbtg_sim::script::validate::Issue {
        iwbtg_sim::script::validate::Issue {
            script: 0,
            line: 1,
            column: 1,
            severity: iwbtg_sim::script::validate::Severity::Error,
            message: format!("unknown sound \"{name}\""),
            unknown: Some(("sound".to_owned(), name.to_owned())),
        }
    }

    #[test]
    fn a_script_engine_name_gets_the_engine_pack_error() {
        let mut files = PackageFiles::new();
        files
            .insert(
                "engine/a1/engine.toml",
                b"id = \"a1\"
"
                .to_vec(),
            )
            .unwrap();
        // A pack that is bundled but lacks the sound, and a pack that is not bundled.
        assert_eq!(
            script_issue_message(&files, unknown_issue("engine:a1/jump")),
            crate::engine_pack::unresolved(&files, "engine:a1/jump", "sound")
        );
        assert!(
            script_issue_message(&files, unknown_issue("engine:a1/jump")).contains("has no sound `jump`")
        );
        assert!(script_issue_message(&files, unknown_issue("engine:zz/jump")).contains("not in the package"));
        // A plain name keeps the validator's message.
        assert_eq!(
            script_issue_message(&files, unknown_issue("boom")),
            "unknown sound \"boom\""
        );
    }

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

/// A `difficulty` global as the engine difficulty: a whole number from 0 to 255.
/// An indexed `set_global` whose index global is the engine's save slot (`global.savenum`),
/// not a package global.
fn is_engine_index(name: &str) -> bool {
    name == "savenum"
}

fn whole_difficulty(v: &iwbtg_sim::world::Value) -> Option<u8> {
    match v {
        iwbtg_sim::world::Value::Real(r) => {
            let n = r.get();
            (0u8..=255).find(|d| f64::from(*d).total_cmp(&n).is_eq())
        }
        _ => None,
    }
}
