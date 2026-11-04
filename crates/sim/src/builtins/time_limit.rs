//! Built-in `time_limit` (mechanics `builtin.time_limit`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/time_limit.md). Tier 3.
//!
//! Two roles, one per object in the original (KOT, LT, MIKU, ANI, RBF):
//!
//! - `timer` (timelimitobject): holds hours, minutes and seconds. It starts idle. Alarm 0 takes
//!   one second off when a kid exists, then always re-arms itself for `room_speed` ticks. Step
//!   normalizes the counters: seconds below 0 borrow from minutes, then from hours; seconds
//!   below 0 with minutes and hours both 0 is the expiry, one alarm period after the display
//!   reads 0:00:00 (Confirmed, recorder, MIKU roommiku: 0:00 on frame 8803, timer gone on 8853).
//! - `start_zone` (timelimitcollition): a kid touching it sets every timer's alarm 0 to
//!   `room_speed` and destroys the zone.
//!
//! `on_expire`: `kill` kills the first kid and destroys the timer, `shake` spawns the `shake`
//! kind and destroys the timer, `nothing` destroys the timer, `goto_room` goes to `room` and
//! places or destroys the kid like a warp. The H:MM:SS display is the client's.
//! OPEN (#356): the original kills player2 instead of player when `global.grav` is set; here the
//! first kid is killed, whichever kind it is.

use serde::{Deserialize, Serialize};

use super::warp::{KIDS, destroy_kid_and_bow, whole};
use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_error, param_kind, param_real,
    state_mut, state_of,
};
use crate::alarm;
use crate::def::{KindDef, KindId, ParamValue, Params, RoomId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::kid::api::{KillCause, kill};
use crate::num::Real;
use crate::room::RoomRequest;
use crate::world::{Ctx, Flow, InstId, KindState, SpawnInit};

const ROLES: &[&str] = &["timer", "start_zone"];
const EXPIRES: &[&str] = &["kill", "shake", "nothing", "goto_room"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "role",
        ty: ParamType::Enum(ROLES),
        required: false,
    },
    ParamSpec {
        name: "hours",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "minutes",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "seconds",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "on_expire",
        ty: ParamType::Enum(EXPIRES),
        required: false,
    },
    ParamSpec {
        name: "room",
        ty: ParamType::Room,
        required: false,
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
        name: "shake",
        ty: ParamType::Kind,
        required: false,
    },
];

/// What expiry does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Expire {
    /// Kill the kid.
    Kill,
    /// Spawn the shake kind.
    Shake,
    /// Only destroy the timer.
    Nothing,
    /// Go to a room.
    GotoRoom,
}

/// A time limit object's settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TimeLimitState {
    /// The start zone, not the timer.
    pub start_zone: bool,
    /// Hours left.
    pub hours: i64,
    /// Minutes left.
    pub minutes: i64,
    /// Seconds left (below 0 only between the alarm and the Step that normalizes it).
    pub seconds: i64,
    /// What expiry does.
    pub on_expire: Expire,
    /// The room for `goto_room`.
    pub room: Option<RoomId>,
    /// Where the kid is placed in that room; (0, 0) destroys the kid instead.
    pub warp_x: Real,
    /// See `warp_x`.
    pub warp_y: Real,
    /// The kind `shake` spawns.
    pub shake: Option<KindId>,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: the timer its Step and alarm 0, the zone a kid touching
/// it. The role is a param, so a kind lists both.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        step: true,
        alarms: 1,
        ..EventSet::default()
    };
    set.collisions.push(KIDS);
    set
}

