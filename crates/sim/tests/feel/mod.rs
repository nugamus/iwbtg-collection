//! Feel contracts: the loader, validator and evaluator (docs/architecture/feel-contracts.md).
//!
//! A contract is data: a scenario plus assertions over what a player can observe. The format is
//! specified in `docs/architecture/feel-contracts.md`; this module pins the parts the spec leaves
//! to the runner:
//!
//! - `tick = N` is `World::tick == N`, sampled after `Sim::step`. `.inputs` frame `f` is the step
//!   that ends at tick `f + 1`, so frame 20 is observed at tick 21.
//! - A metric is a probe-registry name (`kid.x`, `global.death`, `instance_number(kid)`,
//!   `random_get_seed()`). A bare `x`/`y` or any row-relative read is a load error; the contract
//!   always names its owner.
//! - `{ exact = v }` compares raw bits (numbers) or text. `{ min, max }` and
//!   `{ within = w, of = c }` are pixels or speeds, never ULP. Every non-exact assertion has a
//!   `why`.
//! - `control.latency_max` counts response frames: `latency = response_frame - press_frame`, with
//!   0 meaning the pressed step itself moved the metric.
//! - `reset_scope` compares the named metrics at `before` and `after`: `persists` are equal,
//!   `resets` differ.
//! - `sound.tick` is the tick the emitting step began at: the step that starts at tick N emits
//!   tick-N events and ends at N+1; `Sim::new`'s first-room output is tick 0.
//! - An F1 contract with no `evidence` loads but is `OPEN`. `--bless` (the tool) rewrites the
//!   baseline's statuses only; assertion values are never rewritten.
//!
//! The module uses only the sim's public API, `serde` and `std`, so the sim integration test
//! (`tests/feel.rs`) and `iwbtg feel` (`crates/tools/src/feel_cmd.rs`) include the same code. It
//! reads a `Sim`; it never changes the sim.

#![allow(
    dead_code,
    reason = "the sim test and the tool use different parts of the same module"
)]

pub mod fixtures;

use std::collections::BTreeMap;
use std::sync::Arc;

use iwbtg_sim::def::ids::SoundId;
use iwbtg_sim::def::{ClassMask, GameDef, RoomId};
use iwbtg_sim::input::{Action, ParsedInputs, parse_action_inputs};
use iwbtg_sim::num::Real;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::probe::{self, Column, ProbeTarget, registry};
use iwbtg_sim::save::SaveSlot;
use iwbtg_sim::sound::{SoundOutput, SoundTarget};
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::trace::Value;
use iwbtg_sim::world::SimStatus;
use iwbtg_sim::{Sim, SimOptions, StartPoint};
use serde::Deserialize;

/// The contract's impact tier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Impact {
    /// Can change what a skilled player can do; needs a contract and evidence.
    F1,
    /// Visible or audible feedback; needs a contract and screenshot/audio evidence.
    F2,
    /// No player can observe it; a hash or unit test is the right home.
    F3,
}

impl Impact {
    fn parse(s: &str) -> Option<Impact> {
        match s {
            "F1" => Some(Impact::F1),
            "F2" => Some(Impact::F2),
            "F3" => Some(Impact::F3),
            _ => None,
        }
    }

    /// The tier's name.
    pub fn name(self) -> &'static str {
        match self {
            Impact::F1 => "F1",
            Impact::F2 => "F2",
            Impact::F3 => "F3",
        }
    }
}

/// The run state a contract starts from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Start {
    /// `{ new_game = true, difficulty = ... }`: the package's new-game room, save deleted.
    NewGame {
        /// Difficulty index.
        difficulty: u8,
    },
    /// `{ room = "<name>", difficulty = ... }`: a direct room start, like a trace case.
    Room {
        /// The room.
        room: RoomId,
        /// Difficulty index.
        difficulty: u8,
    },
}

/// One loaded and validated feel contract.
#[derive(Clone, Debug)]
pub struct Contract {
    /// Must equal the path below the contracts root, without `.toml`.
    pub id: String,
    /// F1, F2 or F3.
    pub impact: Impact,
    /// A package id, or `fixture:<name>`.
    pub package: String,
    /// Provenance only in harness v1 (the run uses the package's own ruleset).
    pub ruleset: Option<String>,
    /// Provenance only in harness v1 (the run uses the package's own profile).
    pub profile: Option<String>,
    /// Where the run starts.
    pub start: Start,
    /// A starting checkpoint (`save = { ... }`); the run starts from it (`StartPoint::FromSave`).
    pub save: Option<SaveSlot>,
    /// Play original or stripped (ranked).
    pub play_mode: PlayMode,
    /// The RNG seed.
    pub seed: u32,
    /// Mods; harness v1 runs none, so a non-empty list is a load error.
    pub mods: Vec<String>,
    /// The trace, spec item or playtest that justifies the assertions.
    pub evidence: Vec<String>,
    /// The compiled `.inputs` script.
    pub inputs: ParsedInputs,
    /// Assertions at one tick.
    pub asserts: Vec<Assert>,
    /// Trajectory envelopes over a tick range.
    pub windows: Vec<Window>,
}

/// One `[[assert]]`: a tick and the checks made after its step.
#[derive(Clone, Debug)]
pub struct Assert {
    /// `World::tick == tick` after the step.
    pub tick: u64,
    /// The optional one-line justification for the assertion.
    pub why: Option<String>,
    /// The checks, in source order.
    pub checks: Vec<Check>,
}

/// One check inside an `[[assert]]`.
#[derive(Clone, Debug)]
pub enum Check {
    /// `room = "<name>"`.
    Room(String),
    /// `kid_count = n`.
    KidCount(u32),
    /// `death = n`.
    Death(u32),
    /// `time = n`.
    Time(u32),
    /// `clear = true|false`.
    Clear(bool),
    /// `status = "running"|"ended"|"halted"`.
    Status(StatusLit),
    /// `map = "<id>"`.
    Map(String),
    /// A probe-registry metric with a tolerance.
    Metric {
        /// The registry expression, for the message.
        name: String,
        /// The resolved column.
        column: Column,
        /// The declared tolerance.
        tolerance: Tolerance,
    },
    /// `control = { action, metric, latency_max }`.
    Control {
        /// The pressed action.
        action: Action,
        /// The metric that must respond.
        name: String,
        /// The resolved column.
        column: Column,
        /// The `.inputs` frame that presses the action.
        press_frame: u32,
        /// The largest accepted response delay, in frames.
        latency_max: u32,
        /// The justification.
        why: String,
    },
    /// `reset_scope = { before, after, persists, resets }`.
    ResetScope {
        /// The tick before the retry/reset.
        before: u64,
        /// The tick after it.
        after: u64,
        /// Metrics whose value must be unchanged.
        persists: Vec<NamedColumn>,
        /// Metrics whose value must differ.
        resets: Vec<NamedColumn>,
        /// The justification.
        why: String,
    },
    /// `sound = { event, sound, tick }`.
    Sound {
        /// The `SoundOutput` event.
        event: SoundEvent,
        /// The built-in sound, when the event names one.
        sound: Option<SoundId>,
        /// The sound's name, for the message.
        sound_name: Option<String>,
        /// The tick the event happens on.
        tick: u64,
    },
    /// `rng = { stream, seed, value }`.
    Rng {
        /// The contract seed, signed as `random_get_seed()` prints it.
        seed: Option<i64>,
        /// The current generator state, signed.
        value: Option<i64>,
    },
    /// `matches_save = true`: `kid.x`/`kid.y`/`room` equal the save slot's.
    MatchesSave,
    /// `trace = "<case>"`: informational evidence for this assertion.
    Trace(String),
}

/// A named probe column.
#[derive(Clone, Debug)]
pub struct NamedColumn {
    /// The registry expression.
    pub name: String,
    /// The resolved column.
    pub column: Column,
}

