//! Built-in `jump_refresher` (mechanics `jump_refresher`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/jump_refresher.md). Tier 3.
//!
//! Showdown moreJumping and moreJumping2: a kid touching it gets `djump` true
//! (`kid::api::refresh_air_jump`), and it hides at (0, 0) for `hide_ticks` (50, the room speed),
//! then returns to its start (Alarm 0).
//!
//! `tjump` (moreJumping `clear`, moreJumping2 `set`) also sets or clears the triple-jump flag
//! (`kid::api::set_triple_jump`); `none` leaves it alone.

use serde::{Deserialize, Serialize};

use super::warp::KIDS;
use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_error, param_real, state_mut, state_of,
};
use crate::alarm;
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::kid::api;
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

const TJUMP: &[&str] = &["none", "set", "clear"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "hide_ticks",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "tjump",
        ty: ParamType::Enum(TJUMP),
        required: false,
    },
];

/// A refresher settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RefresherState {
    /// Ticks it stays hidden.
    pub hide_ticks: Real,
    /// Where it returns to (set in Create).
    pub start: (Real, Real),
    /// What it does to the triple-jump flag: `Some(true)` sets it, `Some(false)` clears it.
    pub tjump: Option<bool>,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: the kid touching it, and Alarm 0.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        alarms: 0b1,
        ..EventSet::default()
    };
    set.collisions.push(KIDS);
    set
}

/// A new instance state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let tjump = match param_enum(params, "tjump", TJUMP)? {
        Some("set") => Some(true),
        Some("clear") => Some(false),
        _ => None,
    };
    let hide_ticks = param_real(params, "hide_ticks")?.unwrap_or(Real::new(50.0));
    if hide_ticks.get() < 1.0 {
        return Err(param_error("hide_ticks", "must be at least 1"));
    }
    Ok(BuiltinState::Refresher(RefresherState {
        hide_ticks,
        start: (Real::ZERO, Real::ZERO),
        tjump,
    }))
}

/// Runs one event on a refresher.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Refresher(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            if let Some(inst) = ctx.w.get(me) {
                let pos = (inst.m.x(), inst.m.y());
                if let Some(BuiltinState::Refresher(st)) = state_mut(ctx, me) {
                    st.start = pos;
                }
            }
        }
        EventKind::Collision { with, other } if with.intersects(KIDS) => {
            api::refresh_air_jump(ctx, *other);
            if let Some(on) = s.tjump {
                api::set_triple_jump(ctx, *other, on);
            }
            let rules = ctx.rules;
            if let Some(inst) = ctx.w.get_mut(me) {
                inst.m.set_x(Real::ZERO)?;
                inst.m.set_y(Real::ZERO)?;
                alarm::set(inst, 0, s.hide_ticks, &rules.num)?;
            }
        }
        EventKind::Alarm(0) => {
            if let Some(inst) = ctx.w.get_mut(me) {
                inst.m.set_x(s.start.0)?;
                inst.m.set_y(s.start.1)?;
            }
        }
        _ => {}
    }
    Ok(Flow::Continue)
}
