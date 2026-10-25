//! Slip blocks and ice: `[ice] model = "slip"`, `ice_temp_hspeed` (docs/plans/sim-core.md 5.3,
//! N11). WP0 creates the file; WP5c owns it.
#![allow(
    unused_variables,
    reason = "WP0 stub: WP5c adds the variants and their bodies"
)]

use serde::Deserialize;

use crate::error::SimError;
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`; WP5c adds one variant per action it implements.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IceAction {}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &IceAction) -> Result<Flow, SimError> {
    match *action {}
}
