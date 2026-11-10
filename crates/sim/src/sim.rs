//! The simulation handle the client, server and CLI drive (docs/plans/sim-core.md 2.1, 13 WP0).
//!
//! WP0 declared; WP1 owns the method bodies.

use std::sync::Arc;

use crate::def::GameDef;
use crate::error::SimError;
use crate::input::InputFrame;
use crate::room::{self, StartPoint};
use crate::rules::RunnerRules;
use crate::script::ScriptRuntime;
use crate::sound::DllMode;
use crate::tick;
use crate::timing::Split;
use crate::world::{Ctx, SimStatus, World};

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
    /// The compiled scripts, lent to each `Ctx` as `ctx.scripts` (WP9).
    pub(crate) scripts: ScriptRuntime,
    /// Between `step_rows` and `finish_step`.
    pub(crate) rows_done: bool,
}

impl Sim {
    /// Sets up the world from `options.start`: rules from the game's runner and engine_rev,
    /// the script host (`ScriptRuntime::new`), boot instances, the first room, Game Start
    /// (`room::start_game`). Every `Ctx` it builds carries `options.dll_mode`.
    ///
    /// The world starts in `GameDef::start_room` at tick 0 before `start_game` moves it. A
    /// start that halts is returned as the error.
    pub fn new(def: Arc<GameDef>, options: SimOptions) -> Result<Sim, SimError> {
        let mut rules = RunnerRules::for_runner(def.runner, def.engine_rev)?;
        rules.num.float_model = def.float_model;
        let scripts = ScriptRuntime::new(&def, &rules)?;
        let mut world = World::new(def.start_room, options.seed);
        {
            let mut ctx = Ctx {
                def: &def,
                rules: &rules,
                w: &mut world,
                scripts: &scripts,
                dll_mode: options.dll_mode,
            };
            room::start_game(&mut ctx, &options.start)?;
        }
        if let SimStatus::Halted(e) = &world.status {
            return Err(e.clone());
        }
        Ok(Sim {
            def,
            rules,
            world,
            options,
            scripts,
            rows_done: false,
        })
    }

    /// Runs one tick with this input. A halted or ended sim does nothing. `World::output` holds
    /// the client events of this tick only (cleared first), except that the first step (tick 0)
    /// keeps what `Sim::new` produced: the first room's start (music, sounds, shake) is reported
    /// with tick 0, so a client that reads `output` after every step never misses it and must
    /// not also read it after `Sim::new`. Reaching `GameDef::max_ticks` halts
    /// with `SimError::Overflow`; any error halts (plan 2.2).
    pub fn step(&mut self, input: InputFrame) -> StepResult {
        self.step_rows(input);
        self.finish_step()
    }

    /// Rows 0-14 of [`Sim::step`]. Returns whether a room change is pending: the world then is
    /// what the original's recorder samples on the tick of a room change (the old room's
    /// instances with the kid already moved, #450). Call [`Sim::finish_step`] next; `step` does.
    pub fn step_rows(&mut self, input: InputFrame) -> bool {
        self.rows_done = false;
        if self.world.status == SimStatus::Running {
            if self.world.tick > 0 {
                self.world.output.clear();
            }
            let mut ctx = Ctx {
                def: &self.def,
                rules: &self.rules,
                w: &mut self.world,
                scripts: &self.scripts,
                dll_mode: self.options.dll_mode,
            };
            if ctx.w.tick >= ctx.def.max_ticks {
                ctx.halt(SimError::overflow(format!("max_ticks ({})", ctx.def.max_ticks)));
            } else {
                self.rows_done = tick::run_rows(&mut ctx, input, &tick::Hooks::ENGINE).is_some();
            }
        }
        self.rows_done && self.world.pending.is_some()
    }

    /// Rows 15-16 of [`Sim::step`] (the room change, the sound tick, the tick count). Does nothing
    /// after a halted [`Sim::step_rows`].
    pub fn finish_step(&mut self) -> StepResult {
        let mut splits = Vec::new();
        if std::mem::take(&mut self.rows_done) {
            let mut ctx = Ctx {
                def: &self.def,
                rules: &self.rules,
                w: &mut self.world,
                scripts: &self.scripts,
                dll_mode: self.options.dll_mode,
            };
            match tick::finish_tick(&mut ctx, &tick::Hooks::ENGINE) {
                Ok(s) => splits = s,
                Err(e) => {
                    ctx.halt(e);
                }
            }
        }
        StepResult {
            tick: self.world.tick,
            status: self.world.status.clone(),
            splits,
        }
    }

    /// The world, read only.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The game.
    pub fn def(&self) -> &GameDef {
        &self.def
    }

    /// Turns the `keyboard_check_direct` latch off or on for this run. The trace runner turns it
    /// off, as the recorder has no latch (CLONE_SPEC input, Confirmed (recorder)).
    pub fn set_direct_latch(&mut self, latch: crate::rules::tick::DirectLatch) {
        self.rules.tick.direct_latch = latch;
    }

    /// The rules of this run.
    pub fn rules(&self) -> &RunnerRules {
        &self.rules
    }

    /// The options the run started with.
    pub fn options(&self) -> &SimOptions {
        &self.options
    }

    /// The script host of this run.
    pub fn scripts(&self) -> &ScriptRuntime {
        &self.scripts
    }

    /// The state hash of the world (`hash::state_hash`).
    pub fn state_hash(&self) -> [u8; 32] {
        crate::hash::state_hash(&self.world)
    }
}
