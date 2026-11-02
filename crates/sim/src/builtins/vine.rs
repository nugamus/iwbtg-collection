//! Built-in `vine` (WalljumpL, WalljumpR; mechanics `vine`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/vine.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! No events: all the logic is in the kid, which reads the `vine_l` and `vine_r` classes. `side`
//! picks the class and `mask_columns` the mask strip, both applied at load; this file declares
//! and checks the params.

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_enum};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

const SIDES: &[&str] = &["left", "right"];

const SPECS: &[ParamSpec] = &[ParamSpec {
    name: "side",
    ty: ParamType::Enum(SIDES),
    required: true,
}];

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: nothing.
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet::default()
}

/// A new instance's state: none; `side` is required.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    param_enum(params, "side", SIDES)?
        .ok_or_else(|| super::param_error("side", "a vine needs `side` (left or right)"))?;
    Ok(BuiltinState::Stateless)
}

/// A vine has no events.
pub fn on_event(_ctx: &mut Ctx, _me: InstId, _ev: &EventKind) -> Result<Flow, SimError> {
    Ok(Flow::Continue)
}
