//! Built-in `kid_param_zone` (mechanics; docs/plans/sim-core.md 6.1): calls
//! `kid::api::apply_param_zone`. Tier 3. WP0 creates the file; WP6c owns it.
#![allow(unused_variables, reason = "WP0 stubs: WP6c fills the bodies")]

use super::{BuiltinState, ParamError, ParamSpec};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    unimplemented!("WP6c: kid_param_zone::param_specs")
}

/// What a kind with this base handles (`builtins::event_set` routes here).
pub fn event_set(kind: &KindDef) -> EventSet {
    unimplemented!("WP6c: kid_param_zone::event_set")
}

/// A new instance's state from its params (`builtins::init_state` routes here). Add a
/// `BuiltinState` variant in `state.rs` for per-instance state, or return `Stateless`.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    unimplemented!("WP6c: kid_param_zone::init_state")
}

/// Runs one event on an instance of this built-in (`builtins::on_event` routes here).
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    unimplemented!("WP6c: kid_param_zone::on_event")
}
