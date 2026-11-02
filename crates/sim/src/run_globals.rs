//! Run-wide engine globals: difficulty, deaths, time, save slot number, clear flag
//! (docs/plans/sim-core.md 1.5). WP0 declared `difficulty`, which `room::start_game` (WP4a)
//! sets from the `StartPoint`; WP4b owns the file and adds the other fields.

use serde::{Deserialize, Serialize};

use crate::error::SimError;

/// Run-wide engine globals.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RunGlobals {
    /// The engines' `global.difficulty`, as the game numbers it. Set by `room::start_game` from
    /// `StartPoint::NewGame` or `StartPoint::Room`, by the `difficulty_warp` built-in, and by a
    /// save load.
    pub difficulty: u8,
    /// The death counter (`global.death`). Checked: an overflow halts the sim (plan 10.3).
    pub deaths: u32,
    /// The play time counter, in the unit the game counts (`global.time`). Checked.
    pub time: u32,
    /// The save slot number (`global.slot`, 1 unless the game has a slot menu).
    pub slot: u8,
    /// The game was cleared (`global.clear`).
    pub clear: bool,
    /// The retry that is waiting in the boot room (see [`BootLoad`]).
    pub boot_load: Option<BootLoad>,
}

/// A `GameRestartViaBoot` retry that has not finished loading (plan 4.3).
///
/// In the original the temp file holds the slot number, and the init object in rInit loads
/// from it in its Step. The sim has no temp file, so this marker is it: `restart::run_restart`
/// sets it, `room::game_restart` keeps it (the file survives `game_restart`, the globals do
/// not), `restart::boot_step` (called by the `world` built-in's Step) counts it down, and
/// `restart::run_reset` deletes it (F2 deletes the temp file).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BootLoad {
    /// Boot ticks left, counting the tick that loads.
    pub ticks_left: u8,
}

impl RunGlobals {
    /// Adds `n` deaths.
    pub fn add_deaths(&mut self, n: u32) -> Result<(), SimError> {
        self.deaths = self
            .deaths
            .checked_add(n)
            .ok_or_else(|| SimError::overflow("global.death"))?;
        Ok(())
    }

    /// Adds `n` to the play time counter.
    pub fn add_time(&mut self, n: u32) -> Result<(), SimError> {
        self.time = self
            .time
            .checked_add(n)
            .ok_or_else(|| SimError::overflow("global.time"))?;
        Ok(())
    }
}
