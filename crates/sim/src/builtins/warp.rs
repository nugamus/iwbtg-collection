//! Built-in `warp` (mechanics `warp`, roomChanger; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/warp.md). Tier 1. WP0 creates the file; WP6a owns it.
//!
//! Collision with the kid, when `on_collide`: set the transition for the client, request the
//! room (deferred to tick row 15, Confirmed), then in the same handler either destroy the kid
//! and the bow when `warp_x` and `warp_y` are both 0 (the new room's `kid_start` makes a fresh
//! kid) or move the kid to `(warp_x, warp_y)` (he keeps his speed). Returns `AbortTick`.
//!
//! OPEN, not done: `gate` (boss and item gates: LR warpToBoss, KAE warpCount, XIA objWarpfin),
//! `screen_shift` and the negative-vspeed zeroing (KAE), NY's kept djump. They are per game and
//! need the boss and item flags.

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_real, state_of};
use crate::def::{ClassMask, EngineKind, KindDef, ParamValue, Params, RoomId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::output::OutputEvent;
use crate::room::{RoomOutput, RoomRequest};
use crate::world::{Ctx, Flow, InstId};

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "room_to",
        ty: ParamType::Room,
        required: true,
    },
    ParamSpec {
        name: "warp_x",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "warp_y",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "transition",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "transition_steps",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "on_collide",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "visible",
        ty: ParamType::Bool,
        required: false,
    },
];

/// A warp's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WarpState {
    /// Where it goes.
    pub room_to: RoomId,
    /// Kid x there; 0 with `warp_y` 0 means destroy the kid.
    pub warp_x: Real,
    /// Kid y there.
    pub warp_y: Real,
    /// `transition_kind` (0 none, 21 fade out and in).
    pub transition: u8,
    /// `transition_steps`.
    pub transition_steps: u32,
    /// `onCollide`: the touch does something (warp sets it true in Create).
    pub on_collide: bool,
    /// `visible` override.
    pub visible: Option<bool>,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// The kid classes a touch handler listens for.
pub(crate) const KIDS: ClassMask = ClassMask(ClassMask::KID.0 | ClassMask::KID_FLIPPED.0);

/// What a kind with this base handles: the kid touching it.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    set.collisions.push(KIDS);
    set
}

/// A non-negative whole number param as `u32`.
pub(crate) fn whole(v: Real, name: &str) -> Result<u32, ParamError> {
    let x = v.get();
    if x < 0.0 || x.fract() != 0.0 {
        return Err(super::param_error(name, "must be a whole number, 0 or more"));
    }
    u32::try_from(crate::num::to_int_trunc(v).map_err(|_| super::param_error(name, "out of range"))?)
        .map_err(|_| super::param_error(name, "out of range"))
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let room_to = match params.0.get("room_to") {
        Some(ParamValue::Room(r)) => *r,
        _ => return Err(super::param_error("room_to", "a warp needs the room it goes to")),
    };
    let zero = Real::ZERO;
    Ok(BuiltinState::Warp(WarpState {
        room_to,
        warp_x: param_real(params, "warp_x")?.unwrap_or(zero),
        warp_y: param_real(params, "warp_y")?.unwrap_or(zero),
        transition: u8::try_from(whole(
            param_real(params, "transition")?.unwrap_or(zero),
            "transition",
        )?)
        .map_err(|_| super::param_error("transition", "must be 0 to 255"))?,
        transition_steps: whole(
            param_real(params, "transition_steps")?.unwrap_or(zero),
            "transition_steps",
        )?,
        on_collide: param_bool(params, "on_collide")?.unwrap_or(true),
        visible: param_bool(params, "visible")?,
    }))
}

/// Destroys every kid (both kinds) and the bow (what the roomChanger does for a warp to (0, 0)).
pub(crate) fn destroy_kid_and_bow(ctx: &mut Ctx) -> Result<(), SimError> {
    let bow = ctx.def.engine_kind(EngineKind::Bow);
    let ids: Vec<InstId> = ctx
        .w
        .iter_alive()
        .filter(|i| i.classes.intersects(KIDS) || Some(i.kind) == bow)
        .map(|i| i.id)
        .collect();
    for id in ids {
        if ctx.destroy(id).is_halt() {
            return Err(ctx.halted_error());
        }
    }
    Ok(())
}

/// Sets the transition for the next room change (client-visual only). Kind 0 is none.
pub(crate) fn emit_transition(ctx: &mut Ctx, kind: u8, steps: u32) {
    if kind > 0 {
        ctx.emit(OutputEvent::Room(RoomOutput::Transition { kind, steps }));
    }
}

/// Runs one event on a warp.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Warp(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            if let (Some(v), Some(inst)) = (s.visible, ctx.w.get_mut(me)) {
                inst.visible = v;
            }
            Ok(Flow::Continue)
        }
        EventKind::Collision { with, other } if with.intersects(KIDS) && s.on_collide => {
            emit_transition(ctx, s.transition, s.transition_steps);
            ctx.request_room(RoomRequest::Goto(s.room_to));
            // gm-compare: ok (the (0, 0) sentinel of the params, not a GML test)
            if s.warp_x.get() == 0.0 && s.warp_y.get() == 0.0 {
                destroy_kid_and_bow(ctx)?;
            } else if let Some(kid) = ctx.w.get_mut(*other) {
                kid.m.set_x(s.warp_x)?;
                kid.m.set_y(s.warp_y)?;
            }
            Ok(Flow::AbortTick)
        }
        _ => Ok(Flow::Continue),
    }
}
