//! The kid at the room edge (`events.boundary`, docs/plans/sim-core.md 5.3 edge row). WP5a owns
//! it.
//!
//! Spec: kid/A1.md "Intersect Boundary". With `events.boundary = "intersect_origin"` the kid's
//! Intersect Boundary event (the pipeline fires it when the collision box crosses the room edge,
//! R5) does:
//!
//! 1. Test the origin: `x < 0`, `x > room_width`, `y < 0` or `y > room_height`, all strict, so
//!    `x = room_width` is inside.
//! 2. When outside: with no `edge_warp` (the original's outsideroomChanger) overlapping the kid,
//!    kill it (`KillCause::Boundary`; `global.looproom` is never set). With one: request its
//!    `room_to`; when its `warp_x` and `warp_y` are both 0 destroy the kid and the bow (the new
//!    room's `kid_start` makes a fresh kid), else move the kid to `(warp_x, warp_y)`.
//! 3. In every case, if the kid is still alive: `x < 0` gives `x = room_width - 16` and undoes
//!    this step's `vspeed` on `y`; else `x > room_width` gives `x = 16` and the same; else
//!    `y < 0` gives `y = room_height - 16` and `x = xprevious`; else `y > room_height` gives
//!    `y = 16` and `x = xprevious`. These read the position after step 2.
//!
//! The warp target is read from the overlapping instance's [`WarpState`], which `edge_warp`
//! shares with `warp` (OPEN until WP6b builds `edge_warp`: an `edge_warp` with a per-edge table
//! has no `WarpState` and is `Unsupported`). `OutsideRoomKill` (family B) and `EndStepOrigin`
//! (WP5c) are `Unsupported` here.

use super::api::{KillCause, kill};
use crate::builtins::state::BuiltinState;
use crate::builtins::warp::{WarpState, destroy_kid_and_bow};
use crate::collide::query::{Target, instance_place};
use crate::def::ClassMask;
use crate::def::physics::BoundaryModel;
use crate::error::SimError;
use crate::events::EventKind;
use crate::num::{Real, gm_eq, gm_gt, gm_lt};
use crate::room::RoomRequest;
use crate::world::{Ctx, Flow, InstId};

/// Runs a boundary event on the kid `me`; `None` when the profile's model does not handle `ev`.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Option<Flow>, SimError> {
    let model = ctx.def.physics.events.boundary;
    match (model, ev) {
        (BoundaryModel::IntersectOrigin, EventKind::IntersectBoundary) => intersect_origin(ctx, me).map(Some),
        (BoundaryModel::OutsideRoomKill, EventKind::OutsideRoom) => Err(SimError::unsupported(
            "kid boundary: events.boundary outside_room_kill (5.3 edge row, WP5b)",
        )),
        (BoundaryModel::EndStepOrigin, EventKind::EndStep) => Err(SimError::unsupported(
            "kid boundary: events.boundary end_step_origin (5.3 edge row, WP5c)",
        )),
        _ => Ok(None),
    }
}

fn intersect_origin(ctx: &mut Ctx, me: InstId) -> Result<Flow, SimError> {
    let n = &ctx.rules.num;
    let Some(room) = ctx.def.room(ctx.w.room) else {
        return Err(SimError::invariant(
            "kid boundary: the current room does not exist",
        ));
    };
    let (rw, rh) = (
        Real::new(f64::from(room.width)),
        Real::new(f64::from(room.height)),
    );
    let pos = |ctx: &Ctx| {
        ctx.w
            .get(me)
            .map(|i| (i.m.x(), i.m.y(), i.m.xprevious(), i.m.vspeed()))
    };
    let missing = || SimError::invariant("kid boundary: kid vanished");
    let (x, y, _, _) = pos(ctx).ok_or_else(missing)?;
    let outside = gm_lt(x, Real::ZERO, n) || gm_gt(x, rw, n) || gm_lt(y, Real::ZERO, n) || gm_gt(y, rh, n);
    let mut warped = false;
    if outside {
        match instance_place(ctx, me, x, y, Target::Class(ClassMask::EDGE_WARP))? {
            None => {
                let flow = kill(ctx, me, KillCause::Boundary)?;
                if flow != Flow::Continue {
                    return Ok(flow);
                }
            }
            Some(changer) => {
                let Some(BuiltinState::Warp(WarpState {
                    room_to,
                    warp_x,
                    warp_y,
                    ..
                })) = ctx.w.get(changer).and_then(|i| match &i.state {
                    crate::world::KindState::Builtin(b) => Some(b.clone()),
                    _ => None,
                })
                else {
                    return Err(SimError::unsupported(
                        "kid boundary: an edge_warp without a room_to and warp point (per-edge tables, WP6b)",
                    ));
                };
                ctx.request_room(RoomRequest::Goto(room_to));
                warped = true;
                if gm_eq(warp_x, Real::ZERO, n) && gm_eq(warp_y, Real::ZERO, n) {
                    destroy_kid_and_bow(ctx)?;
                } else if let Some(i) = ctx.w.get_mut(me) {
                    i.m.set_x(warp_x)?;
                    i.m.set_y(warp_y)?;
                }
            }
        }
    }
    // The wrap, on whatever is left of the kid.
    if !ctx.w.get(me).is_some_and(|i| i.alive) {
        return Ok(Flow::Continue);
    }
    let (x, y, xprev, vs) = pos(ctx).ok_or_else(missing)?;
    let sixteen = Real::new(16.0);
    let i = ctx.w.get_mut(me).ok_or_else(missing)?;
    if gm_lt(x, Real::ZERO, n) {
        i.m.set_x(rw - sixteen)?;
        i.m.set_y(y - vs)?;
    } else if gm_gt(x, rw, n) {
        i.m.set_x(sixteen)?;
        i.m.set_y(y - vs)?;
    } else if gm_lt(y, Real::ZERO, n) {
        i.m.set_y(rh - sixteen)?;
        i.m.set_x(xprev)?;
    } else if gm_gt(y, rh, n) {
        i.m.set_y(sixteen)?;
        i.m.set_x(xprev)?;
    }
    Ok(if warped { Flow::AbortTick } else { Flow::Continue })
}
