//! Built-in `gravity_flip` (mechanics `gravity_flip`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/gravity_flip.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! `direction = "up"` (KOT obj_ue, FT grav_up): a touch by the normal kid sets `global.grav`.
//! `direction = "down"` (obj_sita, grav_down): a touch by the flipped kid clears it. The kid
//! swaps object (or flips) on his next Step; that is the kid code. Both are destroyed outside
//! the room. The sound (FT restarts sndBlockChange) is the client.
//!
//! `transition`: `swap_object` and `grav_sign` are the same write here. `yflag_with_view_rotation`
//! (KAE objGravUp and objGravDown: rotate the view 12 degrees per tick, then reverse the kid
//! `yflag`) is refused at load until the view rotation and the kid yflag flip exist (OPEN, nugamus/iwbtg-collection-research#51).

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_error, state_of};
use crate::def::{ClassMask, KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

const DIRECTIONS: &[&str] = &["up", "down"];
const TRANSITIONS: &[&str] = &["swap_object", "yflag_with_view_rotation", "grav_sign"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "direction",
        ty: ParamType::Enum(DIRECTIONS),
        required: true,
    },
    ParamSpec {
        name: "transition",
        ty: ParamType::Enum(TRANSITIONS),
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

/// What a kind with this base handles: the kid touching it, and Outside Room.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        outside_room: true,
        ..EventSet::default()
    };
    set.collisions.push(ClassMask::KID);
    set.collisions.push(ClassMask::KID_FLIPPED);
    set
}

/// A new instance's state from its params: whether it flips up.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    if param_enum(params, "transition", TRANSITIONS)? == Some("yflag_with_view_rotation") {
        return Err(param_error(
            "transition",
            "yflag_with_view_rotation (KAE) is not supported yet (OPEN, nugamus/iwbtg-collection-research#51: view rotation, kid yflag flip)",
        ));
    }
    let up = param_enum(params, "direction", DIRECTIONS)?
        .ok_or_else(|| param_error("direction", "a gravity flip needs `direction` (up or down)"))?
        == "up";
    Ok(BuiltinState::GravityFlip { up })
}

/// Runs one event on a gravity flip.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::GravityFlip { up }) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let up = *up;
    match ev {
        EventKind::Collision { with, .. } => {
            if up && with.intersects(ClassMask::KID) {
                ctx.w.engine.kid.grav = true;
            } else if !up && with.intersects(ClassMask::KID_FLIPPED) {
                ctx.w.engine.kid.grav = false;
            }
        }
        EventKind::OutsideRoom => {
            ctx.destroy(me);
        }
        _ => {}
    }
    Ok(Flow::Continue)
}
