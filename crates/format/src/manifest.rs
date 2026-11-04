//! The package manifest `game.toml` (docs/format/v0.md, Manifest).
//!
//! Field names, required fields and allowed values follow v0.md exactly. Every struct denies
//! unknown keys, so a typo or a field from a newer format fails with file, line and column.
//! Fields that later checks point at keep their span ([`Spanned`]).

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use iwbtg_sim::def::{
    DeathOnRestart, ResetKind, ResetModel, RestartKind, RestartModel, RestartTrigger, RoleSet, RoomRole,
};
use iwbtg_sim::num::Real;
use serde::Deserialize;
use toml::Spanned;
use toml::value::Datetime;

use crate::diag::Diagnostic;
use crate::files::{CONTENT_DIRS, PackageFiles, TESTS_DIR, check_path};
use crate::ids::{EngineRef, KindRef, check_id, check_semver};
use crate::number::Number;
use crate::sprites::MAX_IMAGE_SIDE;

/// Path of the manifest inside a package.
pub const MANIFEST_PATH: &str = "game.toml";

/// Newest format major version this build reads. Format 0 is the v0 draft.
pub const FORMAT_VERSION: u32 = 0;

/// Default room speed when `tick_rate` is absent.
pub const DEFAULT_TICK_RATE: u32 = 50;

/// Default script budget per tick when `[budget].ops_per_tick` is absent
/// (docs/decisions/0001-script-language.md, Step budget).
pub const DEFAULT_OPS_PER_TICK: u64 = 1_000_000;

/// Hard cap on `[budget].ops_per_tick` (docs/decisions/0001-script-language.md, Step budget).
pub const MAX_OPS_PER_TICK: u64 = 2_000_000;

/// The parsed `game.toml`.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Format major version. The client reads every version up to [`FORMAT_VERSION`].
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub format: Spanned<u32>,
    /// Package id, snake_case. Stable forever; leaderboards key on it.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub id: Spanned<String>,
    /// Display title.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub title: Spanned<String>,
    /// Semver of our port. For humans.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub version: Spanned<String>,
    /// Version of the original we ported.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub source_version: Option<Spanned<String>>,
    /// Engine pack id in `games/_engines/` (`engine_pack`; `[engine]` is the engine settings
    /// table).
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub engine_pack: Option<Spanned<String>>,
    /// Built-in behavior revision this package was verified against.
    #[cfg_attr(feature = "schema", schemars(with = "u16"))]
    pub engine_rev: Spanned<u16>,
    /// Original runner; selects event-order differences.
    #[cfg_attr(feature = "schema", schemars(with = "crate::schema::RunnerSchema"))]
    pub runner: Spanned<Runner>,
    /// Path to the physics profile.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub physics: Spanned<String>,
    /// First room.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub start_room: Spanned<String>,
    /// Default room speed; [`DEFAULT_TICK_RATE`] if absent. Rooms may override.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<u32>"))]
    pub tick_rate: Option<Spanned<u32>>,
    /// Upper bound for a replay, in ticks. The server rejects longer replays.
    #[cfg_attr(feature = "schema", schemars(with = "u64"))]
    pub max_ticks: Spanned<u64>,
    /// Event and collision rank order of every kind used: kind names and selectors.
    #[cfg_attr(feature = "schema", schemars(with = "Vec<OrderItem>"))]
    pub object_order: Spanned<Vec<Spanned<OrderItem>>>,
    /// GM room order (`room_goto_next`, `room_goto_previous`); rooms not listed follow in path
    /// order.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<Vec<String>>"))]
    pub room_order: Option<Spanned<Vec<Spanned<String>>>>,
    /// Engine settings: restart and reset models, boot kinds, number model, new-game room, engine
    /// roles (docs/plans/sim-core.md 8.3).
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<EngineSection>"))]
    pub engine: Option<Spanned<EngineSection>>,
    /// Default sprite and mask of bare built-in kinds, per variant (`[defaults.spike]`).
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "BTreeMap<String, KindDefaults>"))]
    pub defaults: BTreeMap<Spanned<String>, Spanned<KindDefaults>>,
    /// At least one creator.
    #[cfg_attr(feature = "schema", schemars(with = "Vec<Creator>"))]
    pub creators: Spanned<Vec<Creator>>,
    /// Where the original comes from and its permission status.
    #[cfg_attr(feature = "schema", schemars(with = "Source"))]
    pub source: Spanned<Source>,
    /// Script budget.
    #[serde(default)]
    pub budget: Option<Budget>,
    /// Run timing: start and end triggers and stages.
    #[cfg_attr(feature = "schema", schemars(with = "Timing"))]
    pub timing: Spanned<Timing>,
    /// Switches for tricks that are engine behaviors.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "BTreeMap<String, TrickSetting>"))]
    pub tricks: BTreeMap<Spanned<String>, Spanned<TrickSetting>>,
    /// One entry per file that is not the creator's own work or ours.
    #[serde(default)]
    pub third_party: Vec<ThirdParty>,
    /// Optional per-track settings, keyed by music id.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "BTreeMap<String, MusicSettings>"))]
    pub music: BTreeMap<Spanned<String>, MusicSettings>,
    /// Maps: each built-in difficulty, category and version, with its own timing and board.
    /// Absent: one map, [`DEFAULT_MAP_ID`], timed by `[timing]`.
    #[serde(default)]
    pub maps: Vec<MapEntry>,
    /// Art for the background system and the game select list; absent: the first ranked
    /// room's background ([`crate::key_art`]).
    #[serde(default)]
    pub key_art: Option<KeyArtSection>,
}

/// Id of the one map of a package without `[[maps]]`.
pub const DEFAULT_MAP_ID: &str = "main";

