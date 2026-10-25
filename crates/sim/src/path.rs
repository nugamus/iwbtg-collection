//! Path following (docs/plans/sim-core.md 3.1, M8; UNKNOWN U9, U-MECH-4). Tier 2.
//!
//! `path::advance` is the only path code. WP0 declares; WP2 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP2 fills the bodies")]

use serde::{Deserialize, Serialize};

use crate::def::{GameDef, PathId};
use crate::error::SimError;
use crate::motion::Motion;
use crate::num::Real;

/// A running path on one instance.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct PathRun {
    /// The path.
    pub path: PathId,
    /// GM `path_position`, 0 to 1.
    pub position: Real,
    /// GM `path_speed`.
    pub speed: Real,
    /// What happens at the end.
    pub end_action: PathEnd,
    /// Absolute (true) or relative to the start position.
    pub absolute: bool,
    /// GM `path_scale`.
    pub scale: Real,
    /// Start position for a relative path.
    pub xstart: Real,
    /// Start position for a relative path.
    pub ystart: Real,
}

/// GM `path_endaction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PathEnd {
    /// Stop.
    Stop,
    /// Restart from the beginning.
    Restart,
    /// Continue from the current position.
    Continue,
    /// Reverse.
    Reverse,
}

/// GML `path_start`.
pub fn start(
    m: &Motion,
    path: PathId,
    speed: Real,
    end_action: PathEnd,
    absolute: bool,
) -> Result<PathRun, SimError> {
    unimplemented!("WP2: path::start")
}

/// One tick of path following inside automatic motion. Returns true when the path ended.
pub fn advance(run: &mut PathRun, m: &mut Motion, def: &GameDef) -> Result<bool, SimError> {
    unimplemented!("WP2: path::advance")
}

/// GML `path_end`.
pub fn end(run: &mut Option<PathRun>) {
    unimplemented!("WP2: path::end")
}
