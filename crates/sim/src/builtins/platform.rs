//! Built-in `platform` (mechanics `platform`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/platform.md). Tier 1. WP0 creates the file; WP6a owns it.
//!
//! The one-way platform has no events: it is not solid, usually invisible, its mask is a strip
//! (32x8 rows 0-7 in rely) and the kid's own "Collision with platform" event does the work. The
//! loader gives it the classes `platform | block` (`parent_kind = "block"`) or
//! `platform | solidblock` (`"solidblock"`, RORI and FT, so it fires the kid's solidblock
//! event). `width` sets the mask width and `parent_kind` the classes, both at load; the sim
//! reads neither, this file only declares and checks the params.

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_real};
use crate::def::{KindDef, Params};
use crate::events::EventSet;

const PARENTS: &[&str] = &["block", "solidblock"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "width",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "parent_kind",
        ty: ParamType::Enum(PARENTS),
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

/// A new instance's state: none, but the params are checked (`width` must be positive).
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    param_enum(params, "parent_kind", PARENTS)?;
    if let Some(w) = param_real(params, "width")?
        && w.get() <= 0.0
    {
        return Err(super::param_error("width", "must be positive"));
    }
    Ok(BuiltinState::Stateless)
}

/// A platform has no events.
pub fn on_event(
    _ctx: &mut crate::world::Ctx,
    _me: crate::world::InstId,
    _ev: &crate::events::EventKind,
) -> Result<crate::world::Flow, crate::error::SimError> {
    Ok(crate::world::Flow::Continue)
}