/// One `[[maps]]` entry (docs/format/v0.md, Maps). Timing keys it leaves out come from
/// `[timing]`.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MapEntry {
    /// Map id, unique within the package. Stable forever; leaderboards key on it.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub id: Spanned<String>,
    /// Display name in game select.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub name: Spanned<String>,
    /// The original's built-in difficulty this map plays, as the game names it ("Medium").
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub difficulty: Option<Spanned<String>>,
    /// The run category ("Any%", "100%", "True Ending").
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub category: Option<Spanned<String>>,
    /// The original's version this map plays; default `source_version`.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub version: Option<Spanned<String>>,
    /// The run starts at this trigger; default `[timing].start`.
    #[serde(default)]
    pub start: Option<TriggerRef>,
    /// The run ends at this trigger; default `[timing].end`.
    #[serde(default)]
    pub end: Option<TriggerRef>,
    /// Stages in order; default `[timing].stages`. `[]` is a single-stage map.
    #[serde(default)]
    pub stages: Option<Vec<Stage>>,
}

fn opt_str(s: &Option<Spanned<String>>) -> Option<&str> {
    s.as_ref().map(|v| v.get_ref().as_str())
}

/// A map with its timing filled in from `[timing]` ([`Manifest::map_list`]).
#[derive(Debug, Clone, Copy)]
pub struct MapView<'a> {
    /// Map id.
    pub id: &'a str,
    /// Display name.
    pub name: &'a str,
    /// Built-in difficulty, if any.
    pub difficulty: Option<&'a str>,
    /// Category, if any.
    pub category: Option<&'a str>,
    /// The original's version, if known.
    pub version: Option<&'a str>,
    /// Start trigger.
    pub start: &'a TriggerRef,
    /// End trigger.
    pub end: &'a TriggerRef,
    /// Stages in order.
    pub stages: &'a [Stage],
}

/// The original GameMaker runner. One type with the sim, which selects rules by it
/// (`RunnerRules::for_runner`).
pub use iwbtg_sim::rules::Runner;

/// One `[[creators]]` entry.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Creator {
    /// Name as the creator wants it shown.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub name: Spanned<String>,
    /// What they did.
    pub role: CreatorRole,
}

/// A creator's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum CreatorRole {
    /// Made the game.
    Author,
    /// Made the game with others.
    CoAuthor,
    /// Made the music.
    Music,
    /// Made the art.
    Art,
    /// Tested the game.
    Tester,
    /// Anything else.
    Other,
}

/// The `[source]` table.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Original game page.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub url: Spanned<String>,
    /// Permission status.
    #[cfg_attr(feature = "schema", schemars(with = "Permission"))]
    pub permission: Spanned<Permission>,
    /// When the status was last set (a local date).
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub permission_date: Option<Spanned<Datetime>>,
    /// Where the permission came from, or why it is pending.
    #[serde(default)]
    pub permission_note: Option<String>,
}

/// Permission to port the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    /// The creator said yes.
    Granted,
    /// Covered by the community's standing assumption.
    CommunityAssumed,
    /// Asked, no answer yet.
    Pending,
    /// The creator said no. A revoked package is never loaded, built or served.
    Revoked,
}

/// The `[budget]` table.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Budget {
    /// Script budget units per tick; [`DEFAULT_OPS_PER_TICK`] if absent, at most
    /// [`MAX_OPS_PER_TICK`].
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<u64>"))]
    pub ops_per_tick: Option<Spanned<u64>>,
}

/// The `[timing]` table.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Timing {
    /// The run starts at this trigger.
    pub start: TriggerRef,
    /// The run ends at this trigger.
    pub end: TriggerRef,
    /// Stages in order; each ends at its trigger. May be absent for a single-stage game.
    #[serde(default)]
    pub stages: Vec<Stage>,
}

/// `{ trigger = "name" }`, or `{ trigger = "enter_room", room = "stage1_01" }`.
///
/// The loader accepts the `[timing]` vocabulary of `iwbtg_sim::timing::TimingTrigger`:
/// `run_start`, `run_end` and `enter_room` (with `room`) ([`TIMING_TRIGGERS`]).
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TriggerRef {
    /// A trigger name from the vocabulary.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub trigger: Spanned<String>,
    /// The room, for `enter_room`.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub room: Option<Spanned<String>>,
}

/// One split stage.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Stage {
    /// Stage id, unique within the package.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub id: Spanned<String>,
    /// Display name.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub name: Spanned<String>,
    /// The stage ends at this trigger.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub trigger: Spanned<String>,
    /// The room, for `enter_room`.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub room: Option<Spanned<String>>,
}

/// The `[timing]` trigger vocabulary (WP8b, `iwbtg_sim::timing::TimingTrigger`).
pub const TIMING_TRIGGERS: [&str; 3] = ["run_start", "run_end", "enter_room"];

/// One `object_order` entry as written: a kind name, or a selector table
/// `{ kind = "spike", dir = "up" }` with exactly one param key besides `kind`.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum OrderItem {
    /// `"spike"`, `"@world"`.
    Kind(String),
    /// `{ kind = "spike", dir = "up" }`.
    Selector(
        #[cfg_attr(feature = "schema", schemars(with = "BTreeMap<String, serde_json::Value>"))]
        BTreeMap<String, toml::Value>,
    ),
}

impl OrderItem {
    /// The kind name, if the entry has a valid shape.
    pub fn kind(&self) -> Option<&str> {
        match self {
            OrderItem::Kind(k) => Some(k),
            OrderItem::Selector(t) => t.get("kind").and_then(toml::Value::as_str),
        }
    }

    /// The selector's `(key, value)`, or why the table is malformed. `Ok(None)` for a plain name.
    pub fn selector(&self) -> Result<Option<(&str, &toml::Value)>, String> {
        let OrderItem::Selector(t) = self else {
            return Ok(None);
        };
        if self.kind().is_none() {
            return Err("an object_order selector needs `kind = \"<kind>\"`".to_owned());
        }
        let mut rest = t.iter().filter(|(k, _)| k.as_str() != "kind");
        match (rest.next(), rest.next()) {
            (Some((k, v)), None) => match v {
                toml::Value::String(_)
                | toml::Value::Integer(_)
                | toml::Value::Float(_)
                | toml::Value::Boolean(_) => Ok(Some((k.as_str(), v))),
                _ => Err(format!(
                    "selector param `{k}` must be a string, number or boolean"
                )),
            },
            _ => Err(
                "an object_order selector has `kind` and exactly one param, for example \
                      { kind = \"spike\", dir = \"up\" }"
                    .to_owned(),
            ),
        }
    }
}