/// The run's status at the assertion tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusLit {
    /// Normal play.
    Running,
    /// `game_end` ran.
    Ended,
    /// The sim halted with an error.
    Halted,
}

impl StatusLit {
    fn parse(s: &str) -> Option<StatusLit> {
        match s {
            "running" => Some(StatusLit::Running),
            "ended" => Some(StatusLit::Ended),
            "halted" => Some(StatusLit::Halted),
            _ => None,
        }
    }

    /// The status's TOML word.
    pub fn name(self) -> &'static str {
        match self {
            StatusLit::Running => "running",
            StatusLit::Ended => "ended",
            StatusLit::Halted => "halted",
        }
    }
}

/// The `SoundOutput` event a `sound` assertion names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundEvent {
    /// `SoundOutput::Play`.
    Play,
    /// `SoundOutput::Stop`.
    Stop,
    /// `SoundOutput::StopAll`.
    StopAll,
    /// `SoundOutput::Volume`.
    Volume,
    /// `SoundOutput::Pitch`.
    Pitch,
}

impl SoundEvent {
    fn parse(s: &str) -> Option<SoundEvent> {
        match s {
            "play" => Some(SoundEvent::Play),
            "stop" => Some(SoundEvent::Stop),
            "stop_all" => Some(SoundEvent::StopAll),
            "volume" => Some(SoundEvent::Volume),
            "pitch" => Some(SoundEvent::Pitch),
            _ => None,
        }
    }

    /// The event's TOML name.
    pub fn name(self) -> &'static str {
        match self {
            SoundEvent::Play => "play",
            SoundEvent::Stop => "stop",
            SoundEvent::StopAll => "stop_all",
            SoundEvent::Volume => "volume",
            SoundEvent::Pitch => "pitch",
        }
    }
}

/// A declared tolerance. Exact compares raw bits; the others are pixels or speeds.
#[derive(Clone, Debug)]
pub enum Tolerance {
    /// Raw-bit equality.
    Exact(Literal),
    /// An inclusive numeric range.
    MinMax {
        /// The lower bound.
        min: f64,
        /// The upper bound.
        max: f64,
    },
    /// A symmetric window around a centre.
    Within {
        /// The half-width, in pixels or speed.
        within: f64,
        /// The centre.
        of: f64,
    },
}

impl Tolerance {
    fn is_exact(&self) -> bool {
        matches!(self, Tolerance::Exact(_))
    }

    fn text(&self) -> String {
        match self {
            Tolerance::Exact(v) => format!("exact {}", v.text()),
            Tolerance::MinMax { min, max } => format!("in [{min:?}, {max:?}]"),
            Tolerance::Within { within, of } => format!("within {within:?} of {of:?}"),
        }
    }
}

/// An exact value.
#[derive(Clone, Debug)]
pub enum Literal {
    /// A number (TOML integer or float).
    Real(f64),
    /// A flag, compared as 1/0 like the probe registry writes truth.
    Bool(bool),
    /// Text (a room or save field).
    Str(String),
}

impl Literal {
    fn value(&self) -> Value {
        match self {
            Literal::Real(v) => Value::Real(*v),
            Literal::Bool(b) => Value::Real(if *b { 1.0 } else { 0.0 }),
            Literal::Str(s) => Value::Str(s.clone()),
        }
    }

    fn text(&self) -> String {
        match self {
            Literal::Real(v) => format!("{v:?}"),
            Literal::Bool(b) => b.to_string(),
            Literal::Str(s) => format!("{s:?}"),
        }
    }
}

/// One `[[window]]`: a trajectory envelope over a closed tick range.
#[derive(Clone, Debug)]
pub struct Window {
    /// The first tick.
    pub first: u64,
    /// The last tick (inclusive).
    pub last: u64,
    /// The registry expression.
    pub name: String,
    /// The resolved column.
    pub column: Column,
    /// The largest deviation from the value at `first`.
    pub max_delta: Option<f64>,
    /// One `[min, max]` pair per tick, `last - first + 1` of them.
    pub envelope: Option<Vec<(f64, f64)>>,
    /// The justification.
    pub why: String,
}

/// The outcome of running a contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Every assertion holds and the contract has the evidence its tier needs.
    Ok,
    /// Every assertion holds, but an F1 contract has no evidence yet.
    Open,
    /// At least one assertion does not hold.
    Fail(Vec<Mismatch>),
}

impl Verdict {
    /// `OK`, `OPEN` or `FAIL`, the baseline word.
    pub fn word(&self) -> &'static str {
        match self {
            Verdict::Ok => "OK",
            Verdict::Open => "OPEN",
            Verdict::Fail(_) => "FAIL",
        }
    }

    /// True when the contract did not fail (OK or OPEN).
    pub fn is_clean(&self) -> bool {
        !matches!(self, Verdict::Fail(_))
    }
}

/// One failed assertion, in the same shape as a trace mismatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// The tick sampled.
    pub tick: u64,
    /// The field name.
    pub metric: String,
    /// What the contract declared.
    pub expected: String,
    /// What the sim gave.
    pub got: String,
    /// Up to five ticks each side of `tick`, as `(tick, value)`.
    pub context: Vec<(u64, String)>,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "mismatch: tick {}, {}: expected {}, got {}",
            self.tick, self.metric, self.expected, self.got
        )?;
        if !self.context.is_empty() {
            writeln!(f, "  context (tick: {}):", self.metric)?;
            for (tick, value) in &self.context {
                let marker = if *tick == self.tick { ">" } else { " " };
                writeln!(f, "  {marker} {tick}: {value}")?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing and validation

/// The raw TOML shape. Parsing is separate from validation so the caller names the file and the
/// sim test can parse without a `GameDef`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawContract {
    /// Must equal the path below the contracts root, without `.toml`.
    pub id: String,
    /// `F1`, `F2` or `F3`.
    pub impact: String,
    /// A package id, or `fixture:<name>`.
    pub package: String,
    /// Optional provenance.
    #[serde(default)]
    pub ruleset: Option<String>,
    /// Optional provenance.
    #[serde(default)]
    pub profile: Option<String>,
    /// The start point.
    pub start: RawStart,
    /// A starting checkpoint; the run starts from it (`StartPoint::FromSave`).
    #[serde(default)]
    pub save: Option<RawSave>,
    /// `original` (default) or `stripped`.
    #[serde(default)]
    pub play_mode: Option<String>,
    /// The RNG seed (default 0).
    #[serde(default)]
    pub seed: Option<u32>,
    /// Harness v1 runs unmodded; a non-empty list is a load error.
    #[serde(default)]
    pub mods: Vec<String>,
    /// The evidence for the assertions.
    #[serde(default)]
    pub evidence: Vec<String>,
    /// The `.inputs` script, verbatim.
    pub inputs: String,
    /// `[[assert]]` tables.
    #[serde(default, rename = "assert")]
    pub asserts: Vec<BTreeMap<String, RawValue>>,
    /// `[[window]]` tables.
    #[serde(default, rename = "window")]
    pub windows: Vec<RawWindow>,
}

/// The raw `start` table.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawStart {
    /// `{ new_game = true }`.
    #[serde(default)]
    pub new_game: bool,
    /// `{ room = "<name>" }`.
    #[serde(default)]
    pub room: Option<String>,
    /// Difficulty index (default 0).
    #[serde(default)]
    pub difficulty: Option<u8>,
}

