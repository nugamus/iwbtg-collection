//! Built-in `breakable_block` (mechanics `breakable_block`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/breakable_block.md). Tier 3.
//!
//! A solid block child that breaks.
//! - `break_on = "shot"` (rainbow-feather break): a kid bullet touching it destroys it (the
//!   sound is the client).
//! - `break_on = "touch_delay"` (showdown object831, object832, object833): the first touch by a
//!   kid arms Alarm 0 = `stage_ticks` (50). Each time it runs, the block shows the next frame;
//!   after `stages` runs it is destroyed (object833: 1 stage, object832: 2, object831: 3).
//!   Later touches change nothing.

use serde::{Deserialize, Serialize};

use super::warp::KIDS;
use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_error, param_real, state_mut, state_of,
};
use crate::alarm;
use crate::def::{ClassMask, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

const BREAK_ON: &[&str] = &["shot", "touch_delay"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "break_on",
        ty: ParamType::Enum(BREAK_ON),
        required: false,
    },
    ParamSpec {
        name: "stages",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "stage_ticks",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "sound",
        ty: ParamType::Sound,
        required: false,
    },
];

/// A breakable block settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BreakableState {
    /// Breaks when shot (else after a touch delay).
    pub on_shot: bool,
    /// Alarm runs before it is destroyed.
    pub stages: u32,
    /// Ticks per stage.
    pub stage_ticks: Real,
    /// The first touch happened.
    pub started: bool,
    /// Stages finished so far.
    pub stage: u32,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    if matches!(kind.params.0.get("break_on"), Some(ParamValue::Str(w)) if w == "touch_delay") {
        set.alarms = 0b1;
        set.collisions.push(KIDS);
    } else {
        set.collisions.push(ClassMask::KID_BULLET);
    }
    set
}

/// A new instance state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let count = |name: &str, default: f64| -> Result<u32, ParamError> {
        let v = param_real(params, name)?.map_or(default, Real::get);
        if v.fract() != 0.0 || !(1.0..=1_000_000.0).contains(&v) {
            return Err(param_error(name, "must be a whole number of at least 1"));
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked whole, in range"
        )]
        Ok(v as u32)
    };
    Ok(BuiltinState::Breakable(BreakableState {
        on_shot: param_enum(params, "break_on", BREAK_ON)? != Some("touch_delay"),
        stages: count("stages", 1.0)?,
        stage_ticks: Real::new(f64::from(count("stage_ticks", 50.0)?)),
        started: false,
        stage: 0,
    }))
}

/// Runs one event on a breakable block.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Breakable(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    let rules = ctx.rules;
    match ev {
        EventKind::Collision { with, .. } if s.on_shot && with.intersects(ClassMask::KID_BULLET) => {
            Ok(ctx.destroy(me))
        }
        EventKind::Collision { with, .. } if !s.on_shot && with.intersects(KIDS) && !s.started => {
            if let Some(BuiltinState::Breakable(st)) = state_mut(ctx, me) {
                st.started = true;
            }
            if let Some(inst) = ctx.w.get_mut(me) {
                alarm::set(inst, 0, s.stage_ticks, &rules.num)?;
            }
            Ok(Flow::Continue)
        }
        EventKind::Alarm(0) if !s.on_shot => {
            let stage = s.stage + 1;
            if stage >= s.stages {
                return Ok(ctx.destroy(me));
            }
            if let Some(BuiltinState::Breakable(st)) = state_mut(ctx, me) {
                st.stage = stage;
            }
            if let Some(inst) = ctx.w.get_mut(me) {
                inst.img.set_index(Real::new(f64::from(stage)))?;
                alarm::set(inst, 0, s.stage_ticks, &rules.num)?;
            }
            Ok(Flow::Continue)
        }
        _ => Ok(Flow::Continue),
    }
}
