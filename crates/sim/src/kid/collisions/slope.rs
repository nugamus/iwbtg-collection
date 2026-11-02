//! Slope collision runs: `block_slope_step`, `slope_por_rise` (docs/plans/sim-core.md 5.3).
//! WP0 creates the file; WP5c owns it.
#![allow(
    unused_variables,
    reason = "WP0 stub: WP5c adds the variants and their bodies"
)]

use serde::Deserialize;

use crate::error::SimError;
use crate::world::{Ctx, Flow, InstId};

/// The collision runs this file implements, by their `physics.toml` names (snake case). Part
/// of `kid::KidCollisionRun`; WP5c adds one variant per run it implements.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlopeRun {}

/// Runs one collision run for the kid `me` against `other`. `kid::run_collision` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, other: InstId, run: &SlopeRun) -> Result<Flow, SimError> {
    match *run {}
}
