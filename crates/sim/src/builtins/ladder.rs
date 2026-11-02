//! Built-in `ladder` (docs/plans/sim-core.md 6.1; docs/format/builtins/ladder.md). Tier 3.
//! A marker (mechanics `builtin.ladder`): the climbing is the kid's `ladder` action (A1 Step action 2).
//!
//! The loader gives the kind the `ladder` class (`def/class.rs` LADDER), which the kid's `ladder`
//! action tests with `place_meeting`. The built-in itself has no events, no params and no state.

use super::{BuiltinState, ParamError, ParamSpec};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

/// No params.
pub fn param_specs() -> &'static [ParamSpec] {
    &[]
}

/// Handles no event.
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet::default()
}

/// No state.
pub fn init_state(_params: &Params) -> Result<BuiltinState, ParamError> {
    Ok(BuiltinState::Stateless)
}

/// Never called (no events); does nothing.
pub fn on_event(_ctx: &mut Ctx, _me: InstId, _ev: &EventKind) -> Result<Flow, SimError> {
    Ok(Flow::Continue)
}