/// The raw `save` table: a checkpoint the run starts from.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSave {
    /// The saved room; defaults to the start room.
    #[serde(default)]
    pub room: Option<String>,
    /// The saved x.
    pub x: f64,
    /// The saved y.
    pub y: f64,
    /// Saved difficulty (default: the start difficulty).
    #[serde(default)]
    pub difficulty: Option<u8>,
    /// Saved facing, -1 or 1 (default 1).
    #[serde(default)]
    pub facing: Option<i8>,
    /// Saved flip flag (default false).
    #[serde(default)]
    pub flipped: Option<bool>,
    /// Saved death counter (default 0).
    #[serde(default)]
    pub deaths: Option<u32>,
    /// Saved play time (default 0).
    #[serde(default)]
    pub time: Option<u32>,
    /// Saved boss flags (default 0).
    #[serde(default)]
    pub bosses: Option<u16>,
    /// Saved item flags (default 0).
    #[serde(default)]
    pub items: Option<u16>,
    /// Saved clear flag (default false).
    #[serde(default)]
    pub clear: Option<bool>,
}

/// One raw `[[window]]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawWindow {
    /// `[first, last]`.
    pub ticks: [u64; 2],
    /// The registry expression.
    pub metric: String,
    /// The largest deviation from the first tick's value.
    #[serde(default)]
    pub max_delta: Option<f64>,
    /// One `[min, max]` pair per tick.
    #[serde(default)]
    pub envelope: Option<Vec<[f64; 2]>>,
    /// The justification.
    pub why: String,
}

/// A TOML value in an assertion.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum RawValue {
    /// An integer.
    Integer(i64),
    /// A float.
    Float(f64),
    /// A flag.
    Bool(bool),
    /// Text.
    String(String),
    /// A list.
    Array(Vec<RawValue>),
    /// A table.
    Table(BTreeMap<String, RawValue>),
}

/// Parses a contract's TOML. The caller prefixes the file path to the message.
pub fn parse(text: &str) -> Result<RawContract, String> {
    toml::from_str(text).map_err(|e| e.to_string())
}

/// The raw package a contract names, so the caller can resolve its `GameDef`.
pub fn package(raw: &RawContract) -> &str {
    &raw.package
}

/// Validates a raw contract against its game definition. Every unresolved name is reported, so a
/// contract can be fixed in one pass.
pub fn validate(raw: RawContract, def: &GameDef, expected_id: &str) -> Result<Contract, String> {
    let mut errors: Vec<String> = Vec::new();
    if raw.id != expected_id {
        errors.push(format!(
            "id {:?} does not match the path below the contracts root ({expected_id:?})",
            raw.id
        ));
    }
    let impact = match Impact::parse(&raw.impact) {
        Some(i) => i,
        None => {
            errors.push(format!("impact {:?} is not F1, F2 or F3", raw.impact));
            Impact::F3
        }
    };
    let play_mode = match raw.play_mode.as_deref() {
        None => PlayMode::Original,
        Some(s) => match PlayMode::parse(s) {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!("play_mode: {e}"));
                PlayMode::Original
            }
        },
    };
    let seed = raw.seed.unwrap_or(0);
    if !raw.mods.is_empty() {
        errors.push("mods are not supported by feel v1; the run is unmodded".into());
    }
    let start = validate_start(&raw.start, def, &mut errors);
    let save = raw
        .save
        .as_ref()
        .and_then(|save| validate_save(save, &start, def, &mut errors));
    let inputs = match parse_action_inputs(&raw.inputs) {
        Ok(inputs) => Some(inputs),
        Err(e) => {
            errors.push(format!("inputs: {e}"));
            None
        }
    };
    let last_tick = inputs.as_ref().map_or(0, |i| u64::from(i.end) + 1);
    let mut asserts = Vec::new();
    for (n, raw_assert) in raw.asserts.iter().enumerate() {
        match validate_assert(raw_assert, def, inputs.as_ref(), last_tick) {
            Ok(assert) => asserts.push(assert),
            Err(e) => errors.push(format!("assert #{}: {e}", n + 1)),
        }
    }
    let mut windows = Vec::new();
    for (n, raw_window) in raw.windows.iter().enumerate() {
        match validate_window(raw_window, def, last_tick) {
            Ok(window) => windows.push(window),
            Err(e) => errors.push(format!("window #{}: {e}", n + 1)),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n  "));
    }
    Ok(Contract {
        id: raw.id,
        impact,
        package: raw.package,
        ruleset: raw.ruleset,
        profile: raw.profile,
        start,
        save,
        play_mode,
        seed,
        mods: raw.mods,
        evidence: raw.evidence,
        inputs: inputs.expect("checked above"),
        asserts,
        windows,
    })
}

/// Resolves a starting checkpoint. A `save` scenario starts the run from it
/// (`StartPoint::FromSave`), so the `start` must name a room; `start.room` and `save.room` must
/// agree when both are given.
fn validate_save(raw: &RawSave, start: &Start, def: &GameDef, errors: &mut Vec<String>) -> Option<SaveSlot> {
    let (start_room, difficulty) = match *start {
        Start::Room { room, difficulty } => (Some(room), difficulty),
        Start::NewGame { difficulty } => (None, difficulty),
    };
    if start_room.is_none() {
        errors.push("save needs a room start; a new game has no checkpoint".into());
        return None;
    }
    let room = match &raw.room {
        Some(name) => match def.rooms.iter().position(|r| &r.name == name) {
            Some(i) => RoomId(u32::try_from(i).expect("room count fits")),
            None => {
                errors.push(format!("save room {name:?} is not in the package"));
                return None;
            }
        },
        None => start_room.expect("checked above"),
    };
    if let (Some(name), Some(start_room)) = (&raw.room, start_room)
        && def.room(start_room).map(|r| r.name.as_str()) != Some(name.as_str())
    {
        errors.push(format!(
            "save room {name:?} does not match the start room {:?}",
            def.room(start_room).map(|r| r.name.as_str()).unwrap_or("?")
        ));
    }
    Some(SaveSlot {
        room,
        x: Real::new(raw.x),
        y: Real::new(raw.y),
        precision: Default::default(),
        flipped: raw.flipped.unwrap_or(false),
        facing: raw.facing.unwrap_or(1),
        difficulty: raw.difficulty.unwrap_or(difficulty),
        deaths: raw.deaths.unwrap_or(0),
        time: raw.time.unwrap_or(0),
        bosses: raw.bosses.unwrap_or(0),
        items: raw.items.unwrap_or(0),
        clear: raw.clear.unwrap_or(false),
    })
}

fn validate_start(raw: &RawStart, def: &GameDef, errors: &mut Vec<String>) -> Start {
    let difficulty = raw.difficulty.unwrap_or(0);
    match (&raw.room, raw.new_game) {
        (Some(_), true) => {
            errors.push("start names both room and new_game; pick one".into());
            Start::NewGame { difficulty }
        }
        (None, false) => {
            errors.push("start needs room or new_game".into());
            Start::NewGame { difficulty }
        }
        (None, true) => Start::NewGame { difficulty },
        (Some(name), false) => match def.rooms.iter().position(|r| &r.name == name) {
            Some(i) => Start::Room {
                room: RoomId(u32::try_from(i).expect("room count fits")),
                difficulty,
            },
            None => {
                errors.push(format!("start room {name:?} is not in the package"));
                Start::NewGame { difficulty }
            }
        },
    }
}