/// The `[engine]` table (docs/plans/sim-core.md 4.2, 4.3, 8.3; CR7).
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct EngineSection {
    /// Where `StartPoint::NewGame` starts (CR7). Defaults to `start_room` with a warning.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub new_game_room: Option<Spanned<String>>,
    /// Persistent kinds created before the first room; default: the `world` engine kind.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<Vec<String>>"))]
    pub boot: Option<Vec<Spanned<String>>>,
    /// GM "treat uninitialized variables as 0"; default true.
    #[serde(default)]
    pub uninit_reads_zero: Option<bool>,
    /// Runner FPU model (N3); default `f64`.
    #[serde(default)]
    pub float_model: Option<FloatModelName>,
    /// The retry path (R).
    #[serde(default)]
    pub restart: Option<RestartSection>,
    /// The reset path (F2).
    #[serde(default)]
    pub reset: Option<ResetSection>,
    /// Engine roles: which kind the sim spawns as `kid_bullet`, `bow`, ... (`"none"`: the game
    /// never spawns one). Keys: [`ENGINE_ROLES`].
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "BTreeMap<String, String>"))]
    pub roles: BTreeMap<Spanned<String>, Spanned<String>>,
}

/// Engine role names, in `EngineKind` order.
pub const ENGINE_ROLES: [&str; 10] = [
    "world",
    "kid",
    "kid_flipped",
    "kid_bullet",
    "bow",
    "blood",
    "blood_emitter",
    "game_over",
    "playerface",
    "death_sound",
];

/// `float_model` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum FloatModelName {
    /// IEEE binary64.
    #[serde(rename = "f64")]
    F64,
}

/// A room role (`role` in room files, `excluded_roles` here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RoleName {
    /// Boot room.
    Boot,
    /// Title.
    Title,
    /// Menu.
    Menu,
    /// Difficulty or stage select.
    Select,
    /// Gameplay.
    Play,
}

impl RoleName {
    /// The sim's role.
    pub const fn role(self) -> RoomRole {
        match self {
            RoleName::Boot => RoomRole::Boot,
            RoleName::Title => RoomRole::Title,
            RoleName::Menu => RoomRole::Menu,
            RoleName::Select => RoomRole::Select,
            RoleName::Play => RoomRole::Play,
        }
    }
}

/// Restart and reset triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TriggerName {
    /// The world's Key press event.
    KeyPressEvent,
    /// The world's Step, every tick while held.
    StepHeld,
    /// The world's Step, on the press.
    StepPressed,
    /// The world's Step, on the press, skipped while paused.
    StepPressedUnlessPaused,
}

impl TriggerName {
    /// The sim's trigger.
    pub const fn trigger(self) -> RestartTrigger {
        match self {
            TriggerName::KeyPressEvent => RestartTrigger::KeyPressEvent,
            TriggerName::StepHeld => RestartTrigger::StepHeld,
            TriggerName::StepPressed => RestartTrigger::StepPressed {
                skip_while_paused: false,
            },
            TriggerName::StepPressedUnlessPaused => RestartTrigger::StepPressed {
                skip_while_paused: true,
            },
        }
    }
}

/// `[engine] restart = { ... }`; absent keys keep `RestartModel::default()`.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RestartSection {
    /// What the retry does.
    #[serde(default)]
    pub kind: Option<RestartKindName>,
    /// Where it runs.
    #[serde(default)]
    pub trigger: Option<TriggerName>,
    /// Roles where R does nothing.
    #[serde(default)]
    pub excluded_roles: Option<Vec<RoleName>>,
    /// Boot ticks of `game_restart_via_boot`.
    #[serde(default)]
    pub boot_ticks: Option<u8>,
    /// y offset when the saved kid was flipped.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<Number>"))]
    pub flipped_load_dy: Option<Spanned<Number>>,
    /// What a retry counts.
    #[serde(default)]
    pub death_on_restart: Option<DeathName>,
    /// The kid-side steps of the retry (family B, showdown).
    #[serde(default)]
    pub kid_steps: Option<KidStepsSection>,
}

/// `restart = { kid_steps = { ... } }`; absent keys are off.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct KidStepsSection {
    /// Ticks added to the play timer first (B: 2).
    #[serde(default)]
    pub add_time: Option<u32>,
    /// Death-only save after `add_time`.
    #[serde(default)]
    pub save_death_time: Option<bool>,
    /// Clear `frozen` and `frozen2`.
    #[serde(default)]
    pub clear_frozen: Option<bool>,
    /// Reset the new kid's jump values (showdown).
    #[serde(default)]
    pub reset_jump_values: Option<bool>,
    /// Stop the death sound and restore the music volume before the retry (B).
    #[serde(default)]
    pub release_death_sound: Option<bool>,
    /// Clear the trigger arrays and `global.triggered` after the load (showdown).
    #[serde(default)]
    pub clear_triggers: Option<bool>,
}

/// Restart kinds (plan 4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RestartKindName {
    /// `GameRestartViaBoot`.
    GameRestartViaBoot,
    /// `RoomGotoSaved`.
    RoomGotoSaved,
    /// `ShowdownLoad`.
    ShowdownLoad,
    /// `RecreateInPlace`.
    RecreateInPlace,
}

/// `death_on_restart` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DeathName {
    /// Nothing.
    Nothing,
    /// A death-only save of the counters.
    SaveCounters,
    /// One death per live kid, then the death-only save (showdown).
    SaveCountersPerKid,
}

