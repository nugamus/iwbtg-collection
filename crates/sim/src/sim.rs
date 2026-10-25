//! The simulation handle the client, server and CLI drive (docs/plans/sim-core.md 2.1, 13 WP0).
//!
//! WP0 declares; WP1 owns the method bodies.
#![allow(unused_variables, reason = "WP0 stubs: WP1 fills the bodies")]

use std::sync::Arc;

use crate::def::GameDef;
use crate::error::SimError;
use crate::input::InputFrame;
use crate::room::StartPoint;
use crate::rules::RunnerRules;
use crate::sound::DllMode;
use crate::timing::Split;
use crate::world::{SimStatus, World};

/// How a run starts.
#[derive(Clone, PartialEq, Debug)]
pub struct SimOptions {
    /// Where the run starts.
    pub start: StartPoint,
    /// The RNG seed (replay header).
    pub seed: u32,
    /// DLL adapters modeled, or stubbed like the recorder's `--stub-dlls`.
    pub dll_mode: DllMode,
}

/// What one tick did.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StepResult {
    /// The world's tick after the step.
    pub tick: u64,
    /// Running, halted or ended after the step.
    pub status: SimStatus,
    /// Timing splits recorded this tick (WP8b).
    pub splits: Vec<Split>,
}

/// One running game.
#[derive(Clone, Debug)]
pub struct Sim {
    pub(crate) def: Arc<GameDef>,
    pub(crate) rules: RunnerRules,
    pub(crate) world: World,
    pub(crate) options: SimOptions,
}

impl Sim {
    /// Sets up the world from `options.start`: rules from the game's runner and engine_rev,
    /// boot instances, the first room, Game Start (`room::start_game`).
    pub fn new(def: Arc<GameDef>, options: SimOptions) -> Result<Sim, SimError> {
        unimplemented!("WP1: Sim::new")
    }

    /// Runs one tick with this input. A halted or ended sim does nothing.
    pub fn step(&mut self, input: InputFrame) -> StepResult {
        unimplemented!("WP1: Sim::step")
    }

    /// The world, read only.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The game.
    pub fn def(&self) -> &GameDef {
        &self.def
    }

    /// The rules of this run.
    pub fn rules(&self) -> &RunnerRules {
        &self.rules
    }

    /// The options the run started with.
    pub fn options(&self) -> &SimOptions {
        &self.options
    }

    /// The state hash of the world (`hash::state_hash`).
    pub fn state_hash(&self) -> [u8; 32] {
        crate::hash::state_hash(&self.world)
    }
}