fn count(params: &Params, name: &str) -> Result<i64, ParamError> {
    let v = param_real(params, name)?.unwrap_or(Real::ZERO);
    Ok(i64::from(whole(v, name)?))
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let start_zone = param_enum(params, "role", ROLES)?.unwrap_or("timer") == "start_zone";
    let on_expire = match param_enum(params, "on_expire", EXPIRES)?.unwrap_or("kill") {
        "kill" => Expire::Kill,
        "shake" => Expire::Shake,
        "nothing" => Expire::Nothing,
        _ => Expire::GotoRoom,
    };
    let room = match params.0.get("room") {
        None => None,
        Some(ParamValue::Room(r)) => Some(*r),
        Some(_) => return Err(param_error("room", "must name a room")),
    };
    if !start_zone && on_expire == Expire::GotoRoom && room.is_none() {
        return Err(param_error("room", "goto_room needs the room it goes to"));
    }
    Ok(BuiltinState::TimeLimit(TimeLimitState {
        start_zone,
        hours: count(params, "hours")?,
        minutes: count(params, "minutes")?,
        seconds: count(params, "seconds")?,
        on_expire,
        room,
        warp_x: param_real(params, "warp_x")?.unwrap_or(Real::ZERO),
        warp_y: param_real(params, "warp_y")?.unwrap_or(Real::ZERO),
        shake: param_kind(params, "shake")?,
    }))
}

/// Arms alarm 0 of `id` for one second.
fn arm_second(ctx: &mut Ctx, id: InstId) -> Result<(), SimError> {
    let second = ctx.w.room_speed;
    let rules = ctx.rules;
    if let Some(inst) = ctx.w.get_mut(id) {
        alarm::set(inst, 0, second, &rules.num)?;
    }
    Ok(())
}

/// Runs one event on a time limit object.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::TimeLimit(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match (ev, s.start_zone) {
        (EventKind::Collision { with, .. }, true) if with.intersects(KIDS) => {
            let timers: Vec<InstId> = ctx
                .w
                .iter_alive()
                .filter(
                    |i| matches!(&i.state, KindState::Builtin(BuiltinState::TimeLimit(t)) if !t.start_zone),
                )
                .map(|i| i.id)
                .collect();
            for t in timers {
                arm_second(ctx, t)?;
            }
            Ok(ctx.destroy(me))
        }
        (EventKind::Alarm(0), false) => {
            if ctx.w.kid(&ctx.rules.tick).is_some()
                && let Some(BuiltinState::TimeLimit(st)) = state_mut(ctx, me)
            {
                st.seconds -= 1;
            }
            arm_second(ctx, me)?;
            Ok(Flow::Continue)
        }
        (EventKind::Step, false) if s.seconds < 0 => step_borrow(ctx, me, &s),
        _ => Ok(Flow::Continue),
    }
}

/// Step: normalizes negative seconds, or expires.
fn step_borrow(ctx: &mut Ctx, me: InstId, s: &TimeLimitState) -> Result<Flow, SimError> {
    if s.minutes == 0 && s.hours == 0 {
        return expire(ctx, me, s);
    }
    if let Some(BuiltinState::TimeLimit(st)) = state_mut(ctx, me) {
        st.seconds += 60;
        st.minutes -= 1;
        if st.minutes < 0 {
            st.minutes += 60;
            st.hours -= 1;
        }
    }
    Ok(Flow::Continue)
}

fn expire(ctx: &mut Ctx, me: InstId, s: &TimeLimitState) -> Result<Flow, SimError> {
    match s.on_expire {
        Expire::Kill => {
            if let Some(kid) = ctx.w.kid(&ctx.rules.tick) {
                kill(ctx, kid, KillCause::Script)?;
            }
        }
        Expire::Shake => {
            if let Some(kind) = s.shake {
                let (x, y) = ctx
                    .w
                    .get(me)
                    .map_or((Real::ZERO, Real::ZERO), |i| (i.m.x(), i.m.y()));
                ctx.spawn(kind, x, y, SpawnInit::default())?;
            }
        }
        Expire::Nothing => {}
        Expire::GotoRoom => {
            if let Some(room) = s.room {
                ctx.request_room(RoomRequest::Goto(room));
                // gm-compare: ok (warp placement: the (0, 0) sentinel of a param, not GML)
                if s.warp_x.get() == 0.0 && s.warp_y.get() == 0.0 {
                    destroy_kid_and_bow(ctx)?;
                } else if let Some(kid) = ctx.w.kid(&ctx.rules.tick).and_then(|k| ctx.w.get_mut(k)) {
                    kid.m.set_x(s.warp_x)?;
                    kid.m.set_y(s.warp_y)?;
                }
            }
        }
    }
    let flow = ctx.destroy(me);
    if s.on_expire == Expire::GotoRoom {
        Ok(Flow::AbortTick)
    } else {
        Ok(flow)
    }
}