/// `[engine] reset = { ... }`; absent keys keep `ResetModel::default()`.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ResetSection {
    /// What F2 does.
    #[serde(default)]
    pub kind: Option<ResetKindName>,
    /// Where it runs.
    #[serde(default)]
    pub trigger: Option<TriggerName>,
    /// Roles where F2 does nothing.
    #[serde(default)]
    pub excluded_roles: Option<Vec<RoleName>>,
}

/// Reset kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ResetKindName {
    /// `DeleteTempAndRestart`.
    DeleteTempAndRestart,
    /// `GameReset`.
    GameReset,
}

/// A role set from names.
pub fn role_set(roles: &[RoleName]) -> RoleSet {
    RoleSet(roles.iter().fold(0, |acc, r| acc | r.role().bit()))
}

impl RestartSection {
    /// The model, over `RestartModel::default()`.
    pub fn model(&self) -> RestartModel {
        let mut m = RestartModel::default();
        if let Some(k) = self.kind {
            m.kind = match k {
                RestartKindName::GameRestartViaBoot => RestartKind::GameRestartViaBoot,
                RestartKindName::RoomGotoSaved => RestartKind::RoomGotoSaved,
                RestartKindName::ShowdownLoad => RestartKind::ShowdownLoad,
                RestartKindName::RecreateInPlace => RestartKind::RecreateInPlace,
            };
        }
        if let Some(t) = self.trigger {
            m.trigger = t.trigger();
        }
        if let Some(r) = &self.excluded_roles {
            m.excluded_roles = role_set(r);
        }
        if let Some(b) = self.boot_ticks {
            m.boot_ticks = b;
        }
        if let Some(dy) = &self.flipped_load_dy {
            m.flipped_load_dy = Real::new(dy.get_ref().0);
        }
        if let Some(d) = self.death_on_restart {
            m.death_on_restart = match d {
                DeathName::Nothing => DeathOnRestart::Nothing,
                DeathName::SaveCounters => DeathOnRestart::SaveCounters,
                DeathName::SaveCountersPerKid => DeathOnRestart::SaveCountersPerKid,
            };
        }
        if let Some(k) = &self.kid_steps {
            m.kid_steps.add_time = k.add_time.unwrap_or(0);
            m.kid_steps.save_death_time = k.save_death_time.unwrap_or(false);
            m.kid_steps.clear_frozen = k.clear_frozen.unwrap_or(false);
            m.kid_steps.reset_jump_values = k.reset_jump_values.unwrap_or(false);
            m.kid_steps.release_death_sound = k.release_death_sound.unwrap_or(false);
            m.kid_steps.clear_triggers = k.clear_triggers.unwrap_or(false);
        }
        m
    }
}

impl ResetSection {
    /// The model, over `ResetModel::default()`.
    pub fn model(&self) -> ResetModel {
        let mut m = ResetModel::default();
        if let Some(k) = self.kind {
            m.kind = match k {
                ResetKindName::DeleteTempAndRestart => ResetKind::DeleteTempAndRestart,
                ResetKindName::GameReset => ResetKind::GameReset,
            };
        }
        if let Some(t) = self.trigger {
            m.trigger = t.trigger();
        }
        if let Some(r) = &self.excluded_roles {
            m.excluded_roles = role_set(r);
        }
        m
    }
}

/// `[defaults.<builtin>]`: the sprite and mask a bare built-in kind uses, per variant
/// (docs/plans/sim-core.md 8.3, "Kind default sprites per variant").
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct KindDefaults {
    /// Default sprite.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub sprite: Option<Spanned<String>>,
    /// Default object-level mask sprite.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub mask: Option<Spanned<String>>,
    /// Per-variant sprites: each table has param keys to match (`dir = "up"`) plus `sprite`
    /// and/or `mask`.
    #[serde(default)]
    #[cfg_attr(
        feature = "schema",
        schemars(with = "Vec<BTreeMap<String, serde_json::Value>>")
    )]
    pub variants: Vec<Spanned<BTreeMap<String, toml::Value>>>,
}

/// A trick switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum TrickSetting {
    /// The trick works.
    On,
    /// The trick is disabled.
    Off,
    /// The trick works and the game needs it.
    Required,
}

/// One `[[third_party]]` entry.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ThirdParty {
    /// Package path of the file.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub path: Spanned<String>,
    /// What it is.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub what: Spanned<String>,
    /// Rights holder.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub owner: Spanned<String>,
    /// SPDX id, or `unknown`.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub license: Spanned<String>,
    /// Clearance status.
    pub status: ThirdPartyStatus,
}

/// Clearance status of a third-party file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum ThirdPartyStatus {
    /// Cleared for public release.
    Cleared,
    /// Not cleared yet.
    ToClear,
    /// Never shipped in public builds.
    ExcludedFromPublic,
}

/// One `[music.<id>]` table.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MusicSettings {
    /// Playback volume, 1.0 is unchanged.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<Number>"))]
    pub volume: Option<Spanned<Number>>,
}

/// The `[key_art]` table (docs/format/v0.md, Key art).
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct KeyArtSection {
    /// The game's key art: a PNG inside the package, 1920 x 1080 recommended.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub image: Option<Spanned<String>>,
    /// Per-map overrides, keyed by map id: a PNG for that map instead of `image`.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "BTreeMap<String, String>"))]
    pub maps: BTreeMap<Spanned<String>, Spanned<String>>,
}

/// Only the `format` key, read before the full parse so a newer package gets a clear
/// "update the client" error instead of an unknown-key error.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
struct FormatProbe {
    #[cfg_attr(feature = "schema", schemars(with = "Option<i64>"))]
    format: Option<Spanned<i64>>,
}

