//! Built-in `decor` (docs/plans/sim-core.md 6.1; docs/format/builtins/decor.md). Tier 3.
//! Sprite-only objects with no behavior (mechanics `builtin.decor`): no events, no params.

use super::{BuiltinState, ParamError, ParamSpec};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

/// No params.
pub fn param_specs() -> &'static [ParamSpec] {
    &[]
}

/// Handles no event: the kind's own animation and depth are kind data.
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
