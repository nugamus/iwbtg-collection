//! Built-in `spike` (mechanics `spike`; collision spikes Confirmed; docs/plans/sim-core.md 6.1).
//! Tier 1. WP0 creates the file; WP6a owns it.
#![allow(unused_variables, reason = "WP0 stubs: WP6a fills the bodies")]

use super::{BuiltinState, ParamError, ParamSpec};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    unimplemented!("WP6a: spike::param_specs")
}

/// What a kind with this base handles (`builtins::event_set` routes here).
pub fn event_set(kind: &KindDef) -> EventSet {
    unimplemented!("WP6a: spike::event_set")
}

/// A new instance's state from its params (`builtins::init_state` routes here). Add a
/// `BuiltinState` variant in `state.rs` for per-instance state, or return `Stateless`.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    unimplemented!("WP6a: spike::init_state")
}

/// Runs one event on an instance of this built-in (`builtins::on_event` routes here).
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    unimplemented!("WP6a: spike::on_event")
}