impl Manifest {
    /// Parses `game.toml` text. Syntax errors, missing fields, unknown keys and wrong value
    /// types come back as one diagnostic with line and column.
    ///
    /// A missing field is reported at the start of the table that lacks it, because that is
    /// the span `toml` gives. For a missing top-level field (`id`, `format`, `[timing]`, ...)
    /// the table is the whole document, so the position is always 1:1, on the `#:schema`
    /// line, and the message names the field. This is on purpose: a root key can go anywhere
    /// before the first `[table]` header, so no single line is more correct than another.
    pub fn parse(text: &str) -> Result<Self, Diagnostic> {
        if let Ok(FormatProbe { format: Some(format) }) = toml::from_str::<FormatProbe>(text)
            && *format.get_ref() > i64::from(FORMAT_VERSION)
        {
            return Err(Diagnostic::error_at(
                MANIFEST_PATH,
                text,
                format.span().start,
                format!(
                    "this package uses format {}, but this client reads formats up to {FORMAT_VERSION}; \
                     update the client",
                    format.get_ref()
                ),
            ));
        }
        toml::from_str::<Manifest>(text).map_err(|e| toml_error(MANIFEST_PATH, text, &e))
    }

    /// Default room speed.
    pub fn tick_rate(&self) -> u32 {
        self.tick_rate
            .as_ref()
            .map_or(DEFAULT_TICK_RATE, |t| *t.get_ref())
    }