fn validate_assert(
    map: &BTreeMap<String, RawValue>,
    def: &GameDef,
    inputs: Option<&ParsedInputs>,
    last_tick: u64,
) -> Result<Assert, String> {
    let tick = match map.get("tick") {
        Some(RawValue::Integer(i)) if *i >= 0 => u64::try_from(*i).expect("non-negative"),
        _ => return Err("tick is required and must be a non-negative integer".into()),
    };
    if tick > last_tick {
        return Err(format!(
            "tick {tick} is past the inputs' last tick {last_tick} (`frame {}: end`)",
            last_tick.saturating_sub(1)
        ));
    }
    let why = match map.get("why") {
        None => None,
        Some(RawValue::String(s)) if !s.trim().is_empty() => Some(s.clone()),
        Some(_) => return Err("why must be a non-empty string".into()),
    };
    let mut checks = Vec::new();
    let mut errors = Vec::new();
    for (key, value) in map {
        // Dotted metric keys reach us as nested tables (`kid.x = { .. }` is `kid -> x -> { .. }`),
        // so a single top-level key can name several metrics.
        if !is_dedicated(key) {
            match flatten_metric(key, value, "") {
                Ok(metrics) => {
                    for (name, table) in metrics {
                        match metric_check(&name, &table, def, why.as_deref()) {
                            Ok(check) => checks.push(check),
                            Err(e) => errors.push(e),
                        }
                    }
                }
                Err(e) => errors.push(e),
            }
            continue;
        }
        let result = match key.as_str() {
            "tick" | "why" => continue,
            "room" => string(value, key).map(Check::Room),
            "kid_count" => count(value, key).map(Check::KidCount),
            "death" => count(value, key).map(Check::Death),
            "time" => count(value, key).map(Check::Time),
            "clear" => boolean(value, key).map(Check::Clear),
            "status" => string(value, key).and_then(|s| {
                StatusLit::parse(&s)
                    .map(Check::Status)
                    .ok_or_else(|| format!("status {s:?} is not running, ended or halted"))
            }),
            "map" => string(value, key).and_then(|m| {
                let known = def.maps.iter().any(|map| map.id == m) || (def.maps.is_empty() && m == "main");
                if known {
                    Ok(Check::Map(m))
                } else {
                    Err(format!("map {m:?} is not a map of the package"))
                }
            }),
            "control" => {
                table(value, key).and_then(|t| validate_control(t, tick, def, inputs, why.as_deref()))
            }
            "reset_scope" => table(value, key).and_then(|t| validate_reset(t, tick, def, why.as_deref())),
            "sound" => table(value, key).and_then(|t| validate_sound(t, tick, def, last_tick)),
            "rng" => table(value, key).and_then(validate_rng),
            "matches_save" => match value {
                RawValue::Bool(true) => Ok(Check::MatchesSave),
                RawValue::Bool(false) => Err("matches_save = false is not an assertion; remove it".into()),
                _ => Err("matches_save must be true".into()),
            },
            "trace" => string(value, key).map(Check::Trace),
            "screenshot" => Err(
                "screenshot is client evidence, not a sim assertion; put it in the issue or a UI test".into(),
            ),
            other => unreachable!("{other} is not a dedicated key"),
        };
        match result {
            Ok(check) => checks.push(check),
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    if checks.is_empty() {
        return Err("an assert needs at least one check".into());
    }
    Ok(Assert { tick, why, checks })
}

/// The assertion keys with a fixed meaning. Every other key is a probe-registry metric.
fn is_dedicated(key: &str) -> bool {
    matches!(
        key,
        "tick"
            | "why"
            | "room"
            | "kid_count"
            | "death"
            | "time"
            | "clear"
            | "status"
            | "map"
            | "control"
            | "reset_scope"
            | "sound"
            | "rng"
            | "matches_save"
            | "trace"
            | "screenshot"
    )
}

/// A metric's tolerance table, from one leaf of a TOML dotted key.
type ToleranceTable = BTreeMap<String, RawValue>;

/// Flattens TOML dotted keys into probe-registry expressions. `kid.x = { exact = 1 }` reaches us
/// as the table `kid -> x -> { exact = 1 }`; the leaf is a tolerance table.
fn flatten_metric(
    key: &str,
    value: &RawValue,
    prefix: &str,
) -> Result<Vec<(String, ToleranceTable)>, String> {
    let name = if prefix.is_empty() {
        key.to_owned()
    } else {
        format!("{prefix}.{key}")
    };
    match value {
        RawValue::Table(table) if is_tolerance(table) => Ok(vec![(name, table.clone())]),
        RawValue::Table(table) => {
            if table.is_empty() {
                return Err(format!("{name}: an empty table is not a metric"));
            }
            let mut out = Vec::new();
            for (child, child_value) in table {
                out.extend(flatten_metric(child, child_value, &name)?);
            }
            Ok(out)
        }
        // A bare scalar is the exact value, the convenience form the lifecycle contract uses
        // (`save.room = "play"`). It is still an exact assertion, so it needs no `why`.
        _ => Ok(vec![(
            name,
            BTreeMap::from([("exact".to_owned(), value.clone())]),
        )]),
    }
}

fn is_tolerance(table: &BTreeMap<String, RawValue>) -> bool {
    const KEYS: [&str; 6] = ["exact", "min", "max", "within", "of", "why"];
    !table.is_empty() && table.keys().all(|k| KEYS.contains(&k.as_str()))
}

fn metric_check(
    name: &str,
    table: &BTreeMap<String, RawValue>,
    def: &GameDef,
    assert_why: Option<&str>,
) -> Result<Check, String> {
    let column = metric_column(name, def)?;
    if table.is_empty() {
        return Err(format!(
            "{name}: a metric needs a tolerance table (`{{ exact = ... }}`, `{{ min = ..., max = ... }}` or `{{ within = ..., of = ... }}`)"
        ));
    }
    let tolerance = parse_tolerance(name, table, assert_why)?;
    Ok(Check::Metric {
        name: name.to_owned(),
        column,
        tolerance,
    })
}

fn parse_tolerance(
    name: &str,
    table: &BTreeMap<String, RawValue>,
    assert_why: Option<&str>,
) -> Result<Tolerance, String> {
    let (mut exact, mut min, mut max, mut within, mut of, mut why) = (None, None, None, None, None, None);
    for (key, value) in table {
        match key.as_str() {
            "exact" => exact = Some(literal(value, name)?),
            "min" => min = Some(number(value, name, key)?),
            "max" => max = Some(number(value, name, key)?),
            "within" => within = Some(number(value, name, key)?),
            "of" => of = Some(number(value, name, key)?),
            "why" => why = Some(string(value, key)?),
            other => return Err(format!("{name}: unknown tolerance key {other:?}")),
        }
    }
    let tolerance = match (exact, min, max, within, of) {
        (Some(v), None, None, None, None) => Tolerance::Exact(v),
        (None, Some(min), Some(max), None, None) => {
            if min > max {
                return Err(format!("{name}: min {min:?} is above max {max:?}"));
            }
            Tolerance::MinMax { min, max }
        }
        (None, None, None, Some(within), Some(of)) => {
            if within < 0.0 {
                return Err(format!("{name}: within {within:?} is negative"));
            }
            Tolerance::Within { within, of }
        }
        (None, None, None, Some(_), None) => {
            return Err(format!(
                "{name}: `within` needs an `of` centre (`{{ within = 2.0, of = 96.0 }}`)"
            ));
        }
        (None, None, None, None, Some(_)) => {
            return Err(format!("{name}: `of` only goes with `within`"));
        }
        _ => {
            return Err(format!(
                "{name}: use exactly one of `{{ exact = v }}`, `{{ min = v, max = v }}` or `{{ within = w, of = c }}`"
            ));
        }
    };
    let has_why = why.is_some_and(|w| !w.trim().is_empty()) || assert_why.is_some();
    if !tolerance.is_exact() && !has_why {
        return Err(format!(
            "{name}: a non-exact tolerance needs a one-line `why` on the assert or in the table"
        ));
    }
    Ok(tolerance)
}

fn validate_control(
    table: &BTreeMap<String, RawValue>,
    tick: u64,
    def: &GameDef,
    inputs: Option<&ParsedInputs>,
    assert_why: Option<&str>,
) -> Result<Check, String> {
    let (mut action, mut metric, mut latency_max, mut press_frame, mut why) = (None, None, None, None, None);
    for (key, value) in table {
        match key.as_str() {
            "action" => action = Some(string(value, key)?),
            "metric" => metric = Some(string(value, key)?),
            "latency_max" => latency_max = Some(count(value, key)?),
            "press_frame" => press_frame = Some(count(value, key)?),
            "why" => why = Some(string(value, key)?),
            other => return Err(format!("control: unknown key {other:?}")),
        }
    }
    let action_name = action.ok_or("control needs an action")?;
    let action = Action::from_name(&action_name)
        .ok_or_else(|| format!("control action {action_name:?} is not an action name"))?;
    let metric = metric.ok_or("control needs a metric")?;
    let column = metric_column(&metric, def)?;
    let latency_max = latency_max.ok_or("control needs latency_max")?;
    let why = why
        .filter(|w| !w.trim().is_empty())
        .or_else(|| assert_why.map(str::to_owned))
        .ok_or_else(|| format!("control on {metric:?} is a timing window and needs a one-line `why`"))?;
    let inputs = inputs.ok_or("control needs a parsable inputs script")?;
    let presses: Vec<u32> = inputs
        .frames
        .iter()
        .enumerate()
        .filter(|(_, frame)| frame.pressed & action.bit() != 0)
        .map(|(i, _)| u32::try_from(i).expect("input frames fit"))
        .collect();
    let press_frame = match press_frame {
        Some(f) => {
            if !presses.contains(&f) {
                return Err(format!(
                    "scenario out of sync: .inputs frame {f} does not press {}",
                    action.name()
                ));
            }
            f
        }
        None => presses
            .iter()
            .copied()
            .filter(|f| u64::from(*f) < tick)
            .max()
            .ok_or_else(|| {
                format!(
                    "scenario out of sync: the inputs never press {} by tick {tick}",
                    action.name()
                )
            })?,
    };
    Ok(Check::Control {
        action,
        name: metric,
        column,
        press_frame,
        latency_max,
        why,
    })
}

fn validate_reset(
    table: &BTreeMap<String, RawValue>,
    tick: u64,
    def: &GameDef,
    assert_why: Option<&str>,
) -> Result<Check, String> {
    let (mut before, mut after, mut persists, mut resets, mut why) = (None, None, None, None, None);
    for (key, value) in table {
        match key.as_str() {
            "before" => before = Some(count64(value, key)?),
            "after" => after = Some(count64(value, key)?),
            "persists" => persists = Some(names(value, key)?),
            "resets" => resets = Some(names(value, key)?),
            "why" => why = Some(string(value, key)?),
            other => return Err(format!("reset_scope: unknown key {other:?}")),
        }
    }
    let before = before.ok_or("reset_scope needs `before`, the tick before the retry/reset")?;
    let after = after.unwrap_or(tick);
    if before >= after {
        return Err(format!(
            "reset_scope before {before} must be less than after {after}"
        ));
    }
    let persists = resolve_names(persists.unwrap_or_default(), def)?;
    let resets = resolve_names(resets.unwrap_or_default(), def)?;
    if persists.is_empty() && resets.is_empty() {
        return Err("reset_scope needs persists or resets".into());
    }
    let why = why
        .filter(|w| !w.trim().is_empty())
        .or_else(|| assert_why.map(str::to_owned))
        .ok_or_else(|| {
            "reset_scope is a comparison across a transition and needs a one-line `why`".to_owned()
        })?;
    Ok(Check::ResetScope {
        before,
        after,
        persists,
        resets,
        why,
    })
}

fn validate_sound(
    table: &BTreeMap<String, RawValue>,
    tick: u64,
    def: &GameDef,
    last_tick: u64,
) -> Result<Check, String> {
    let (mut event, mut sound, mut sound_tick) = (None, None, None);
    for (key, value) in table {
        match key.as_str() {
            "event" => event = Some(string(value, key)?),
            "sound" => sound = Some(string(value, key)?),
            "tick" => sound_tick = Some(count64(value, key)?),
            other => return Err(format!("sound: unknown key {other:?}")),
        }
    }
    let event_name = event.ok_or("sound needs an event")?;
    let event = SoundEvent::parse(&event_name)
        .ok_or_else(|| format!("sound event {event_name:?} is not play, stop, stop_all, volume or pitch"))?;
    let tick = sound_tick.unwrap_or(tick);
    // A sound event belongs to the tick its step began at; the last step begins at `last_tick - 1`
    // and ends at `last_tick`.
    let last_events_tick = last_tick.saturating_sub(1);
    if tick > last_events_tick {
        return Err(format!(
            "sound tick {tick} is past the last Step's tick {last_events_tick}"
        ));
    }
    let (sound, sound_name) = match (event, sound) {
        (SoundEvent::StopAll, None) => (None, None),
        (SoundEvent::StopAll, Some(s)) => {
            return Err(format!("sound: stop_all takes no sound name (got {s:?})"));
        }
        (_, None) => return Err(format!("sound {} needs a sound name", event.name())),
        (_, Some(name)) => {
            let id = def
                .sounds
                .iter()
                .position(|s| s.name == name)
                .and_then(|i| u32::try_from(i).ok())
                .map(SoundId)
                .ok_or_else(|| format!("sound {name:?} is not a sound of the package"))?;
            (Some(id), Some(name))
        }
    };
    Ok(Check::Sound {
        event,
        sound,
        sound_name,
        tick,
    })
}

fn validate_rng(table: &BTreeMap<String, RawValue>) -> Result<Check, String> {
    let (mut stream, mut seed, mut value) = (None, None, None);
    for (key, raw) in table {
        match key.as_str() {
            "stream" => stream = Some(string(raw, key)?),
            "seed" => seed = Some(integer(raw, key)?),
            "value" => value = Some(integer(raw, key)?),
            other => return Err(format!("rng: unknown key {other:?}")),
        }
    }
    match stream.as_deref() {
        Some("gameplay") => {}
        Some(other) => {
            return Err(format!(
                "rng stream {other:?} is not supported; v1 has `gameplay`"
            ));
        }
        None => return Err("rng needs `stream = \"gameplay\"`".into()),
    }
    if seed.is_none() && value.is_none() {
        return Err("rng needs seed or value".into());
    }
    Ok(Check::Rng { seed, value })
}

fn validate_window(raw: &RawWindow, def: &GameDef, last_tick: u64) -> Result<Window, String> {
    let [first, last] = raw.ticks;
    if first > last {
        return Err(format!("ticks [{first}, {last}] go backwards"));
    }
    if last > last_tick {
        return Err(format!(
            "ticks end at {last}, past the inputs' last tick {last_tick}"
        ));
    }
    if raw.why.trim().is_empty() {
        return Err("a window needs a one-line `why`".into());
    }
    let column = metric_column(&raw.metric, def)?;
    match (raw.max_delta, &raw.envelope) {
        (Some(max_delta), None) if max_delta < 0.0 => {
            return Err(format!("max_delta {max_delta:?} is negative"));
        }
        (Some(_), Some(_)) => return Err("a window takes max_delta or envelope, not both".into()),
        (None, None) => return Err("a window needs max_delta or envelope".into()),
        (None, Some(pairs)) => {
            let want = usize::try_from(last - first + 1).expect("window length fits");
            if pairs.len() != want {
                return Err(format!(
                    "envelope has {} pairs; ticks [{first}, {last}] need exactly {want}",
                    pairs.len()
                ));
            }
            for [min, max] in pairs {
                if min > max {
                    return Err(format!("envelope pair [{min:?}, {max:?}] has min above max"));
                }
            }
        }
        _ => {}
    }
    let envelope = raw
        .envelope
        .as_ref()
        .map(|pairs| pairs.iter().map(|[min, max]| (*min, *max)).collect::<Vec<_>>());
    Ok(Window {
        first,
        last,
        name: raw.metric.clone(),
        column,
        max_delta: raw.max_delta,
        envelope,
        why: raw.why.clone(),
    })
}

/// Resolves a metric expression, rejecting the reads a contract must not make.
fn metric_column(name: &str, def: &GameDef) -> Result<Column, String> {
    let column = registry::column(name, def).map_err(|e| format!("metric {name:?}: {e}"))?;
    match &column {
        Column::Field {
            target: ProbeTarget::Row,
            ..
        } => Err(format!(
            "metric {name:?} reads the row instance; a bare x/y is a load error, name the owner (kid.x, save.x, ...)"
        )),
        Column::Key(_) => Err(format!("metric {name:?} is a keyboard read, not a feel metric")),
        Column::PlaceFree { .. } => Err(format!("metric {name:?} is a place test, not a feel metric")),
        _ => Ok(column),
    }
}

fn resolve_names(names: Vec<String>, def: &GameDef) -> Result<Vec<NamedColumn>, String> {
    names
        .into_iter()
        .map(|name| {
            Ok(NamedColumn {
                column: metric_column(&name, def)?,
                name,
            })
        })
        .collect()
}

fn string(value: &RawValue, key: &str) -> Result<String, String> {
    match value {
        RawValue::String(s) => Ok(s.clone()),
        _ => Err(format!("{key} must be a string")),
    }
}

fn table<'a>(value: &'a RawValue, key: &str) -> Result<&'a BTreeMap<String, RawValue>, String> {
    match value {
        RawValue::Table(t) => Ok(t),
        _ => Err(format!("{key} must be a table")),
    }
}

