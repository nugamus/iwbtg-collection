//! The mutable world: instances, globals, room state (docs/architecture/sim/world.md).

pub mod api;
pub mod instance;
pub mod store;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use api::{Flow, SpawnInit};
pub(crate) use instance::set_element;
pub use instance::{InstId, Instance, KindState, MAX_ARRAY_SIZE, Persistence, Value, assign_bare};

use crate::collide::grid::Grid;
use crate::def::{GameDef, PartSystemDefId, RoomId, VarId};
use crate::error::SimError;
use crate::input_state::InputState;
use crate::num::Real;
use crate::output::OutputEvent;
use crate::particles::{PartSystemId, Particles};
use crate::rng::{Rng, RngStreams};
use crate::room::RoomRequests;
use crate::rules::RunnerRules;
use crate::save::SaveSlot;
use crate::script::ScriptRuntime;
use crate::sound::{DllMode, SoundState};
use crate::stripped::PlayMode;
use crate::timing::TimingState;
use crate::view::View;
use crate::{builtins, kid, run_globals};

/// Everything that changes while a game runs. Every field is hashed by `hash::state_hash` except
/// `output`, `grid`, `room_requests`, `particle_defs` and `particle_clears`
/// (docs/architecture/sim/hash.md, State hash).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct World {
    /// Ticks since the run started.
    pub tick: u64,
    /// The current room.
    pub room: RoomId,
    /// Ticks since the current room was entered.
    pub room_tick: u64,
    /// GM `room_speed`: legacy per-room clock data (R14, R30). The file speed on entry, the
    /// package tick rate after the world's Room Start. `ClockPolicy::Fixed` (standard) never
    /// reads it as a clock; it stays hashed either way.
    pub room_speed: Real,
    /// Instances sorted by id (docs/architecture/sim/world.md): creation order, except that room-placed instances with
    /// a GM8 id (#669) sort below every runtime id. Private to `world` so every
    /// mutable access goes through `World::get_mut`, which keeps the broadphase grid in step
    /// (docs/architecture/sim/collision.md); read them with `World::instances`.
    pub(in crate::world) instances: Vec<Instance>,
    /// The GM instance list order (`World::instance_list`): each stored id's place in the room's
    /// entry sequence — the placed instances in file order, then the carried persistent ones in
    /// their previous list order, then the instances created during the room (research#142).
    /// Depth sorts over it at query time. Hashed: it decides which instance "the first" is.
    pub(crate) room_entry: BTreeMap<InstId, u64>,
    /// The next free value in `room_entry`; reset when a room change starts. Hashed.
    pub(crate) next_entry: u64,
    /// The next runtime id to hand out (checked, docs/architecture/sim/determinism.md); starts at
    /// [`World::first_runtime_id`].
    pub next_id: u64,
    /// Variable names first seen at run time (scripts; docs/architecture/sim/world.md).
    pub var_names: BTreeMap<String, VarId>,
    /// GML globals.
    pub globals: BTreeMap<VarId, Value>,
    /// Engine state that is not per instance, split by owner (docs/architecture/sim/world.md).
    pub engine: EngineGlobals,
    /// The gameplay random generator; the shared stream under `gm8`.
    pub rng: Rng,
    /// The named streams a split layout adds (decision 0005, N10). Hashed: a replay observes the
    /// layout and each stream's state. Under `Shared` they are seeded but never read.
    #[serde(default)]
    pub streams: RngStreams,
    /// This tick's input and the direct-read latch.
    pub input: InputState,
    /// The eight views.
    pub views: [View; 8],
    /// The room requests waiting for the room-change row, in raise order (Inferred, R13).
    /// `RoomRules::nested_request` decides what is kept and when it runs: `LastWins` keeps at most
    /// the request raised last (gm8), `Queue` runs them in raise order before the next tick, and
    /// `QueueNextTick` runs one per following tick (#1294).
    pub pending: RoomRequests,
    /// Instances a **persistent room** keeps while the player is away, restored on return.
    /// Distinct from [`Persistence::Run`] instances, which follow the room change, and
    /// from [`Persistence::Save`] save objects, which write the checkpoint.
    pub persistent_rooms: BTreeMap<RoomId, Vec<Instance>>,
    /// The save slot; `None` means "save file missing".
    pub save: Option<SaveSlot>,
    /// Which sounds play; hashed, because `sound_isplaying` changes game state.
    pub sound: SoundState,
    /// Timing trigger state: which splits fired, whether the run started, and the
    /// `run_end` flag `game_clear` raises. Hashed and saved, so a restored run splits the same.
    pub timing: TimingState,
    /// Particle types, systems, emitters and the live particles that still draw (N10, #764):
    /// `Particles::update` runs after End Step when `NumRules::particles_draw_rng` is set. Hashed,
    /// because the update draws from `rng`.
    #[serde(default)]
    pub particles: Particles,
    /// The `particles/<id>.toml` definition each `particle_system` built-in's system was built
    /// from, by its system in `particles` (#866). For the client's drawing only: the sim writes it
    /// when it builds a system and never reads it, so it is not hashed. A system built otherwise
    /// (a particles-mode `blood_emitter`) has no entry.
    #[serde(default)]
    pub particle_defs: BTreeMap<PartSystemId, PartSystemDefId>,
    /// How many `clear` calls (`part_particles_clear`) each `particle_system` built-in's system
    /// has had, by its system in `particles` (#883): `clear` leaves no other trace in the state,
    /// so the client clears its drawing copy when the count moves. For the client only, like
    /// `particle_defs`: the sim writes it and never reads it, so it is not hashed. A system never
    /// cleared has no entry.
    #[serde(default)]
    pub particle_clears: BTreeMap<PartSystemId, u32>,
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
    /// A trace setup action ran (docs/architecture/sim/determinism.md). Hashed, so a run that
    /// used setup never matches a clean verification.
    pub setup_applied: bool,
    /// Broadphase cache. Rebuilt on room entry and savestate restore; never hashed, and
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
            room_entry: BTreeMap::new(),
            next_entry: 0,
            next_id: InstId::FIRST.0,
            var_names: BTreeMap::new(),
            globals: BTreeMap::new(),
            engine: EngineGlobals::default(),
            rng: Rng::new(seed),
            streams: RngStreams::seeded(seed, crate::rules::rng::StreamLayout::Shared),
            input: InputState::default(),
            views: Default::default(),
            pending: RoomRequests::default(),
            persistent_rooms: BTreeMap::new(),
            save: None,
            sound: SoundState::default(),
            timing: TimingState::default(),
            particles: Particles::default(),
            particle_defs: BTreeMap::new(),
            particle_clears: BTreeMap::new(),
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

/// Engine globals split by area (docs/architecture/sim/world.md). Each part starts empty.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct EngineGlobals {
    /// Difficulty, deaths, time, save slot number, clear flag.
    pub run: run_globals::RunGlobals,
    /// Grav flag, frozen, reverse, game-side key flags.
    pub kid: kid::KidGlobals,
    /// Trigger models, boss and item flags.
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
/// from its own fields; tests build it by hand.
pub struct Ctx<'a> {
    /// The game.
    pub def: &'a GameDef,
    /// The rules for this run.
    pub rules: &'a RunnerRules,
    /// The world.
    pub w: &'a mut World,
    /// The script host with the game's compiled scripts. Shared, not `&mut`: a script
    /// that spawns runs the new instance's Create inside `spawn`, which may run another script,
    /// so the host is re-entered while it runs. Its per-call state stays inside the host.
    pub scripts: &'a ScriptRuntime,
    /// DLL sound adapters modeled or stubbed (`SimOptions::dll_mode`). Every kaenbyou
    /// trace runs in `Stub` mode.
    pub dll_mode: DllMode,
}