    /// Every map in order, with its timing filled in from `[timing]`. A package without
    /// `[[maps]]` has one map, [`DEFAULT_MAP_ID`], named after the game.
    pub fn map_list(&self) -> Vec<MapView<'_>> {
        let timing = self.timing.get_ref();
        let source_version = self.source_version.as_ref().map(|v| v.get_ref().as_str());
        if self.maps.is_empty() {
            return vec![MapView {
                id: DEFAULT_MAP_ID,
                name: self.title.get_ref(),
                difficulty: None,
                category: None,
                version: source_version,
                start: &timing.start,
                end: &timing.end,
                stages: &timing.stages,
            }];
        }
        self.maps
            .iter()
            .map(|m| MapView {
                id: m.id.get_ref(),
                name: m.name.get_ref(),
                difficulty: opt_str(&m.difficulty),
                category: opt_str(&m.category),
                version: opt_str(&m.version).or(source_version),
                start: m.start.as_ref().unwrap_or(&timing.start),
                end: m.end.as_ref().unwrap_or(&timing.end),
                stages: m.stages.as_deref().unwrap_or(&timing.stages),
            })
            .collect()
    }

    /// Script budget units per tick.
    pub fn ops_per_tick(&self) -> u64 {
        self.budget
            .as_ref()
            .and_then(|b| b.ops_per_tick.as_ref())
            .map_or(DEFAULT_OPS_PER_TICK, |o| *o.get_ref())
    }

    /// Checks everything the parser cannot: id syntax, ranges, references to package files,
    /// duplicates and the permission status. `text` is the manifest source, for positions.
    pub fn validate(&self, text: &str, files: &PackageFiles, out: &mut Vec<Diagnostic>) {
        let mut v = Validator { text, files, out };

        if *self.format.get_ref() > FORMAT_VERSION {
            v.error(
                self.format.span(),
                format!("format must be at most {FORMAT_VERSION}"),
            );
        }
        v.id(&self.id);
        v.non_empty(&self.title, "title");
        if let Err(e) = check_semver(self.version.get_ref()) {
            v.error(self.version.span(), e);
        }
        if let Some(sv) = &self.source_version {
            v.non_empty(sv, "source_version");
        }
        if let Some(engine) = &self.engine_pack
            && v.id(engine)
        {
            crate::engine_pack::validate(v.files, engine.get_ref(), v.out);
        }
        // TODO(OPEN: runner differences between GM 8.0 and 8.1, from the CLONE_SPEC): nothing to
        // check yet; the sim will read `runner` once the differences are known.
        self.validate_physics(&mut v);
        if v.id(&self.start_room) {
            let room = format!("rooms/{}.toml", self.start_room.get_ref());
            if !v.files.contains(&room) {
                v.error(
                    self.start_room.span(),
                    format!("start_room `{}` has no `{room}`", self.start_room.get_ref()),
                );
            }
        }
        if let Some(rate) = &self.tick_rate
            && *rate.get_ref() == 0
        {
            v.error(rate.span(), "tick_rate must be at least 1");
        }
        if *self.max_ticks.get_ref() == 0 {
            v.error(self.max_ticks.span(), "max_ticks must be at least 1");
        }
        self.validate_object_order(&mut v);
        self.validate_creators(&mut v);
        self.validate_source(&mut v);
        if let Some(ops) = self.budget.as_ref().and_then(|b| b.ops_per_tick.as_ref())
            && !(1..=MAX_OPS_PER_TICK).contains(ops.get_ref())
        {
            v.error(
                ops.span(),
                format!("ops_per_tick must be between 1 and {MAX_OPS_PER_TICK}"),
            );
        }
        self.validate_timing(&mut v);
        self.validate_maps(&mut v);
        for key in self.tricks.keys() {
            // TODO(OPEN: trick ids, from the research trick inventory): check `key` against the
            // known trick ids.
            v.id(key);
        }
        self.validate_third_party(&mut v);
        self.validate_key_art(&mut v);
        for (track, settings) in &self.music {
            if v.id(track) {
                let file = format!("music/{}.ogg", track.get_ref());
                if !v.files.contains(&file) {
                    v.error(
                        track.span(),
                        format!("music `{}` has no `{file}`", track.get_ref()),
                    );
                }
            }
            if let Some(volume) = &settings.volume {
                let value = volume.get_ref().0;
                if !value.is_finite() || value < 0.0 {
                    v.error(volume.span(), "volume must be a finite number of at least 0");
                }
            }
        }
    }

    fn validate_physics(&self, v: &mut Validator<'_>) {
        if !v.path(&self.physics, "physics") {
            return;
        }
        let path = self.physics.get_ref();
        if !path.ends_with(".toml")
            || path == MANIFEST_PATH
            || CONTENT_DIRS.iter().any(|dir| path.starts_with(dir))
            || path.starts_with(crate::engine_pack::ENGINE_DIR)
        {
            v.error(
                self.physics.span(),
                format!(
                    "physics profile `{path}` must be a `.toml` file other than `{MANIFEST_PATH}`, \
                     outside {} (usually `physics.toml`)",
                    CONTENT_DIRS.join(", ")
                ),
            );
        } else if !v.files.contains(path) {
            v.error(
                self.physics.span(),
                format!("physics profile `{path}` is not in the package"),
            );
        }
    }

    fn validate_object_order(&self, v: &mut Validator<'_>) {
        let mut seen = BTreeSet::new();
        for entry in self.object_order.get_ref() {
            let item = entry.get_ref();
            let selector = match item.selector() {
                Ok(s) => s.map(|(k, value)| format!("{k}={value}")),
                Err(e) => {
                    v.error(entry.span(), e);
                    continue;
                }
            };
            let Some(name) = item.kind() else {
                v.error(entry.span(), "an object_order selector needs `kind = \"<kind>\"`");
                continue;
            };
            match KindRef::parse(name) {
                Err(e) => v.error(entry.span(), e),
                Ok(kind) => {
                    let shown = match &selector {
                        None => name.to_owned(),
                        Some(s) => format!("{name} {{{s}}}"),
                    };
                    if !seen.insert((name.to_owned(), selector)) {
                        v.error(entry.span(), format!("`{shown}` is listed twice in object_order"));
                    }
                    match kind {
                        // TODO(OPEN: built-in kind list, from research/specs/mechanics.md): check
                        // bare kinds against the built-in list.
                        KindRef::BuiltIn(_) => {}
                        KindRef::Package(id) => {
                            let file = format!("objects/{id}.toml");
                            if !v.files.contains(&file) {
                                v.error(entry.span(), format!("`@{id}` has no `{file}`"));
                            }
                        }
                    }
                }
            }
        }
    }

    fn validate_creators(&self, v: &mut Validator<'_>) {
        if self.creators.get_ref().is_empty() {
            v.error(self.creators.span(), "creators needs at least one entry");
        }
        for creator in self.creators.get_ref() {
            v.non_empty(&creator.name, "creator name");
        }
    }

    fn validate_source(&self, v: &mut Validator<'_>) {
        let source = self.source.get_ref();
        let url = source.url.get_ref();
        let host = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or("");
        if host.is_empty() || host.starts_with('/') || url.chars().any(char::is_whitespace) {
            v.error(
                source.url.span(),
                format!("source url `{url}` must be an http:// or https:// address"),
            );
        }
        if *source.permission.get_ref() == Permission::Revoked {
            v.error(
                source.permission.span(),
                "permission is `revoked`: a revoked package is never loaded, built or served",
            );
        }
        if let Some(date) = &source.permission_date {
            let d = date.get_ref();
            if d.date.is_none() || d.time.is_some() || d.offset.is_some() {
                v.error(
                    date.span(),
                    "permission_date must be a date without a time, for example 2026-10-24",
                );
            }
        }
    }

    fn validate_timing(&self, v: &mut Validator<'_>) {
        let timing = self.timing.get_ref();
        v.non_empty(&timing.start.trigger, "trigger name");
        v.non_empty(&timing.end.trigger, "trigger name");
        validate_stages(v, &timing.start, &timing.stages);
    }

    fn validate_maps(&self, v: &mut Validator<'_>) {
        let timing = self.timing.get_ref();
        let mut ids = BTreeSet::new();
        for map in &self.maps {
            if v.id(&map.id) && !ids.insert(map.id.get_ref().as_str()) {
                v.error(
                    map.id.span(),
                    format!("map id `{}` is used twice", map.id.get_ref()),
                );
            }
            v.non_empty(&map.name, "map name");
            for (field, what) in [
                (&map.difficulty, "difficulty"),
                (&map.category, "category"),
                (&map.version, "map version"),
            ] {
                if let Some(value) = field {
                    v.non_empty(value, what);
                }
            }
            for trigger in [&map.start, &map.end].into_iter().flatten() {
                v.non_empty(&trigger.trigger, "trigger name");
            }
            // Stages inherited with an inherited start were checked under `[timing]`.
            if map.start.is_some() || map.stages.is_some() {
                let start = map.start.as_ref().unwrap_or(&timing.start);
                let stages = map.stages.as_deref().unwrap_or(&timing.stages);
                validate_stages(v, start, stages);
            }
        }
    }

    fn validate_key_art(&self, v: &mut Validator<'_>) {
        let Some(section) = &self.key_art else {
            return;
        };
        if let Some(image) = &section.image {
            v.key_art_image(image, "key_art image");
        }
        for (map, image) in &section.maps {
            v.id(map);
            if !self.map_list().iter().any(|m| m.id == map.get_ref().as_str()) {
                v.error(
                    map.span(),
                    format!("key_art map `{}` is not a map of this package", map.get_ref()),
                );
            }
            v.key_art_image(image, &format!("key_art map `{}`", map.get_ref()));
        }
    }

    fn validate_third_party(&self, v: &mut Validator<'_>) {
        let mut paths = BTreeSet::new();
        for entry in &self.third_party {
            let path = entry.path.get_ref();
            if let Some(engine_ref) = EngineRef::parse(path) {
                match engine_ref {
                    Err(e) => v.error(entry.path.span(), format!("third_party path: {e}")),
                    Ok(r) if !crate::engine_pack::resolves(v.files, r) => v.error(
                        entry.path.span(),
                        format!("third_party path `{path}`: the engine pack has no such asset (is the pack bundled?)"),
                    ),
                    Ok(_) => {}
                }
            } else if v.path(&entry.path, "third_party path") && !v.files.contains(path) {
                v.error(
                    entry.path.span(),
                    format!("third_party path `{path}` is not in the package"),
                );
            }
            if !paths.insert(path.as_str()) {
                v.error(
                    entry.path.span(),
                    format!("third_party path `{path}` is listed twice"),
                );
            }
            v.non_empty(&entry.what, "what");
            v.non_empty(&entry.owner, "owner");
            v.non_empty(&entry.license, "license");
        }
    }
}

