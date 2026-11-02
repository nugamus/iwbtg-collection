//! Built-in `path_platform` (mechanics `path_platform`; kid/B.md pathPlatform; docs/plans/
//! sim-core.md 6.1, M8; docs/format/builtins/path_platform.md). Tier 3.
//!
//! Kaenbyou pathPlatform. A platform that follows a path (`path::start`; the runner advances it
//! every tick). Touch mode (`start_on_touch`) starts the path on the first kid contact at speed
//! `speed + 0.0001`; `scale` is applied as `path_scale` when not 0. In End Step, when two
//! horizontal lines from `x` to `x + 31` at the kid `yflag` heights (`y + 8 - 25 yflag` and
//! `y + 8 - 10 yflag`) both touch a kid and the path position changed this tick, the first kid
//! is moved by the platform displacement this tick with no solid check.
//! `keep_moving_when_kid_absent` (kaenbyou `move`): with no kid in the room the platform leaves
//! the path and keeps moving at the path speed.
//!
//! OPEN (nugamus/iwbtg-collection-research#46): the original y carry also subtracts the unscaled `lengthdir_y(path_speed, direction)`
//! (kid/B.md); this uses the platform displacement alone. The line probe is the bounding box
//! (`prec` false; not specified). No kaenbyou room places a moving one (room23 platforms never
//! move as placed, traces/kaenbyou/famb-platform-b.csv).

use serde::{Deserialize, Serialize};

use super::warp::KIDS;
use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_error, param_real,
    state_mut, state_of,
};
use crate::collide::Target;
use crate::collide::query::collision_line;
use crate::def::{KindDef, ParamValue, Params, PathId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::kid::api::{self, KidField};
use crate::num::Real;
use crate::path::{self, PathEnd};
use crate::world::{Ctx, Flow, InstId, Value};

const PATH_END: &[&str] = &["stop", "restart", "continue", "reverse"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "path",
        ty: ParamType::Real,
        required: true,
    },
    ParamSpec {
        name: "speed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "scale",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "path_end",
        ty: ParamType::Enum(PATH_END),
        required: false,
    },
    ParamSpec {
        name: "start_on_touch",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "keep_moving_when_kid_absent",
        ty: ParamType::Bool,
        required: false,
    },
];

/// A path platform settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PathPlatformState {
    /// The path.
    pub path: PathId,
    /// Path speed (touch mode adds 0.0001).
    pub speed: Real,
    /// `path_scale` when not 0.
    pub scale: Real,
    /// End action.
    pub end: PathEnd,
    /// Starts on the first touch (else in Create).
    pub on_touch: bool,
    /// Leaves the path when no kid exists.
    pub keep_moving: bool,
    /// The path was started.
    pub started: bool,
    /// The path position at the last End Step.
    pub last_position: Real,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: End Step, and the kid touching it in touch mode.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        end_step: true,
        ..EventSet::default()
    };
    if matches!(kind.params.0.get("start_on_touch"), Some(ParamValue::Bool(true))) {
        set.collisions.push(KIDS);
    }
    set
}

/// A new instance state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let id = param_real(params, "path")?.ok_or_else(|| param_error("path", "is required"))?;
    // gm-compare: ok (load-time param validation: integer index check)
    let path = (id.get().fract() == 0.0)
        .then(|| crate::num::round::to_int_trunc(id).ok())
        .flatten()
        .and_then(|i| u32::try_from(i).ok())
        .ok_or_else(|| param_error("path", "must be a path index"))?;
    Ok(BuiltinState::PathPlatform(PathPlatformState {
        path: PathId(path),
        speed: param_real(params, "speed")?.unwrap_or(Real::ZERO),
        scale: param_real(params, "scale")?.unwrap_or(Real::ZERO),
        end: match param_enum(params, "path_end", PATH_END)? {
            Some("restart") => PathEnd::Restart,
            Some("continue") => PathEnd::Continue,
            Some("reverse") => PathEnd::Reverse,
            _ => PathEnd::Stop,
        },
        on_touch: param_bool(params, "start_on_touch")?.unwrap_or(false),
        keep_moving: param_bool(params, "keep_moving_when_kid_absent")?.unwrap_or(false),
        started: false,
        last_position: Real::ZERO,
    }))
}

fn start_path(ctx: &mut Ctx, me: InstId, s: &PathPlatformState, speed: Real) -> Result<(), SimError> {
    if s.path.index().and_then(|i| ctx.def.paths.get(i)).is_none() {
        return Err(SimError::invariant("path_platform path does not exist"));
    }
    if let Some(inst) = ctx.w.get_mut(me) {
        let mut run = path::start(&inst.m, s.path, speed, s.end, false)?;
        // gm-compare: ok (0 means the param was not set)
        if s.scale.get() != 0.0 {
            run.scale = s.scale;
        }
        inst.path = Some(run);
    }
    if let Some(BuiltinState::PathPlatform(st)) = state_mut(ctx, me) {
        st.started = true;
    }
    Ok(())
}

/// Runs one event on a path platform.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::PathPlatform(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create if !s.on_touch => start_path(ctx, me, &s, s.speed)?,
        EventKind::Collision { with, .. } if s.on_touch && !s.started && with.intersects(KIDS) => {
            start_path(ctx, me, &s, s.speed + Real::new(0.0001))?;
        }
        EventKind::EndStep => return end_step(ctx, me, &s),
        _ => {}
    }
    Ok(Flow::Continue)
}

fn end_step(ctx: &mut Ctx, me: InstId, s: &PathPlatformState) -> Result<Flow, SimError> {
    let first = ctx
        .w
        .instances()
        .iter()
        .find(|i| i.alive && i.classes.intersects(KIDS))
        .map(|i| i.id);
    let Some(inst) = ctx.w.get(me) else {
        return Ok(Flow::Continue);
    };
    let position = inst.path.as_ref().map(|p| p.position);
    let (x, y) = (inst.m.x(), inst.m.y());
    let (dx, dy) = (x - inst.m.xprevious(), y - inst.m.yprevious());
    let Some(kid) = first else {
        // No kid: a platform that keeps moving leaves the path at the path speed.
        if s.keep_moving
            && let Some(inst) = ctx.w.get_mut(me)
            && let Some(run) = inst.path.take()
        {
            let speed = run.speed;
            inst.m.set_speed(speed)?;
        }
        return Ok(Flow::Continue);
    };
    let changed = position.is_some_and(|p| p != s.last_position);
    if let Some(p) = position
        && let Some(BuiltinState::PathPlatform(st)) = state_mut(ctx, me)
    {
        st.last_position = p;
    }
    if !changed {
        return Ok(Flow::Continue);
    }
    let f = KidField {
        name: "yflag".to_owned(),
        index: None,
    };
    let yflag = match api::read_field(ctx.w, kid, &f) {
        Some(Value::Real(v)) => v,
        _ => Real::ONE,
    };
    let (x1, x2) = (x, x + Real::new(31.0));
    let target = Target::Class(KIDS);
    for k in [25.0, 10.0] {
        let ly = y + Real::new(8.0) - Real::new(k) * yflag;
        if collision_line(ctx, x1, ly, x2, ly, target, false, Some(me))?.is_none() {
            return Ok(Flow::Continue);
        }
    }
    if let Some(k) = ctx.w.get_mut(kid) {
        let (kx, ky) = (k.m.x() + dx, k.m.y() + dy);
        k.m.set_x(kx)?;
        k.m.set_y(ky)?;
    }
    Ok(Flow::Continue)
}
