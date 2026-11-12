//! The immutable game definition (docs/plans/sim-core.md 1.4, 4.3, 8.1).
//!
//! `crates/format` builds a [`GameDef`] from a package; tests build one with
//! [`GameDefBuilder`](super::testing::GameDefBuilder). The sim never changes it. WP0 declares;
//! WP7a owns afterwards (WP4b owns the behavior of [`RestartModel`] and [`ResetModel`]).

use std::collections::BTreeMap;

use super::ids::{KindId, MaskId, RoomId, ScriptId, SpriteId, VarId};
use super::kind::{KindDef, KindVariant, Params, RankTable, VariantSel, variant_matches};
use super::path::PathDef;
use super::physics::PhysicsProfile;
use super::room::{RoleSet, RoomDef};
use super::script::ScriptDef;
use super::sound::SoundDef;
use super::sprite::{Mask, SpriteDef};
use super::timeline::TimelineDef;
use crate::num::Real;
use crate::rules::Runner;
use crate::rules::num::FloatModel;
use crate::stripped::{RankedStart, StrippedDef};
use crate::timing::TimingDef;

/// Everything the sim needs to run one game.
#[derive(Clone, PartialEq, Debug)]
pub struct GameDef {
    /// Package id.
    pub id: String,
    /// Ticks per second of the original (`room_speed` of play rooms, usually 50).
    pub tick_rate: u32,
    /// Hard limit on a run's ticks, from `game.toml`.
    pub max_ticks: u64,
    /// The original runner.
    pub runner: Runner,
    /// Built-in behavior revision the package was verified against.
    pub engine_rev: u16,
    /// Runner FPU model, from `game.toml [engine]` (N3); copied into `NumRules::float_model`.
    pub float_model: FloatModel,
    /// GM "treat uninitialized variables as 0" (19 games on, keep-celebrating-for-xia off).
    pub uninit_reads_zero: bool,
    /// The GM8 instance id of the first instance created at run time (`[engine]
    /// first_runtime_id`, #680: the game file's last-instance-id + 1). `None` when the package
    /// does not give one; then the runtime counter starts above the highest placed `gm_id`
    /// (`World::first_runtime_id`, #669).
    pub first_runtime_id: Option<u32>,
    /// Sprites.
    pub sprites: Vec<SpriteDef>,
    /// Masks, referenced by sprites and kinds.
    pub masks: Vec<Mask>,
    /// Kinds.
    pub kinds: Vec<KindDef>,
    /// `[defaults.<kind>]` variants, each kind's in `game.toml` order, with the kinds derived
    /// for them: all of them, placed or not, so a script spawn can take one (#701).
    pub variants: Vec<KindVariant>,
    /// Rooms.
    pub rooms: Vec<RoomDef>,
    /// The game's first room as the package names it (title art for game select).
    pub start_room: RoomId,
    /// Where a new game starts (`StartPoint::NewGame`, CR7).
    pub new_game_room: RoomId,
    /// Persistent instances `room::start_game` creates before the first room (the `world`
    /// built-in stands in for rInit).
    pub boot: Vec<KindId>,
    /// Dispatch ranks from `object_order`.
    pub ranks: RankTable,
    /// Kinds the sim spawns by role.
    pub engine_kinds: BTreeMap<EngineKind, KindId>,
    /// Variable names known at load, ids assigned from the sorted list (plan 1.1).
    pub var_names: BTreeMap<String, VarId>,
    /// Instance names (`NameId` index).
    pub names: Vec<String>,
    /// Instance groups (`GroupId` index).
    pub groups: Vec<String>,
    /// The kid's physics profile.
    pub physics: PhysicsProfile,
    /// The retry path (R key).
    pub restart: RestartModel,
    /// The reset path (F2).
    pub reset: ResetModel,
    /// Sounds.
    pub sounds: Vec<SoundDef>,
    /// Timelines.
    pub timelines: Vec<TimelineDef>,
    /// Paths.
    pub paths: Vec<PathDef>,
    /// Script sources, indexed by `ScriptId` (`KindDef::script`). Compiled by the script host
    /// (WP9) when the `Sim` is built.
    pub scripts: Vec<ScriptDef>,
    /// Timing triggers (`[timing]`). A run on a map uses that map's chain ([`GameDef::maps`]);
    /// this one only when `maps` is empty (a definition built by hand).
    pub timing: TimingDef,
    /// The package's maps in `[[maps]]` order, one [`DEFAULT_MAP_ID`] map without `[[maps]]`
    /// (docs/format/v0.md, Maps). A run picks one (`SimOptions::map`, #505).
    pub maps: Vec<MapDef>,
    /// Ranked (stripped) mode: `[stripped]` resolved (`crate::stripped`). Used only by
    /// `PlayMode::Stripped` runs.
    pub stripped: StrippedDef,
}

/// The id of a package's only map when it has no `[[maps]]`.
pub const DEFAULT_MAP_ID: &str = "main";