/// Stage ids are unique, stage names and triggers are not blank, and no two stages (nor a stage
/// and the start) share a trigger.
fn validate_stages(v: &mut Validator<'_>, start_ref: &TriggerRef, stages: &[Stage]) {
    let start = start_ref.trigger.get_ref();
    let mut ids = BTreeSet::new();
    let mut triggers = BTreeSet::new();
    for stage in stages {
        if v.id(&stage.id) && !ids.insert(stage.id.get_ref().as_str()) {
            v.error(
                stage.id.span(),
                format!("stage id `{}` is used twice", stage.id.get_ref()),
            );
        }
        v.non_empty(&stage.name, "stage name");
        if !v.non_empty(&stage.trigger, "trigger name") {
            continue;
        }
        // Each stage ends at its trigger, so a shared trigger would close two stages on one
        // tick, and the start trigger would close a stage as the run begins. The last stage
        // may end at the run's `end` trigger.
        let trigger = stage.trigger.get_ref();
        let room = stage.room.as_ref().map(|r| r.get_ref().as_str());
        let start_room = start_ref.room.as_ref().map(|r| r.get_ref().as_str());
        if trigger == start && room == start_room {
            v.error(
                stage.trigger.span(),
                format!(
                    "stage `{}` ends at trigger `{trigger}`, which is the run's start trigger",
                    stage.id.get_ref()
                ),
            );
        } else if !triggers.insert((trigger.as_str(), room)) {
            v.error(
                stage.trigger.span(),
                format!("stage trigger `{trigger}` is used by two stages"),
            );
        }
    }
}

/// Collects diagnostics for one manifest.
struct Validator<'a> {
    text: &'a str,
    files: &'a PackageFiles,
    out: &'a mut Vec<Diagnostic>,
}

impl Validator<'_> {
    fn error(&mut self, span: Range<usize>, message: impl Into<String>) {
        self.out.push(Diagnostic::error_at(
            MANIFEST_PATH,
            self.text,
            span.start,
            message,
        ));
    }

    /// Checks id syntax; true when valid.
    fn id(&mut self, id: &Spanned<String>) -> bool {
        match check_id(id.get_ref()) {
            Ok(()) => true,
            Err(e) => {
                self.error(id.span(), e);
                false
            }
        }
    }

    /// Checks package path syntax and that the file is shipped (not under `tests/`); true when
    /// valid.
    fn path(&mut self, path: &Spanned<String>, what: &str) -> bool {
        let text = path.get_ref();
        let result = check_path(text).and_then(|()| {
            if text.starts_with(TESTS_DIR) {
                Err("files under `tests/` are not shipped with the package")
            } else {
                Ok(())
            }
        });
        match result {
            Ok(()) => true,
            Err(e) => {
                self.error(path.span(), format!("{what} `{text}`: {e}"));
                false
            }
        }
    }

    /// Checks a key art path and its image (docs/format/v0.md, Key art): a shipped PNG that
    /// decodes and fits the image limit; a warning when it is not 16:9 or under 1280 x 720.
    fn key_art_image(&mut self, path: &Spanned<String>, what: &str) {
        if !self.path(path, what) {
            return;
        }
        let text = path.get_ref();
        if !text.ends_with(".png") {
            self.error(path.span(), format!("{what} `{text}` must be a `.png` file"));
            return;
        }
        let Some(bytes) = self.files.get(text) else {
            self.error(path.span(), format!("{what} `{text}` is not in the package"));
            return;
        };
        match crate::key_art::image_size(bytes) {
            Err(e) => self.error(path.span(), format!("{what} `{text}`: {e}")),
            Ok((w, h)) if w > MAX_IMAGE_SIDE || h > MAX_IMAGE_SIDE => self.error(
                path.span(),
                format!("{what} `{text}` is {w} x {h}; images are at most {MAX_IMAGE_SIDE} px on a side"),
            ),
            Ok((w, h)) if !crate::key_art::is_recommended_size(w, h) => {
                self.out.push(Diagnostic::warning_at(
                    MANIFEST_PATH,
                    self.text,
                    path.span().start,
                    format!(
                        "{what} `{text}` is {w} x {h}; key art should be 16:9 and at least 1280 x 720 \
                         (1920 x 1080 recommended)"
                    ),
                ));
            }
            Ok(_) => {}
        }
    }

    /// Checks that a text field is not blank; true when it is not.
    fn non_empty(&mut self, value: &Spanned<String>, what: &str) -> bool {
        let ok = !value.get_ref().trim().is_empty();
        if !ok {
            self.error(value.span(), format!("{what} must not be empty"));
        }
        ok
    }
}

/// Turns a `toml` parse or deserialize error into a diagnostic at its span.
pub(crate) fn toml_error(file: &str, text: &str, error: &toml::de::Error) -> Diagnostic {
    let offset = error.span().map_or(0, |s| s.start);
    Diagnostic::error_at(file, text, offset, error.message().trim_end())
}

#[cfg(test)]
mod tests {
    use crate::{Diagnostic, PackageFiles, check};

    /// The `tests/valid/minimal` manifest without `[timing]`, with `extra` appended at line 20.
    fn manifest(extra: &str) -> String {
        format!(
            r#"#:schema ../game.schema.json
format = 0
id = "minimal"
title = "Minimal"
version = "0.1.0"
engine_rev = 1
runner = "gm80"
physics = "physics.toml"
start_room = "title"
max_ticks = 180_000
object_order = ["kid", "block"]

[[creators]]
name = "IWBTG Collection"
role = "author"

[source]
url = "https://example.com/minimal"
permission = "granted"
{extra}"#
        )
    }

    /// Lines 20 to 23.
    const TIMING: &str = r#"
[timing]
start = { trigger = "run_start" }
end = { trigger = "run_end" }
"#;

