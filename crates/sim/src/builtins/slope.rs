//! Built-in `slope` (mechanics `slope`; docs/plans/sim-core.md 6.1; docs/format/builtins/
//! slope.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! No events: a slope is solid with a precise mask, and the climb and snap rules belong to the
//! kid (the `[slope]` model of the physics profile). `shape` and a negative `scale_x` pick the
//! class (`slope` or `slope_mirror`) at load (`format::objects::param_classes`); the mask is the
//! sprite's. This file declares and checks the params.

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_real};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

const SHAPES: &[&str] = &["rise_right", "rise_left", "precise_sprite"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "shape",
        ty: ParamType::Enum(SHAPES),
        required: false,
    },
    ParamSpec {
        name: "scale_x",
        ty: ParamType::Real,
        required: false,
    },
];

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: nothing.
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet::default()
}

/// A new instance's state: none; the params are checked.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    param_enum(params, "shape", SHAPES)?;
    param_real(params, "scale_x")?;
    Ok(BuiltinState::Stateless)
}

/// A slope has no events.
pub fn on_event(_ctx: &mut Ctx, _me: InstId, _ev: &EventKind) -> Result<Flow, SimError> {
    Ok(Flow::Continue)
}
