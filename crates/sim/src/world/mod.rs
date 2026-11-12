//! The mutable world: instances, globals, room state (docs/plans/sim-core.md 1.2, 1.4, 1.5).
//!
//! WP0 declares the structs and ships a minimal `Vec` store (`store.rs`); WP1 owns the module.

pub mod api;
pub mod instance;
pub mod store;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use api::{Flow, SpawnInit};
pub use instance::{InstId, Instance, KindState, Value};

use crate::collide::grid::Grid;
use crate::def::{GameDef, RoomId, VarId};
use crate::error::SimError;
use crate::input_state::InputState;
use crate::num::Real;
use crate::output::OutputEvent;
use crate::rng::Rng;
use crate::room::RoomRequest;
use crate::rules::RunnerRules;
use crate::save::SaveSlot;
use crate::script::ScriptRuntime;
use crate::sound::{DllMode, SoundState};
use crate::stripped::PlayMode;
use crate::timing::TimingState;
use crate::view::View;
use crate::{builtins, kid, run_globals};

/// Everything that changes while a game runs. Every field is hashed by `hash::state_hash` except
/// `output` and `grid` (docs/architecture/sim/hash.md, State hash).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct World {
    /// Ticks since the run started.
    pub tick: u64,
    /// The current room.
    pub room: RoomId,
    /// Ticks since the current room was entered.
    pub room_tick: u64,
    /// GM `room_speed`: the file speed on entry, 50 after the world's Room Start (R14).
    pub room_speed: Real,
    /// Instances sorted by id (plan 1.2): creation order, except that room-placed instances with
    /// a GM8 id (#669) sort below every runtime id. Private to `world` so every
    /// mutable access goes through `World::get_mut`, which keeps the broadphase grid in step
    /// (plan 3.4); read them with `World::instances`.
    pub(in crate::world) instances: Vec<Instance>,
    /// The next runtime id to hand out (checked, plan 10.3); starts at
    /// [`World::first_runtime_id`].
    pub next_id: u64,
    /// Variable names first seen at run time (scripts), plan 1.1.
    pub var_names: BTreeMap<String, VarId>,
    /// GML globals.
    pub globals: BTreeMap<VarId, Value>,
    /// Engine state that is not per instance, split by owner (plan 1.5).
    pub engine: EngineGlobals,
    /// The gameplay random generator (WP2).
    pub rng: Rng,
    /// This tick's input and the direct-read latch (WP1).
    pub input: InputState,
    /// The eight views (WP4b).
    pub views: [View; 8],
    /// One pending room request; the last request wins (Inferred, R13).
    pub pending: Option<RoomRequest>,
    /// Saved instances of persistent rooms (WP4a).
    pub persistent_rooms: BTreeMap<RoomId, Vec<Instance>>,
    /// The save slot; `None` means "save file missing" (WP4b).
    pub save: Option<SaveSlot>,
    /// Which sounds play (WP10); hashed, because `sound_isplaying` changes game state.
    pub sound: SoundState,
    /// Timing trigger state (WP8b): which splits fired, whether the run started, and the
    /// `run_end` flag `game_clear` raises. Hashed and saved, so a restored run splits the same.
    pub timing: TimingState,
    /// Events for the client. Not hashed, not saved.
    #[serde(skip)]
    pub output: Vec<OutputEvent>,
    /// Running, halted or ended.
    pub status: SimStatus,
    /// Original or stripped (ranked), from `SimOptions::mode` (#561). Hashed: the same world
    /// goes on differently in the other mode (cut rooms, kinds, instances).
    #[serde(default)]
    pub mode: PlayMode,
    /// The map the run plays, an index into `GameDef::maps` (`SimOptions::map`, #505): its
    /// timing chain and ranked start. Hashed: two maps' runs never share a state hash.
    #[serde(default)]
    pub map: u16,
    /// A trace setup action ran (plan 10.4). Hashed, so a run that used setup never matches a
    /// clean verification.
    pub setup_applied: bool,
    /// Broadphase cache (WP3). Rebuilt on room entry and savestate restore; never hashed, and
    /// never makes two worlds unequal (`Grid`'s `PartialEq` is always true).
    #[serde(skip)]
    pub grid: Grid,
    /// Room requests made since tick row 0 (`Ctx::request_room` counts them). The phases compare
    /// it before and after each handler to apply R8, so a request is seen even when it was made
    /// inside a nested Create. Reset at rows 0 and 15; never hashed or saved.
    #[serde(skip)]
    pub room_requests: u32,
}

impl World {
    /// An empty world in `room` at tick 0, before any room entry: no instances, next id
    /// [`InstId::FIRST`] (a game sets [`World::first_runtime_id`] before its first room),
    /// `room_speed` 0, RNG seeded with `seed`, no save.
    pub fn new(room: RoomId, seed: u32) -> World {
        World {
            tick: 0,
            room,
            room_tick: 0,
            room_speed: Real::ZERO,
            instances: Vec::new(),
            next_id: InstId::FIRST.0,
            var_names: BTreeMap::new(),
            globals: BTreeMap::new(),
            engine: EngineGlobals::default(),
            rng: Rng::new(seed),
            input: InputState::default(),
            views: Default::default(),
            pending: None,
            persistent_rooms: BTreeMap::new(),
            save: None,
            sound: SoundState::default(),
            timing: TimingState::default(),
            output: Vec::new(),
            status: SimStatus::Running,
            mode: PlayMode::Original,
            map: 0,
            setup_applied: false,
            grid: Grid::default(),
            room_requests: 0,
        }
    }
}

/// Engine globals split by owner (plan 1.5). Each part starts empty; only its owner adds fields.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct EngineGlobals {
    /// WP4b: difficulty, deaths, time, save slot number, clear flag.
    pub run: run_globals::RunGlobals,
    /// WP5a: grav flag, frozen, reverse, game-side key flags.
    pub kid: kid::KidGlobals,
    /// WP6a: trigger models, boss and item flags.
    pub mech: builtins::MechGlobals,
}

/// Whether the sim still runs.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum SimStatus {
    /// Running.
    #[default]
    Running,
    /// Stopped by an error; the world is frozen where it stopped.
    Halted(SimError),
    /// `game_end` ran.
    Ended,
}

/// What every built-in, the kid, rooms and scripts work through. `Sim` builds it for each tick
/// from its own fields (WP1); tests build it by hand.
pub struct Ctx<'a> {
    /// The game.
    pub def: &'a GameDef,
    /// The rules for this run.
    pub rules: &'a RunnerRules,
    /// The world.
    pub w: &'a mut World,
    /// The script host with the game's compiled scripts (WP9). Shared, not `&mut`: a script
    /// that spawns runs the new instance's Create inside `spawn`, which may run another script,
    /// so the host is re-entered while it runs. Its per-call state stays inside the host.
    pub scripts: &'a ScriptRuntime,
    /// DLL sound adapters modeled or stubbed (`SimOptions::dll_mode`; WP10). Every kaenbyou
    /// trace runs in `Stub` mode.
    pub dll_mode: DllMode,
}
