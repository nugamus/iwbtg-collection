//! Family B collision runs (kid/B.md; docs/architecture/sim/kid.md).
#![allow(unused_variables, reason = "no family B collision run is implemented yet")]

use serde::Deserialize;

use crate::error::SimError;
use crate::world::{Ctx, Flow, InstId};

/// The collision runs this file implements, by their `physics.toml` names (snake case). Part
/// of `kid::KidCollisionRun`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BRun {}

/// Runs one collision run for the kid `me` against `other`. `kid::run_collision` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, other: InstId, run: &BRun) -> Result<Flow, SimError> {
    match *run {}
}
