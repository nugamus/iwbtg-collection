//! Kid actions specific to family A4 (kid/A4.md: machinary scale layers, ground jump without
//! djump; docs/architecture/sim/kid.md).
#![allow(unused_variables, reason = "no family A4 action is implemented yet")]

use serde::Deserialize;

use crate::error::SimError;
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum A4Action {}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &A4Action) -> Result<Flow, SimError> {
    match *action {}
}
