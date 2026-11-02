//! Built-in `secret_item` (mechanics `secret_item`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/secret_item.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! A kid touch sets `global.item[item_index]` and destroys it (KAE secretItem, WAN, LR, RBF
//! secretitem1). Create removes it when the item is already taken. The sound (KAE plays one, RBF
//! does not) is the client's.

use super::warp::KIDS;
use super::{BuiltinState, ParamError, ParamSpec, ParamType, flag_index, state_of};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "item_index",
        ty: ParamType::Real,
        required: true,
    },
    ParamSpec {
        name: "sprite",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "sound",
        ty: ParamType::Sound,
        required: false,
    },
];

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: the kid touching it.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    set.collisions.push(KIDS);
    set
}

/// A new instance's state from its params: the item index.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    Ok(BuiltinState::Flag(flag_index(params, "item_index")?))
}

/// Runs one event on a secret item.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Flag(i)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let i = *i;
    match ev {
        EventKind::Create => {
            if ctx.w.engine.mech.item(i) {
                ctx.destroy(me);
            }
        }
        EventKind::Collision { with, .. } if with.intersects(KIDS) => {
            ctx.w.engine.mech.set_item(i);
            ctx.destroy(me);
        }
        _ => {}
    }
    Ok(Flow::Continue)
}
