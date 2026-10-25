//! Run-wide engine globals: difficulty, deaths, time, save slot number, clear flag
//! (docs/plans/sim-core.md 1.5). WP0 declares `difficulty`, which `room::start_game` (WP4a)
//! sets from the `StartPoint`; WP4b owns the file and adds the other fields.

use serde::{Deserialize, Serialize};

/// Run-wide engine globals.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RunGlobals {
    /// The engines' `global.difficulty`, as the game numbers it. Set by `room::start_game` from
    /// `StartPoint::NewGame` or `StartPoint::Room`, by the `difficulty_warp` built-in, and by a
    /// save load.
    pub difficulty: u8,
}
