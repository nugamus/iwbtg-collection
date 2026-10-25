//! Timing triggers and splits (docs/plans/sim-core.md 2.1 row 16, 9.4).
//!
//! Timing counts ticks, never rendered frames. Evaluated at tick row 16. WP0 declares; WP8b
//! owns.
#![allow(unused_variables, reason = "WP0 stub: WP8b fills the body")]

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

/// Tick row 16: the splits whose trigger fired this tick.
pub fn evaluate(w: &World, def: &GameDef) -> Result<Vec<Split>, SimError> {
    unimplemented!("WP8b: timing::evaluate")
}
