//! Restart and reset models (docs/plans/sim-core.md 4.3, R16, R17, R26, R27).
//!
//! The engines' init rooms and world objects are not ported as GM code. The `world` built-in
//! (WP6a) calls these at the world's rank, in the phase `RestartModel::trigger` names.
//! WP0 declares; WP4b owns.
#![allow(unused_variables, reason = "WP0 stubs: WP4b fills the bodies")]

use crate::error::SimError;
use crate::world::{Ctx, Flow, InstId};

/// Runs `GameDef::restart` (the R key), called by the world instance `world`.
pub fn run_restart(ctx: &mut Ctx, world: InstId) -> Result<Flow, SimError> {
    unimplemented!("WP4b: restart::run_restart")
}

/// Runs `GameDef::reset` (F2), called by the world instance `world`.
pub fn run_reset(ctx: &mut Ctx, world: InstId) -> Result<Flow, SimError> {
    unimplemented!("WP4b: restart::run_reset")
}
