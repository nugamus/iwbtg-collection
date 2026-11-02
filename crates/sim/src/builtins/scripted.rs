//! Built-in `scripted` (mechanics "Scripts that stay hand-written"; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/scripted.md). Tier 3.
//!
//! The base of an object that has a `script` and no other base. It has no native behavior, no
//! params and no state: `events::dispatch` runs the base handler (this one, which does nothing)
//! and then the kind's script through the script host (`script::on_event`). Which events the
//! kind handles comes from the script's functions and its `collides_with` list; the loader adds
//! them to the kind's event set (`iwbtg-format`, `merge_script_events`), because only the loader
//! has the compiled script.

use super::{BuiltinState, ParamError, ParamSpec};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

/// No params: a scripted object's defaults are its `params` table, which the script reads.
pub fn param_specs() -> &'static [ParamSpec] {
    &[]
}

/// Nothing of its own; the loader adds what the script handles.
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet::default()
}

/// No native state (a script's variables live in `Instance::vars`).
pub fn init_state(_params: &Params) -> Result<BuiltinState, ParamError> {
    Ok(BuiltinState::Stateless)
}

/// Does nothing: the script part of the event runs after this in `events::dispatch`.
pub fn on_event(_ctx: &mut Ctx, _me: InstId, _ev: &EventKind) -> Result<Flow, SimError> {
    Ok(Flow::Continue)
}