fn count(value: &RawValue, key: &str) -> Result<u32, String> {
    let n = integer(value, key)?;
    u32::try_from(n).map_err(|_| format!("{key} {n} is out of range"))
}

fn count64(value: &RawValue, key: &str) -> Result<u64, String> {
    let n = integer(value, key)?;
    u64::try_from(n).map_err(|_| format!("{key} {n} is out of range"))
}

fn integer(value: &RawValue, key: &str) -> Result<i64, String> {
    match value {
        RawValue::Integer(i) => Ok(*i),
        _ => Err(format!("{key} must be an integer")),
    }
}

fn number(value: &RawValue, name: &str, key: &str) -> Result<f64, String> {
    match value {
        RawValue::Float(f) => Ok(*f),
        RawValue::Integer(i) => Ok(*i as f64),
        _ => Err(format!("{name}: {key} must be a number")),
    }
}

fn literal(value: &RawValue, name: &str) -> Result<Literal, String> {
    match value {
        RawValue::Float(f) => Ok(Literal::Real(*f)),
        RawValue::Integer(i) => Ok(Literal::Real(*i as f64)),
        RawValue::Bool(b) => Ok(Literal::Bool(*b)),
        RawValue::String(s) => Ok(Literal::Str(s.clone())),
        _ => Err(format!("{name}: exact needs a number, string or flag")),
    }
}

