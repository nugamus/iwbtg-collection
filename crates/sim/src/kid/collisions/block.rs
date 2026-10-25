//! The `block_standard` collision run: horizontal contact, vertical contact with the landing
//! refresh, diagonal case (docs/plans/sim-core.md 5.3). WP0 creates the file; WP5a owns it.
#![allow(
    unused_variables,
    reason = "WP0 stub: WP5a adds the variants and their bodies"
)]

use serde::Deserialize;

use crate::error::SimError;
use crate::world::{Ctx, Flow, InstId};

/// The collision runs this file implements, by their `physics.toml` names (snake case). Part
/// of `kid::KidCollisionRun`; WP5a adds one variant per run it implements.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockRun {}

/// Runs one collision run for the kid `me` against `other`. `kid::run_collision` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, other: InstId, run: &BlockRun) -> Result<Flow, SimError> {
    match *run {}
}