/// One map of a package, resolved (docs/format/v0.md, Maps): what a run on it times and where
/// a ranked run on it starts.
#[derive(Clone, PartialEq, Debug)]
pub struct MapDef {
    /// The map id, unique in the package.
    pub id: String,
    /// The map's chain: its `start`, `end` and `stages` over `[timing]`'s.
    pub timing: TimingDef,
    /// The map's ranked start: its `stripped_start` merged over `[stripped].start`.
    pub start: RankedStart,
}

impl GameDef {
    /// The index of the map a run on `id` plays; `None` is the first map. `None` when the
    /// package has no such map. Without [`GameDef::maps`] the only map is [`DEFAULT_MAP_ID`].
    pub fn map_index(&self, id: Option<&str>) -> Option<u16> {
        match id {
            None => Some(0),
            Some(id) if self.maps.is_empty() => (id == DEFAULT_MAP_ID).then_some(0),
            Some(id) => self
                .maps
                .iter()
                .position(|m| m.id == id)
                .and_then(|i| u16::try_from(i).ok()),
        }
    }

    /// The id of map `map`.
    pub fn map_id(&self, map: u16) -> &str {
        self.maps
            .get(usize::from(map))
            .map_or(DEFAULT_MAP_ID, |m| m.id.as_str())
    }

    /// The timing chain of map `map` ([`GameDef::timing`] without maps).
    pub fn map_timing(&self, map: u16) -> &TimingDef {
        self.maps
            .get(usize::from(map))
            .map_or(&self.timing, |m| &m.timing)
    }

    /// The ranked start of map `map` (`[stripped].start` without maps).
    pub fn map_start(&self, map: u16) -> &RankedStart {
        self.maps
            .get(usize::from(map))
            .map_or(&self.stripped.start, |m| &m.start)
    }

    /// The kind with this id.
    pub fn kind(&self, id: KindId) -> Option<&KindDef> {
        self.kinds.get(id.index()?)
    }

    /// Whether `kind` is `base` or a kind the loader derived from it, `<base>[<params>]` (a
    /// `[defaults.<base>]` variant, or a placed instance whose params pick other classes, #659),
    /// as a GM object index covers its children. Probe kind targets and the script `instances`
    /// use it.
    pub fn kind_covers(&self, base: KindId, kind: KindId) -> bool {
        if kind == base {
            return true;
        }
        match (self.kind(kind), self.kind(base)) {
            (Some(k), Some(b)) => k
                .name
                .strip_prefix(b.name.as_str())
                .is_some_and(|rest| rest.starts_with('[')),
            _ => false,
        }
    }

    /// The kind an instance of `kind` with `params` uses: the first `[defaults.<kind>]` variant
    /// whose params it matches (the instance's params, else the kind's), else `kind` (#701).
    pub fn variant_kind(&self, kind: KindId, params: &Params) -> KindId {
        let Some(kd) = self.kind(kind) else {
            return kind;
        };
        self.variants
            .iter()
            .find(|v| v.kind == kind && variant_matches(&v.when, params, &kd.params))
            .map_or(kind, |v| v.variant)
    }

    /// The room with this id.
    pub fn room(&self, id: RoomId) -> Option<&RoomDef> {
        self.rooms.get(id.index()?)
    }

    /// The sprite with this id.
    pub fn sprite(&self, id: SpriteId) -> Option<&SpriteDef> {
        self.sprites.get(id.index()?)
    }

    /// The mask with this id.
    pub fn mask(&self, id: MaskId) -> Option<&Mask> {
        self.masks.get(id.index()?)
    }

    /// The script with this id.
    pub fn script(&self, id: ScriptId) -> Option<&ScriptDef> {
        self.scripts.get(id.index()?)
    }

    /// Dispatch rank for an instance of `kind` spawned at run time (plan 1.3).
    pub fn rank_for(&self, kind: KindId, variant: Option<&VariantSel>) -> Option<u16> {
        self.ranks.rank_for(kind, variant)
    }

    /// The kind the sim spawns for an engine role, so built-ins never hard-code kind ids.
    pub fn engine_kind(&self, k: EngineKind) -> Option<KindId> {
        self.engine_kinds.get(&k).copied()
    }
}

/// Engine kinds the sim spawns by role.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum EngineKind {
    /// The engine controller.
    World,
    /// The kid.
    Kid,
    /// The flipped kid object (games that flip by object swap).
    KidFlipped,
    /// The kid's bullet.
    KidBullet,
    /// The bow.
    Bow,
    /// Blood drop.
    Blood,
    /// Blood emitter.
    BloodEmitter,
    /// GAMEOVER.
    GameOver,
    /// Playerface.
    Playerface,
    /// Family B `objDeathSound` (the `death_sound` death step creates it).
    DeathSound,
}

