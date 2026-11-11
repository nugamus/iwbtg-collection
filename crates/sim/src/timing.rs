//! Timing triggers and splits (docs/plans/sim-core.md 2.1 row 16, 9.4).
//!
//! Timing counts ticks, never rendered frames. Evaluated at tick row 16 (WP8b).
//!
//! The triggers form one ordered chain: `start`, then each stage in order, then `end`. A trigger
//! fires at most once. Nothing fires before `start`. A stage is looked at only after the stage
//! before it fired, so stages never fire out of order, and several triggers may fire in one tick
//! (they are listed in chain order with the same tick). `end` may fire with stages still
//! unreached; its index is still `stages.len() + 1`. After `end` fired, nothing fires.
//!
//! The tick of a split is `World::tick` after row 16's increment: the number of ticks run so far.
//! So `run_start` fires at tick 1 (the first tick of a new game) and a run's time is
//! `end.tick - start.tick`.
//!
//! Trigger conditions: `run_start` is true at once; `run_end` is true in the tick `game_clear`
//! (or anything else) called [`raise_run_end`]; `enter_room(r)` is true while the world is in
//! room `r` at row 16, which is the first tick after the room change (or the start of the run
//! in that room) because the chain only looks at it once the trigger before it fired.

use serde::{Deserialize, Serialize};

use crate::def::{GameDef, RoomId};
use crate::error::SimError;
use crate::world::World;

/// One trigger from the `[timing]` vocabulary.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TimingTrigger {
    /// The run starts (first tick of a new game).
    RunStart,
    /// The run ends (`game_clear` raised it).
    RunEnd,
    /// The kid enters this room.
    EnterRoom(RoomId),
}

/// The game's `[timing]` table.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TimingDef {
    /// Starts the timer.
    pub start: TimingTrigger,
    /// Stops the timer; a verified run must reach it.
    pub end: TimingTrigger,
    /// Stage splits, in order.
    pub stages: Vec<TimingTrigger>,
}

impl Default for TimingDef {
    fn default() -> TimingDef {
        TimingDef {
            start: TimingTrigger::RunStart,
            end: TimingTrigger::RunEnd,
            stages: Vec::new(),
        }
    }
}

/// A split: which trigger fired, at which tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Split {
    /// 0 is the start, `1..=stages.len()` the stages, `stages.len() + 1` the end.
    pub index: u16,
    /// The tick it fired at.
    pub tick: u64,
}

/// Timing state kept in `World::timing` (hashed with the world).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct TimingState {
    /// Set by [`raise_run_end`] (`game_clear` and anything else that ends the run); read and
    /// cleared by [`evaluate`] at tick row 16.
    pub run_end_raised: bool,
    /// The `start` trigger fired.
    pub started: bool,
    /// How many stage triggers fired; the next one to look at.
    pub stages_done: u16,
    /// The `end` trigger fired.
    pub ended: bool,
    /// Ranked time: ticks run while the timer ran (after `start`, up to and including the tick
    /// `end` fired in) and no cutscene or dialogue region held (`stripped::begin_tick`, #34).
    /// With the frozen ticks it adds up to `end.tick - start.tick`.
    #[serde(default)]
    pub ranked_ticks: u64,
    /// Ticks the timer ran while a region held: frozen, not ranked time (#34).
    #[serde(default)]
    pub frozen_ticks: u64,
    /// Per dialogue region (`StrippedDef::dialogues`, same order): ticks it has held since it
    /// began or since the last press of its `advance`. Empty until a stripped run's first tick,
    /// and in Play original.
    #[serde(default)]
    pub dialogue_waits: Vec<u32>,
}

/// Raises the `run_end` trigger for this tick's row 16. The `game_clear` built-in (WP6b) calls
/// this; nothing else needs to know how timing works.
pub fn raise_run_end(w: &mut World) {
    w.timing.run_end_raised = true;
}

/// Tick row 16: the splits whose trigger fired this tick. Updates `World::timing`, so a
/// trigger fires once (a stage split is not repeated on later ticks in the same room).
pub fn evaluate(w: &mut World, def: &GameDef) -> Result<Vec<Split>, SimError> {
    // A raise only counts for the tick it was made in.
    let raised = std::mem::take(&mut w.timing.run_end_raised);
    let mut out = Vec::new();
    if w.timing.ended {
        return Ok(out);
    }
    let timing = &def.timing;
    let fired = |t: TimingTrigger, w: &World| match t {
        TimingTrigger::RunStart => true,
        TimingTrigger::RunEnd => raised,
        TimingTrigger::EnterRoom(r) => w.room == r,
    };
    let tick = w.tick;
    if !w.timing.started {
        if !fired(timing.start, w) {
            return Ok(out);
        }
        w.timing.started = true;
        out.push(Split { index: 0, tick });
    }
    while let Some(stage) = timing.stages.get(usize::from(w.timing.stages_done)) {
        if !fired(*stage, w) {
            break;
        }
        w.timing.stages_done = w
            .timing
            .stages_done
            .checked_add(1)
            .ok_or_else(|| SimError::overflow("timing stages"))?;
        out.push(Split {
            index: w.timing.stages_done,
            tick,
        });
    }
    if fired(timing.end, w) {
        w.timing.ended = true;
        let index =
            u16::try_from(timing.stages.len() + 1).map_err(|_| SimError::overflow("timing stages"))?;
        out.push(Split { index, tick });
    }
    Ok(out)
}