    /// Checks a package of `game_toml`, `physics.toml`, `rooms/title.toml` and the `extra` files
    /// (empty; the manifest checks only look at which files exist).
    fn run(game_toml: &str, extra: &[&str]) -> Vec<Diagnostic> {
        let mut files = PackageFiles::new();
        files.insert("game.toml", game_toml.as_bytes().to_vec()).unwrap();
        for path in ["physics.toml", "rooms/title.toml"].iter().chain(extra) {
            files.insert(*path, Vec::new()).unwrap();
        }
        check(&files)
    }

    /// Asserts exactly one error, at `line`, whose message contains `fragment`.
    fn assert_one(diags: &[Diagnostic], line: u32, fragment: &str) {
        match diags {
            [d] if d.is_error() && d.line == line && d.message.contains(fragment) => {}
            _ => panic!("want one error at line {line} containing {fragment:?}, got {diags:#?}"),
        }
    }

    fn with_physics(path: &str) -> String {
        manifest(TIMING).replace("\"physics.toml\"", &format!("\"{path}\""))
    }

    #[test]
    fn base_manifest_is_valid() {
        let diags = run(&manifest(TIMING), &[]);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn physics_under_tests_is_rejected() {
        assert_one(
            &run(&with_physics("tests/p.toml"), &["tests/p.toml"]),
            8,
            "physics `tests/p.toml`: files under `tests/` are not shipped",
        );
    }

    #[test]
    fn physics_must_be_a_profile_file() {
        for bad in [
            "game.toml",
            "rooms/title.toml",
            "music/stage1.toml",
            "physics.png",
        ] {
            assert_one(
                &run(&with_physics(bad), &["music/stage1.toml", "physics.png"]),
                8,
                &format!("physics profile `{bad}` must be a `.toml` file"),
            );
        }
        let diags = run(&with_physics("profiles/needle.toml"), &["profiles/needle.toml"]);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    /// A manifest with one `[[third_party]]` entry; its `path` is on line 26.
    fn third_party(path: &str) -> String {
        manifest(&format!(
            r#"{TIMING}
[[third_party]]
path = "{path}"
what = "Kid sprite"
owner = "Kayin"
license = "unknown"
status = "to-clear"
"#
        ))
    }

    #[test]
    fn third_party_under_tests_is_rejected() {
        assert_one(
            &run(&third_party("tests/x.ogg"), &["tests/x.ogg"]),
            26,
            "third_party path `tests/x.ogg`: files under `tests/` are not shipped",
        );
    }

    #[test]
    fn engine_third_party_paths_are_checked() {
        let diags = run(
            &third_party("engine:yuuutu/kid_mask"),
            &["engine/yuuutu/sprites/kid_mask.png"],
        );
        assert!(diags.is_empty(), "{diags:#?}");
        assert_one(
            &run(&third_party("engine:yuuutu/kid_mask"), &[]),
            26,
            "the engine pack has no such asset",
        );
        for bad in [
            "engine:",
            "engine:../../etc",
            "engine:yuuutu",
            "engine:yuuutu/Kid",
        ] {
            assert_one(
                &run(&third_party(bad), &[]),
                26,
                &format!("third_party path: engine reference `{bad}`"),
            );
        }
    }

    /// A manifest whose `[timing]` has `stages = [<stages>]` on line 24.
    fn stages(stages: &str) -> String {
        manifest(&format!(
            r#"
[timing]
start = {{ trigger = "run_start" }}
end = {{ trigger = "run_end" }}
stages = [{stages}]
"#
        ))
    }

    #[test]
    fn stage_triggers_are_unique() {
        let text =
            stages(r#"{ id = "a", name = "A", trigger = "t" }, { id = "b", name = "B", trigger = "t" }"#);
        let diags = run(&text, &[]);
        assert_one(&diags, 24, "stage trigger `t` is used by two stages");
        // Points at the second stage's trigger.
        assert_eq!(
            diags[0].column,
            1 + text.lines().nth(23).unwrap().rfind("\"t\"").unwrap() as u32
        );
    }

    #[test]
    fn stage_trigger_is_not_the_start_trigger() {
        let text = stages(r#"{ id = "a", name = "A", trigger = "run_start" }"#);
        assert_one(&run(&text, &[]), 24, "which is the run's start trigger");
    }

    #[test]
    fn last_stage_may_end_at_the_end_trigger() {
        let text = stages(
            r#"{ id = "a", name = "A", trigger = "a_clear" }, { id = "b", name = "B", trigger = "run_end" }"#,
        );
        let diags = run(&text, &[]);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    /// The line (1-based) of the first line of `text` that contains `needle`.
    fn line_of(text: &str, needle: &str) -> u32 {
        1 + text.lines().position(|l| l.contains(needle)).unwrap() as u32
    }

    #[test]
    fn map_ids_are_unique_and_names_not_blank() {
        let text = manifest(&format!(
            "{TIMING}
[[maps]]
id = \"any\"
name = \"Any%\"

[[maps]]
id = \"any\"
name = \"Again\"
"
        ));
        let line = 1 + line_of(&text, "name = \"Any%\"") + 2;
        assert_one(&run(&text, &[]), line, "map id `any` is used twice");
        let text = manifest(&format!(
            "{TIMING}
[[maps]]
id = \"any\"
name = \" \"
"
        ));
        assert_one(
            &run(&text, &[]),
            line_of(&text, "name = \" \""),
            "map name must not be empty",
        );
    }

    #[test]
    fn map_stages_follow_the_stage_rules() {
        let text = manifest(&format!(
            "{TIMING}
[[maps]]
id = \"a\"
name = \"A\"
stages = [{{ id = \"s\", name = \"S\", trigger = \"run_start\" }}]
"
        ));
        assert_one(
            &run(&text, &[]),
            line_of(&text, "stages = ["),
            "which is the run's start trigger",
        );
        // A map that only sets `end` inherits stages already checked under `[timing]`.
        let text = manifest(&format!(
            "{TIMING}
[[maps]]
id = \"a\"
name = \"A\"
end = {{ trigger = \"run_end\" }}
"
        ));
        assert!(run(&text, &[]).is_empty());
    }
}
