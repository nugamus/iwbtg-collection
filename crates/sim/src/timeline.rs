//! Running timelines (docs/plans/sim-core.md 2.1 row 3, R6, R6b). WP0 declares; WP1 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP1 fills the bodies")]

use serde::{Deserialize, Serialize};

use crate::def::TimelineId;
use crate::error::SimError;
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

/// A timeline running on one instance.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TimelineRun {
    /// The timeline.
    pub timeline: TimelineId,
    /// GM `timeline_position`.
    pub position: Real,
    /// GM `timeline_speed`.
    pub speed: Real,
    /// GM `timeline_running`.
    pub running: bool,
    /// GM `timeline_loop`.
    pub looping: bool,
}

/// GML Set Time Line: moment N runs N steps after the setting tick (R6b default).
pub fn start(run: &mut Option<TimelineRun>, timeline: TimelineId, position: Real) -> Result<(), SimError> {
    unimplemented!("WP1: timeline::start")
}

/// Tick row 3 for one instance: advance, run the moments passed (R6).
pub fn advance(ctx: &mut Ctx, me: InstId) -> Result<Flow, SimError> {
    unimplemented!("WP1: timeline::advance")
}
