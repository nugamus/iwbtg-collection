//! Kid state (docs/plans/sim-core.md 5.1).
//!
//! WP0 declares the split; WP5a owns this file and adds the core fields (`djump`, `cur_jumps`,
//! `on_platform`, `frozen`, `facing`, ...). Family B fields live in `KidStateB` (WP5b), the rest
//! in `KidStateExt` (WP5c), so no WP edits another's struct.

use serde::{Deserialize, Serialize};

use super::state_b::KidStateB;
use super::state_ext::KidStateExt;

/// One kid instance's state.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct KidState {
    /// Family B fields (WP5b).
    pub b: KidStateB,
    /// Families A2-A5 and C fields (WP5c).
    pub ext: KidStateExt,
}

/// Kid globals that persist while no kid exists: grav flag, frozen, reverse, game-side latched
/// key flags (A3 Inferred, I6). WP5a adds the fields.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidGlobals {}