/// How the game retries after R (plan 4.3). Behavior in `restart.rs` (WP4b); the `world`
/// built-in runs it at the world's rank, in the phase `trigger` names.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RestartModel {
    /// Where the retry runs (R27).
    pub trigger: RestartTrigger,
    /// What the retry does.
    pub kind: RestartKind,
    /// Room roles where R does nothing (menus).
    pub excluded_roles: RoleSet,
    /// Ticks spent in a boot room on `GameRestartViaBoot` (R17, TR-ROOM-01).
    pub boot_ticks: u8,
    /// Added to the loaded y when the saved kid was flipped (+4 in 10 Group A games).
    pub flipped_load_dy: Real,
    /// What a retry counts.
    pub death_on_restart: DeathOnRestart,
    /// The kid-side steps of the retry (family B, showdown).
    pub kid_steps: RetryKidSteps,
}

/// The kid-side steps of a retry that only some games have (kid/B.md "Restart", kid/A3.md
/// showdown "Restart"). All off by default.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RetryKidSteps {
    /// Ticks added to the play timer before the retry (B: 2, then `save_death_time`).
    pub add_time: u32,
    /// Write the death-only save (saveDeathTime) after `add_time`, before the kid is destroyed.
    pub save_death_time: bool,
    /// Clear `frozen` and `frozen2` (the globals and every kid's own flags) before the kid is
    /// destroyed (B, showdown).
    pub clear_frozen: bool,
    /// After the new kid exists: reset its jump values, speeds and jump flags
    /// (`kid::api::reset_jump_values`, showdown's loadGame).
    pub reset_jump_values: bool,
    /// Before the retry destroys anything: every `death_sound` stops its sound, restores the music
    /// volume and is destroyed (B, kid/B.md Restart 2).
    pub release_death_sound: bool,
    /// After the new kid exists: clear every trigger array and `global.triggered`
    /// (`MechGlobals::clear_triggers`, showdown's loadGame).
    pub clear_triggers: bool,
}

impl Default for RestartModel {
    /// Group A: R key press event, game_restart through one boot tick, no flipped offset.
    fn default() -> RestartModel {
        RestartModel {
            trigger: RestartTrigger::KeyPressEvent,
            kind: RestartKind::GameRestartViaBoot,
            excluded_roles: RoleSet::default(),
            boot_ticks: 1,
            flipped_load_dy: Real::ZERO,
            death_on_restart: DeathOnRestart::Nothing,
            kid_steps: RetryKidSteps::default(),
        }
    }
}

/// Where the retry (R27) or the reset runs. The key is `Action::Restart` for [`RestartModel`]
/// and `Action::Reset` for [`ResetModel`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RestartTrigger {
    /// The world's Key press event for the key (keyboard phase).
    KeyPressEvent,
    /// The world's Step, every tick while held (wanna, A3 I7).
    StepHeld,
    /// The world's Step, on the press.
    StepPressed {
        /// Skip while the game is paused (keep-celebrating-for-xia).
        skip_while_paused: bool,
    },
}

/// What the retry does (plan 4.3 table).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RestartKind {
    /// Groups A (minus showdown), B, C: game_restart through the boot room.
    GameRestartViaBoot,
    /// kaenbyou, eat-the-lemon, go-the-spook-house: recreate the kid, go to the saved room.
    RoomGotoSaved,
    /// showdown: loadGame without game_restart (kid/A3.md; R26, settled).
    ShowdownLoad,
    /// keep-celebrating-for-xia: recreate the kid in place, then go to the saved room.
    RecreateInPlace,
}

/// What a retry counts besides restarting.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DeathOnRestart {
    /// Nothing.
    #[default]
    Nothing,
    /// A death-only save of the counters (little-runner, wanna saveDeathTime).
    SaveCounters,
    /// One death for each live kid (both kid kinds) and then the death-only save, and nothing
    /// when no kid lives (showdown, kid/A3.md Restart step 2: "if a player exists, add a death
    /// ... do the same for playerGradhiusu"; trace a3-t18-a, one kid, +1).
    SaveCountersPerKid,
}

/// How the game resets after F2 (plan 4.3). Mirrors [`RestartModel`]: the `world` built-in runs
/// it at the world's rank, in the phase `trigger` names. Behavior in `restart.rs` (WP4b).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ResetModel {
    /// Where the reset runs.
    pub trigger: RestartTrigger,
    /// What the reset does.
    pub kind: ResetKind,
    /// Room roles where F2 does nothing.
    pub excluded_roles: RoleSet,
}

impl Default for ResetModel {
    /// The world's "Key F2 pressed" event (global keys are Key press events on world, CLONE_SPEC
    /// input), delete the temp marker and game_restart, in every room.
    fn default() -> ResetModel {
        ResetModel {
            trigger: RestartTrigger::KeyPressEvent,
            kind: ResetKind::default(),
            excluded_roles: RoleSet::default(),
        }
    }
}

/// What F2 does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ResetKind {
    /// Groups A, B, C: delete the save's temp marker and game_restart.
    #[default]
    DeleteTempAndRestart,
    /// kaenbyou, eat-the-lemon gameReset: reset globals and go to the boot room.
    GameReset,
}
