//! Kid actions shared by every family: `create_defaults`, `ensure_world`, `create_bow`,
//! `first_play_save`, `arm_play_timer`, `walk`, `air_sprite`, `fall_cap`, `shoot`, `jump`,
//! `release`, `vines`, `kill` and the other rows of docs/plans/sim-core.md 5.3 marked 5a.
//! WP0 creates the file; WP5a owns it.
#![allow(
    unused_variables,
    reason = "WP0 stub: WP5a adds the variants and their bodies"
)]

use serde::Deserialize;

use crate::error::SimError;
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`; WP5a adds one variant per action it implements.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommonAction {}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &CommonAction) -> Result<Flow, SimError> {
    match *action {}
}
