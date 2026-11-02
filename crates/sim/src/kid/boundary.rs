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
//! shares with `warp`; an `edge_warp` with a per-edge table (NY roomout, `EdgeTable`) is read
//! instead, whether or not the kid touches it: the first edge crossed (left, right, up, down)
//! names the room, with no target the kid dies (at the top nothing happens). `OutsideRoomKill` (family B) kills on Outside Room
//! unless a room changer overlaps; `EndStepOrigin` (WP5c) is `Unsupported` here.

use super::api::{KillCause, kill};
use crate::builtins::edge_warp::Edge;
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
        (BoundaryModel::IntersectOrigin, EventKind::IntersectBoundary) => {
            intersect_origin(ctx, me, false).map(Some)
        }
        (BoundaryModel::IntersectOriginFixed, EventKind::IntersectBoundary) => {
            intersect_origin(ctx, me, true).map(Some)
        }
        (BoundaryModel::OutsideRoomKill, EventKind::OutsideRoom) => outside_room_kill(ctx, me).map(Some),
        (BoundaryModel::EndStepOrigin, EventKind::EndStep) => Err(SimError::unsupported(
            "kid boundary: events.boundary end_step_origin (5.3 edge row, WP5c)",
        )),
        _ => Ok(None),
    }
}

/// `outside_room_kill` (family B, kid/B.md "Outside Room"): the box is completely outside the
/// room (the pipeline fires the event); the kid dies unless it touches a room changer (`warp`,
/// `edge_warp`).
fn outside_room_kill(ctx: &mut Ctx, me: InstId) -> Result<Flow, SimError> {
    let Some(i) = ctx.w.get(me) else {
        return Ok(Flow::Continue);
    };
    let (x, y) = (i.m.x(), i.m.y());
    let changers = ClassMask::ROOM_CHANGER | ClassMask::EDGE_WARP;
    if instance_place(ctx, me, x, y, Target::Class(changers))?.is_some() {
        return Ok(Flow::Continue);
    }
    kill(ctx, me, KillCause::Boundary)
}

fn intersect_origin(ctx: &mut Ctx, me: InstId, fixed: bool) -> Result<Flow, SimError> {
    let n = &ctx.rules.num;
    let Some(room) = ctx.def.room(ctx.w.room) else {
        return Err(SimError::invariant(
            "kid boundary: the current room does not exist",
        ));
    };
    // `fixed` (A4 machinary): the edges are 800 by 608 whatever the room.
    let (rw, rh) = if fixed {
        (Real::new(800.0), Real::new(608.0))
    } else {
        (
            Real::new(f64::from(room.width)),
            Real::new(f64::from(room.height)),
        )
    };
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
        let touching = instance_place(ctx, me, x, y, Target::Class(ClassMask::EDGE_WARP))?;
        let builtin = |ctx: &Ctx, id: InstId| match ctx.w.get(id).map(|i| &i.state) {
            Some(crate::world::KindState::Builtin(b)) => Some(b.clone()),
            _ => None,
        };
        // A per-edge table (NY roomout) is room-wide: the kid need not touch it.
        let table = touching
            .and_then(|c| match builtin(ctx, c) {
                Some(BuiltinState::EdgeTable(t)) => Some(t),
                _ => None,
            })
            .or_else(|| {
                ctx.w
                    .instances()
                    .iter()
                    .filter(|i| i.alive && i.classes.intersects(ClassMask::EDGE_WARP))
                    .find_map(|i| match &i.state {
                        crate::world::KindState::Builtin(BuiltinState::EdgeTable(t)) => Some(t.clone()),
                        _ => None,
                    })
            });
        if let Some(table) = table {
            // kid/A4.md roomout: left, right, up, then down; the first edge crossed decides. An
            // unset target kills, except at the top (nothing). The kid keeps position and speeds.
            let edge = if gm_lt(x, Real::ZERO, n) {
                Edge::Left
            } else if gm_gt(x, rw, n) {
                Edge::Right
            } else if gm_lt(y, Real::ZERO, n) {
                Edge::Up
            } else {
                Edge::Down
            };
            match table.target(edge) {
                Some(room) => {
                    ctx.request_room(RoomRequest::Goto(room));
                    warped = true;
                }
                None if edge == Edge::Up => {}
                None => {
                    let flow = kill(ctx, me, KillCause::Boundary)?;
                    if flow != Flow::Continue {
                        return Ok(flow);
                    }
                }
            }
        } else if let Some(changer) = touching {
            let Some(BuiltinState::Warp(WarpState {
                room_to,
                warp_x,
                warp_y,
                ..
            })) = builtin(ctx, changer)
            else {
                return Err(SimError::unsupported(
                    "kid boundary: an edge_warp with neither a room_to nor a per-edge table",
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
        } else {
            let flow = kill(ctx, me, KillCause::Boundary)?;
            if flow != Flow::Continue {
                return Ok(flow);
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
        i.m.set_x(if fixed { x + rw } else { rw - sixteen })?;
        i.m.set_y(y - vs)?;
    } else if gm_gt(x, rw, n) {
        i.m.set_x(if fixed { x - rw } else { sixteen })?;
        i.m.set_y(y - vs)?;
    } else if gm_lt(y, Real::ZERO, n) {
        i.m.set_y(if fixed { y + rh } else { rh - sixteen })?;
        i.m.set_x(xprev)?;
    } else if gm_gt(y, rh, n) {
        i.m.set_y(if fixed { y - Real::new(600.0) } else { sixteen })?;
        i.m.set_x(xprev)?;
    }
    Ok(if warped { Flow::AbortTick } else { Flow::Continue })
}
