//! Built-in `trap_stopper` (mechanics `trap_stopper`; docs/plans/sim-core.md 6.1, K8;
//! docs/format/builtins/trap_stopper.md). Tier 3.
//!
//! `mode = "stop"` (rainbow-feather stopneedleblock): an invisible block with no events. A
//! `trap` with `stop_on_stopper` reacts to touching it (the loader gives this kind the
//! `trap_stopper` class). `mode = "destroy"` (help-the-fairy-tale object817): each Step it
//! destroys every `trap` instance it overlaps. OPEN: the original uses a collision event with the
//! listed trap objects, which runs in the collision phase; this runs in Step.

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_real, state_of};
use crate::collide::Target;
use crate::collide::query::instance_place;
use crate::def::{Builtin, KindDef, KindId, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

const MODES: &[&str] = &["stop", "destroy"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "mode",
        ty: ParamType::Enum(MODES),
        required: false,
    },
    ParamSpec {
        name: "size",
        ty: ParamType::Real,
        required: false,
    },
];

/// A stopper settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct StopperState {
    /// Destroys the traps it overlaps.
    pub destroy: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step in `destroy` mode, else nothing.
pub fn event_set(kind: &KindDef) -> EventSet {
    EventSet {
        step: matches!(kind.params.0.get("mode"), Some(ParamValue::Str(w)) if w == "destroy"),
        ..EventSet::default()
    }
}

/// A new instance state from its params. `size` is the loader (the mask size).
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    param_real(params, "size")?;
    Ok(BuiltinState::Stopper(StopperState {
        destroy: param_enum(params, "mode", MODES)? == Some("destroy"),
    }))
}

/// Runs one event on a stopper.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Stopper(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    if !s.destroy || !matches!(ev, EventKind::Step) {
        return Ok(Flow::Continue);
    }
    let Some(inst) = ctx.w.get(me) else {
        return Ok(Flow::Continue);
    };
    let (x, y) = (inst.m.x(), inst.m.y());
    let traps: Vec<KindId> = ctx
        .def
        .kinds
        .iter()
        .enumerate()
        .filter(|(_, k)| k.base == Builtin::Trap)
        .filter_map(|(i, _)| u32::try_from(i).ok().map(KindId))
        .collect();
    for kind in traps {
        while let Some(hit) = instance_place(ctx, me, x, y, Target::Kind(kind))? {
            if ctx.destroy(hit).is_halt() {
                return Ok(Flow::Halt);
            }
            // `destroy` marks it dead: the query skips dead instances.
            if ctx.w.get(hit).is_some_and(|i| i.alive) {
                break;
            }
        }
    }
    Ok(Flow::Continue)
}
