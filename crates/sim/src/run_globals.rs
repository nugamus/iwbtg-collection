//! Run-wide engine globals: difficulty, deaths, time, save slot number, clear flag
//! (docs/plans/sim-core.md 1.5). WP0 declares it empty; WP4b owns and adds the fields.

use serde::{Deserialize, Serialize};

/// Run-wide engine globals.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RunGlobals {}
