//! Built-in `conveyor` (mechanics `conveyor`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/conveyor.md). Tier 3.
//!
//! A solid block child (the loader gives it the `conveyor` class and the block solidity).
//! `mode`:
//! - `add_each_tick` (rainbow-feather object830 and object831, h -2 and 2): `h` is added to a
//!   touching kid `hspeed` on every collision tick.
//! - `set_on_top_edge` (showdown BlockHidari and BlockMigi, h -4 and 4): in Step, a kid touching
//!   the line from `(x + 32, y - 1)` to `(x, y)` gets `hspeed = h`. OPEN (nugamus/iwbtg-collection-research#53): showdown does the same
//!   to pushBlock (not a built-in), and the probe uses the bounding box (`prec` false).

use serde::{Deserialize, Serialize};

use super::warp::KIDS;
use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_real, state_of};
use crate::collide::Target;
use crate::collide::query::collision_line;
use crate::def::{KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

const MODES: &[&str] = &["add_each_tick", "set_on_top_edge"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "h",
        ty: ParamType::Real,
        required: true,
    },
    ParamSpec {
        name: "mode",
        ty: ParamType::Enum(MODES),
        required: false,
    },
];

/// A conveyor settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ConveyorState {
    /// The speed added or set.
    pub h: Real,
    /// Sets on the top edge in Step instead of adding on every collision.
    pub top_edge: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step in `set_on_top_edge`, else the kid touching it.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    if matches!(kind.params.0.get("mode"), Some(ParamValue::Str(m)) if m == "set_on_top_edge") {
        set.step = true;
    } else {
        set.collisions.push(KIDS);
    }
    set
}

/// A new instance state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let h = param_real(params, "h")?.ok_or_else(|| super::param_error("h", "is required"))?;
    Ok(BuiltinState::Conveyor(ConveyorState {
        h,
        top_edge: param_enum(params, "mode", MODES)? == Some("set_on_top_edge"),
    }))
}

/// Runs one event on a conveyor.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Conveyor(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match (ev, s.top_edge) {
        (EventKind::Collision { with, other }, false) if with.intersects(KIDS) => {
            if let Some(k) = ctx.w.get_mut(*other) {
                let h = k.m.hspeed() + s.h;
                k.m.set_hspeed(h)?;
            }
        }
        (EventKind::Step, true) => {
            let Some(inst) = ctx.w.get(me) else {
                return Ok(Flow::Continue);
            };
            let (x, y) = (inst.m.x(), inst.m.y());
            let hit = collision_line(
                ctx,
                x + Real::new(32.0),
                y - Real::ONE,
                x,
                y,
                Target::Class(KIDS),
                false,
                Some(me),
            )?;
            if let Some(kid) = hit
                && let Some(k) = ctx.w.get_mut(kid)
            {
                k.m.set_hspeed(s.h)?;
            }
        }
        _ => {}
    }
    Ok(Flow::Continue)
}
