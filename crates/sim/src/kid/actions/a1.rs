//! Kid actions specific to family A1 (kid/A1.md): `platform_gravity_zero`, `fade_in`
//! (docs/plans/sim-core.md 5.3). WP0 creates the file; WP5a owns it.
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
pub enum A1Action {}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &A1Action) -> Result<Flow, SimError> {
    match *action {}
}
