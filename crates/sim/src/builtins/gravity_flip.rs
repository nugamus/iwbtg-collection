//! Built-in `gravity_flip` (mechanics `gravity_flip`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/gravity_flip.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! `direction = "up"` (KOT obj_ue, FT grav_up): a touch by the normal kid sets `global.grav`.
//! `direction = "down"` (obj_sita, grav_down): a touch by the flipped kid clears it. The kid
//! swaps object (or flips) on his next Step; that is the kid code. Both are destroyed outside
//! the room. The sound (FT restarts sndBlockChange) is the client.
//!
//! `transition`: `swap_object` and `grav_sign` are the same write here. `yflag_with_view_rotation`
//! (KAE objGravUp and objGravDown, kid/B.md "Gravity flip (kaenbyou)"): while the kid touches it
//! (`up`: only while not reversed; `down`: only while reversed) `frozen2` is set every tick; the
//! first touch of an activation starts Alarm 0. Alarm 0 adds 12 degrees to view 0's angle and
//! re-arms itself while the angle is below 180 (`down`: below 360 and not 0); the first Alarm 0 past
//! the end (`down`: sets the angle to 0 and) arms Alarm 1, which runs in the same alarm pass and runs `playerReverse` on every kid,
//! clears `frozen2` and the activation. The arrow's sound is the client's.

use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, ParamValue, param_enum, param_error, state_mut, state_of,
};
use crate::alarm;
use crate::def::{ClassMask, KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::{self, Real};
use crate::world::KindState;
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
pub fn event_set(kind: &KindDef) -> EventSet {
    let rotate = matches!(
        kind.params.0.get("transition"),
        Some(ParamValue::Str(t)) if t == "yflag_with_view_rotation"
    );
    let mut set = EventSet {
        outside_room: true,
        alarms: if rotate { 0b11 } else { 0 },
        ..EventSet::default()
    };
    set.collisions.push(ClassMask::KID);
    set.collisions.push(ClassMask::KID_FLIPPED);
    set
}

/// A new instance's state from its params: whether it flips up, and the KAE rotation.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let rotate = param_enum(params, "transition", TRANSITIONS)? == Some("yflag_with_view_rotation");
    let up = param_enum(params, "direction", DIRECTIONS)?
        .ok_or_else(|| param_error("direction", "a gravity flip needs `direction` (up or down)"))?
        == "up";
    Ok(BuiltinState::GravityFlip {
        up,
        rotate,
        played: false,
    })
}

/// Runs one event on a gravity flip.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::GravityFlip { up, rotate, .. }) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let (up, rotate) = (*up, *rotate);
    if rotate {
        return rotation_event(ctx, me, up, ev);
    }
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

/// The KAE arrow (kid/B.md "Gravity flip (kaenbyou)").
fn rotation_event(ctx: &mut Ctx, me: InstId, up: bool, ev: &EventKind) -> Result<Flow, SimError> {
    let reversed = ctx.w.engine.kid.b.reverse;
    match ev {
        EventKind::Collision { .. } if up != reversed => {
            ctx.w.engine.kid.frozen2 = true;
            let first = match state_mut(ctx, me) {
                Some(BuiltinState::GravityFlip { played, .. }) if !*played => {
                    *played = true;
                    true
                }
                _ => false,
            };
            if first && let Some(i) = ctx.w.get_mut(me) {
                alarm::set(i, 0, Real::ONE, &ctx.rules.num)?;
            }
        }
        EventKind::Alarm(0) => {
            let nr = &ctx.rules.num;
            let turning = |a: Real| {
                if up {
                    num::gm_lt(a, Real::new(180.0), nr)
                } else {
                    num::gm_lt(a, Real::new(360.0), nr) && num::gm_ne(a, Real::ZERO, nr)
                }
            };
            let mut angle = ctx.w.views[0].angle;
            // The turn after the last one arms Alarm 1, which runs in the same alarm pass
            // (famb-gravflip: angle 180 on f54, the flip on f55).
            let next = if turning(angle) {
                angle += Real::new(12.0);
                0
            } else {
                if !up {
                    angle = Real::ZERO;
                }
                1
            };
            ctx.w.views[0].angle = angle;
            if let Some(i) = ctx.w.get_mut(me) {
                alarm::set(i, next, Real::ONE, &ctx.rules.num)?;
            }
        }
        EventKind::Alarm(1) => {
            let kids: Vec<InstId> = ctx
                .w
                .instances()
                .iter()
                .filter(|i| matches!(i.state, KindState::Kid(_)))
                .map(|i| i.id)
                .collect();
            for k in kids {
                crate::kid::api::player_reverse(ctx, k)?;
            }
            ctx.w.engine.kid.frozen2 = false;
            if let Some(BuiltinState::GravityFlip { played, .. }) = state_mut(ctx, me) {
                *played = false;
            }
        }
        EventKind::OutsideRoom => {
            ctx.destroy(me);
        }
        _ => {}
    }
    Ok(Flow::Continue)
}