fn boolean(value: &RawValue, key: &str) -> Result<bool, String> {
    match value {
        RawValue::Bool(b) => Ok(*b),
        RawValue::Integer(0) => Ok(false),
        RawValue::Integer(1) => Ok(true),
        _ => Err(format!("{key} must be true or false")),
    }
}

fn names(value: &RawValue, key: &str) -> Result<Vec<String>, String> {
    match value {
        RawValue::Array(items) => items
            .iter()
            .map(|v| string(v, key))
            .collect::<Result<Vec<_>, _>>(),
        RawValue::String(s) => Ok(vec![s.clone()]),
        _ => Err(format!("{key} must be a list of metric names")),
    }
}

// ---------------------------------------------------------------------------------------------
// Evaluation

/// What the sim showed after one tick.
#[derive(Clone, Debug)]
struct Snapshot {
    status: SimStatus,
    room: Option<String>,
    map: String,
    kid_count: u32,
    death: u32,
    time: u32,
    clear: bool,
    kid: Option<(f64, f64)>,
    save: Option<(Option<String>, f64, f64)>,
    metrics: BTreeMap<String, Value>,
    rng: i64,
}

/// Every metric the contract reads, by registry expression.
fn contract_metrics(contract: &Contract) -> BTreeMap<String, Column> {
    let mut out = BTreeMap::new();
    for assert in &contract.asserts {
        for check in &assert.checks {
            match check {
                Check::Metric { name, column, .. } | Check::Control { name, column, .. } => {
                    out.insert(name.clone(), column.clone());
                }
                Check::ResetScope { persists, resets, .. } => {
                    for named in persists.iter().chain(resets) {
                        out.insert(named.name.clone(), named.column.clone());
                    }
                }
                _ => {}
            }
        }
    }
    for window in &contract.windows {
        out.insert(window.name.clone(), window.column.clone());
    }
    out
}

fn snapshot(sim: &mut Sim, metrics: &BTreeMap<String, Column>) -> Snapshot {
    let tick = sim.world().tick;
    let frame = i64::try_from(tick).unwrap_or(i64::MAX);
    let mut values = BTreeMap::new();
    for (name, column) in metrics {
        let value = probe::read_unrowed(sim, column, frame).unwrap_or(Value::Empty);
        values.insert(name.clone(), value);
    }
    let kid = sim
        .world()
        .kid(&sim.rules().tick)
        .and_then(|id| sim.world().get(id))
        .filter(|i| i.alive && i.active)
        .map(|i| (i.m.x().get(), i.m.y().get()));
    let save = sim.world().save.as_ref().map(|s| {
        (
            sim.def().room(s.room).map(|r| r.name.clone()),
            s.x.get(),
            s.y.get(),
        )
    });
    let w = sim.world();
    Snapshot {
        status: w.status.clone(),
        room: sim.def().room(w.room).map(|r| r.name.clone()),
        map: sim.map_id().to_owned(),
        kid_count: w.count_of(ClassMask::KID | ClassMask::KID_FLIPPED),
        death: w.engine.run.deaths,
        time: w.engine.run.time,
        clear: w.engine.run.clear,
        kid,
        save,
        metrics: values,
        rng: i64::from(w.rng.seed.cast_signed()),
    }
}

/// The `SoundOutput` events one step produced. The step that begins at tick N emits tick-N
/// events and ends at tick N+1; `Sim::new`'s first-room output is tick 0 (the first step keeps it).
fn step_sounds(sim: &Sim) -> Vec<SoundOutput> {
    sim.world()
        .output
        .iter()
        .filter_map(|e| match e {
            OutputEvent::Sound(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

/// Runs a validated contract against a game definition. The fixture or package has already been
/// resolved by the caller; this function reads the sim and changes no sim state.
pub fn run(contract: &Contract, def: GameDef) -> Verdict {
    let options = SimOptions {
        start: match &contract.save {
            Some(save) => StartPoint::FromSave(save.clone()),
            None => match contract.start {
                Start::NewGame { difficulty } => StartPoint::NewGame { difficulty },
                Start::Room { room, difficulty } => StartPoint::Room { room, difficulty },
            },
        },
        seed: contract.seed,
        dll_mode: iwbtg_sim::sound::DllMode::Model,
        mode: contract.play_mode,
        map: None,
    };
    let mut sim = match Sim::new(Arc::new(def), options) {
        Ok(sim) => sim,
        Err(e) => {
            return Verdict::Fail(vec![Mismatch {
                tick: 0,
                metric: "start".into(),
                expected: "a startable run".into(),
                got: e.to_string(),
                context: Vec::new(),
            }]);
        }
    };
    let metrics = contract_metrics(contract);
    let mut snapshots = BTreeMap::new();
    snapshots.insert(0, snapshot(&mut sim, &metrics));
    let mut sounds = BTreeMap::new();
    let last = u64::from(contract.inputs.end) + 1;
    for tick in 1..=last {
        if !matches!(sim.world().status, SimStatus::Running) {
            break;
        }
        let frame = contract
            .inputs
            .frames
            .get(usize::try_from(tick - 1).expect("tick fits"))
            .copied()
            .unwrap_or_default();
        // The events belong to the tick the step began at (the pre-step tick), not the tick the
        // step ends at: the av sound contract asserts tick 0 for a Step on the first tick.
        let events_tick = sim.world().tick;
        sim.step(frame);
        sounds.insert(events_tick, step_sounds(&sim));
        // Normally `World::tick == tick`; a halting step leaves the tick where it stopped, and the
        // snapshot still belongs to the step that just ran.
        snapshots.insert(tick, snapshot(&mut sim, &metrics));
    }
    let run = Run {
        snapshots,
        sounds,
        seed: contract.seed,
    };
    let mut mismatches = Vec::new();
    for assert in &contract.asserts {
        for check in &assert.checks {
            if let Some(m) = run.check(assert.tick, check) {
                mismatches.push(m);
            }
        }
    }
    for window in &contract.windows {
        if let Some(m) = run.check_window(window) {
            mismatches.push(m);
        }
    }
    if !mismatches.is_empty() {
        return Verdict::Fail(mismatches);
    }
    if contract.impact == Impact::F1 && contract.evidence.is_empty() {
        return Verdict::Open;
    }
    Verdict::Ok
}

struct Run {
    snapshots: BTreeMap<u64, Snapshot>,
    /// Events by the tick the emitting step began at.
    sounds: BTreeMap<u64, Vec<SoundOutput>>,
    seed: u32,
}

impl Run {
    fn at(&self, tick: u64) -> Option<&Snapshot> {
        self.snapshots.get(&tick)
    }

    fn context(&self, tick: u64, name: &str) -> Vec<(u64, String)> {
        let lo = tick.saturating_sub(5);
        let hi = tick.saturating_add(5);
        self.snapshots
            .range(lo..=hi)
            .filter_map(|(t, s)| s.metrics.get(name).map(|v| (*t, value_text(v))))
            .collect()
    }

    fn check(&self, tick: u64, check: &Check) -> Option<Mismatch> {
        let Some(snap) = self.at(tick) else {
            return Some(Mismatch {
                tick,
                metric: check_name(check).into(),
                expected: "the tick to run".into(),
                got: format!("the run stopped before tick {tick}"),
                context: Vec::new(),
            });
        };
        match check {
            Check::Room(want) => self.simple(
                tick,
                "room",
                want.as_str(),
                snap.room.as_deref().unwrap_or("<no room>"),
                Vec::new(),
            ),
            Check::KidCount(want) => self.simple(tick, "kid_count", *want, snap.kid_count, Vec::new()),
            Check::Death(want) => self.simple(tick, "death", *want, snap.death, Vec::new()),
            Check::Time(want) => self.simple(tick, "time", *want, snap.time, Vec::new()),
            Check::Clear(want) => self.simple(tick, "clear", *want, snap.clear, Vec::new()),
            Check::Status(want) => self.simple(
                tick,
                "status",
                want.name(),
                status_word_of(&snap.status),
                Vec::new(),
            ),
            Check::Map(want) => self.simple(tick, "map", want.as_str(), snap.map.as_str(), Vec::new()),
            Check::Metric { name, tolerance, .. } => {
                let actual = snap.metrics.get(name);
                let ok = actual.is_some_and(|v| tolerance_holds(tolerance, v));
                if ok {
                    return None;
                }
                Some(Mismatch {
                    tick,
                    metric: name.clone(),
                    expected: tolerance.text(),
                    got: actual.map_or_else(|| "<empty>".into(), value_text),
                    context: self.context(tick, name),
                })
            }
            Check::Control {
                action,
                name,
                press_frame,
                latency_max,
                ..
            } => {
                let mut latency = None;
                for response in *press_frame..=press_frame.saturating_add(*latency_max) {
                    let before = self.at(u64::from(response)).and_then(|s| s.metrics.get(name));
                    let after = self.at(u64::from(response) + 1).and_then(|s| s.metrics.get(name));
                    if let (Some(before), Some(after)) = (before, after)
                        && !before.same(after)
                    {
                        latency = Some(response - *press_frame);
                        break;
                    }
                }
                match latency {
                    Some(latency) if latency <= *latency_max => None,
                    Some(latency) => Some(Mismatch {
                        tick,
                        metric: format!("control {} ({name})", action.name()),
                        expected: format!("latency <= {latency_max} frames"),
                        got: format!("{latency} frames"),
                        context: self.context(tick, name),
                    }),
                    None => Some(Mismatch {
                        tick,
                        metric: format!("control {} ({name})", action.name()),
                        expected: format!("a response within {latency_max} frames of frame {press_frame}"),
                        got: "no change".into(),
                        context: self.context(tick, name),
                    }),
                }
            }
            Check::ResetScope {
                before,
                after,
                persists,
                resets,
                ..
            } => {
                let (Some(before_snap), Some(after_snap)) = (self.at(*before), self.at(*after)) else {
                    return Some(Mismatch {
                        tick,
                        metric: "reset_scope".into(),
                        expected: format!("ticks {before} and {after} to run"),
                        got: "a tick was not reached".into(),
                        context: Vec::new(),
                    });
                };
                for (named, equal) in persists
                    .iter()
                    .map(|n| (n, true))
                    .chain(resets.iter().map(|n| (n, false)))
                {
                    let (a, b) = (
                        before_snap.metrics.get(&named.name),
                        after_snap.metrics.get(&named.name),
                    );
                    let same = a.zip(b).is_some_and(|(a, b)| a.same(b));
                    if same != equal {
                        let (a, b) = (
                            a.map_or_else(|| "<empty>".into(), value_text),
                            b.map_or_else(|| "<empty>".into(), value_text),
                        );
                        return Some(Mismatch {
                            tick,
                            metric: if equal {
                                format!("reset_scope.persists ({})", named.name)
                            } else {
                                format!("reset_scope.resets ({})", named.name)
                            },
                            expected: if equal { "unchanged" } else { "changed" }.into(),
                            got: format!("{a} -> {b}"),
                            context: Vec::new(),
                        });
                    }
                }
                None
            }
            Check::Sound {
                event,
                sound,
                sound_name,
                tick: sound_tick,
            } => {
                let Some(events) = self.sounds.get(sound_tick) else {
                    return Some(Mismatch {
                        tick: *sound_tick,
                        metric: format!("sound.{}", event.name()),
                        expected: "the step to run".into(),
                        got: format!("the run has no Step at tick {sound_tick}"),
                        context: Vec::new(),
                    });
                };
                let found = events.iter().any(|s| sound_matches(s, *event, *sound));
                if found {
                    return None;
                }
                let want = match sound_name {
                    Some(name) => format!("{} {name}", event.name()),
                    None => event.name().to_owned(),
                };
                Some(Mismatch {
                    tick: *sound_tick,
                    metric: "sound".into(),
                    expected: want,
                    got: "no matching SoundOutput event".into(),
                    context: Vec::new(),
                })
            }
            Check::Rng { seed, value } => {
                if let Some(want) = seed {
                    let got = i64::from(self.seed.cast_signed());
                    if *want != got {
                        return Some(Mismatch {
                            tick,
                            metric: "rng.seed".into(),
                            expected: want.to_string(),
                            got: got.to_string(),
                            context: Vec::new(),
                        });
                    }
                }
                if let Some(want) = value
                    && *want != snap.rng
                {
                    return Some(Mismatch {
                        tick,
                        metric: "rng.value".into(),
                        expected: want.to_string(),
                        got: snap.rng.to_string(),
                        context: Vec::new(),
                    });
                }
                None
            }
            Check::MatchesSave => {
                let (Some((name, x, y)), Some((kx, ky))) = (&snap.save, snap.kid) else {
                    return Some(Mismatch {
                        tick,
                        metric: "matches_save".into(),
                        expected: "a live kid and a save".into(),
                        got: match (&snap.save, snap.kid) {
                            (None, _) => "no save".into(),
                            (_, None) => "no kid".into(),
                            _ => "no save".into(),
                        },
                        context: Vec::new(),
                    });
                };
                let room_ok = name.as_deref() == snap.room.as_deref();
                let ok = room_ok && kx.to_bits() == x.to_bits() && ky.to_bits() == y.to_bits();
                if ok {
                    return None;
                }
                Some(Mismatch {
                    tick,
                    metric: "matches_save".into(),
                    expected: format!(
                        "kid at the save ({}, {x:?}, {y:?})",
                        name.as_deref().unwrap_or("?")
                    ),
                    got: format!("kid at ({}, {kx:?}, {ky:?})", snap.room.as_deref().unwrap_or("?")),
                    context: Vec::new(),
                })
            }
            Check::Trace(_) => None,
        }
    }

    fn simple<T: PartialEq + std::fmt::Display>(
        &self,
        tick: u64,
        name: &str,
        want: T,
        got: T,
        context: Vec<(u64, String)>,
    ) -> Option<Mismatch> {
        if want == got {
            return None;
        }
        Some(Mismatch {
            tick,
            metric: name.into(),
            expected: format!("{want}"),
            got: format!("{got}"),
            context,
        })
    }

    fn check_window(&self, window: &Window) -> Option<Mismatch> {
        let mut values = Vec::new();
        for tick in window.first..=window.last {
            let Some(snap) = self.at(tick) else {
                return Some(Mismatch {
                    tick,
                    metric: window.name.clone(),
                    expected: format!("ticks {}..={} to run", window.first, window.last),
                    got: format!("the run stopped before tick {tick}"),
                    context: Vec::new(),
                });
            };
            let Some(value) = snap.metrics.get(&window.name).and_then(Value::as_f64) else {
                return Some(Mismatch {
                    tick,
                    metric: window.name.clone(),
                    expected: "a numeric metric".into(),
                    got: snap
                        .metrics
                        .get(&window.name)
                        .map_or_else(|| "<empty>".into(), value_text),
                    context: self.context(tick, &window.name),
                });
            };
            values.push((tick, value));
        }
        let base = values.first().map(|(_, v)| *v).unwrap_or_default();
        for (i, (tick, value)) in values.iter().enumerate() {
            let ok = match (&window.envelope, window.max_delta) {
                (Some(envelope), _) => {
                    let (min, max) = envelope[i];
                    *value >= min && *value <= max
                }
                (None, Some(max_delta)) => (*value - base).abs() <= max_delta,
                (None, None) => true,
            };
            if ok {
                continue;
            }
            let expected = match (&window.envelope, window.max_delta) {
                (Some(envelope), _) => {
                    let (min, max) = envelope[i];
                    format!("in [{min:?}, {max:?}] at tick {tick}")
                }
                (None, Some(max_delta)) => format!("within {max_delta:?} of {base:?}"),
                (None, None) => "any value".into(),
            };
            return Some(Mismatch {
                tick: *tick,
                metric: window.name.clone(),
                expected,
                got: format!("{value:?}"),
                context: self.context(*tick, &window.name),
            });
        }
        None
    }
}

fn check_name(check: &Check) -> &str {
    match check {
        Check::Room(_) => "room",
        Check::KidCount(_) => "kid_count",
        Check::Death(_) => "death",
        Check::Time(_) => "time",
        Check::Clear(_) => "clear",
        Check::Status(_) => "status",
        Check::Map(_) => "map",
        Check::Metric { name, .. } | Check::Control { name, .. } => name,
        Check::ResetScope { .. } => "reset_scope",
        Check::Sound { .. } => "sound",
        Check::Rng { .. } => "rng",
        Check::MatchesSave => "matches_save",
        Check::Trace(_) => "trace",
    }
}

fn tolerance_holds(tolerance: &Tolerance, actual: &Value) -> bool {
    match tolerance {
        Tolerance::Exact(lit) => actual.same(&lit.value()),
        Tolerance::MinMax { min, max } => actual.as_f64().is_some_and(|v| v >= *min && v <= *max),
        Tolerance::Within { within, of } => actual.as_f64().is_some_and(|v| (v - of).abs() <= *within),
    }
}

fn sound_matches(event: &SoundOutput, want: SoundEvent, sound: Option<SoundId>) -> bool {
    match (want, event) {
        (SoundEvent::Play, SoundOutput::Play { target, .. })
        | (SoundEvent::Stop, SoundOutput::Stop { target, .. })
        | (SoundEvent::Volume, SoundOutput::Volume { target, .. })
        | (SoundEvent::Pitch, SoundOutput::Pitch { target, .. }) => {
            sound.is_some_and(|id| matches!(target, SoundTarget::Builtin(t) if *t == id))
        }
        (SoundEvent::StopAll, SoundOutput::StopAll { .. }) => true,
        _ => false,
    }
}

fn status_word_of(status: &SimStatus) -> &'static str {
    match status {
        SimStatus::Running => "running",
        SimStatus::Ended => "ended",
        SimStatus::Halted(_) => "halted",
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Str(s) => format!("{s:?}"),
        Value::Empty => "<empty>".into(),
        other => other.decimal(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(text: &str, id: &str) -> Result<Contract, String> {
        let def = fixtures::fixture("jump").unwrap();
        validate(parse(text).unwrap(), &def, id)
    }

    #[test]
    fn a_bare_x_is_a_load_error() {
        let text = r#"
id = "self/bare"
impact = "F2"
package = "fixture:jump"
start = { room = "room_0" }
inputs = "frame 0: end"
[[assert]]
tick = 0
x = { exact = 0.0 }
"#;
        let err = contract(text, "self/bare").unwrap_err();
        assert!(err.contains("bare x/y"), "{err}");
    }

    #[test]
    fn a_tolerance_needs_a_why() {
        let text = r#"
id = "self/why"
impact = "F2"
package = "fixture:jump"
start = { room = "room_0" }
inputs = "frame 0: end"
[[assert]]
tick = 0
kid_count = 1
kid.x = { within = 1.0, of = 96.0 }
"#;
        let err = contract(text, "self/why").unwrap_err();
        assert!(err.contains("needs a one-line `why`"), "{err}");
    }

    #[test]
    fn an_envelope_must_match_the_window_length() {
        let text = r#"
id = "self/envelope"
impact = "F2"
package = "fixture:jump"
start = { room = "room_0" }
inputs = "frame 4: end"
[[window]]
ticks = [0, 4]
metric = "kid.x"
envelope = [[0.0, 200.0]]
why = "the kid holds still"
"#;
        let err = contract(text, "self/envelope").unwrap_err();
        assert!(err.contains("need exactly 5"), "{err}");
    }

    #[test]
    fn screenshot_is_not_a_sim_assertion() {
        let text = r#"
id = "self/shot"
impact = "F2"
package = "fixture:jump"
start = { room = "room_0" }
inputs = "frame 0: end"
[[assert]]
tick = 0
screenshot = "looks right"
"#;
        let err = contract(text, "self/shot").unwrap_err();
        assert!(err.contains("screenshot is client evidence"), "{err}");
    }

    #[test]
    fn several_dotted_metrics_in_one_assert() {
        let text = r#"
id = "self/dotted"
impact = "F2"
package = "fixture:jump"
start = { room = "room_0" }
inputs = "frame 0: end"
[[assert]]
tick = 0
kid.x = { exact = 96.0 }
kid.y = { exact = 544.0 }
"#;
        let c = contract(text, "self/dotted").unwrap();
        assert_eq!(c.asserts[0].checks.len(), 2);
    }
}
