//! Built-in `orbit` (mechanics `orbit`; docs/plans/sim-core.md 6.1, K9;
//! docs/format/builtins/orbit.md). Tier 3.
//!
//! Killers that move on a circle. Each Step an orbiting instance is placed at
//! `x = cx + r cos(a)`, `y = cy - r sin(a)`, then `a += speed`.
//!
//! - `mode = "self"` (little-runner object761, poronkusema objIronball): the instance orbits the
//!   center `(center_x, center_y)`. `radius` defaults to the distance from the instance to the
//!   center at Create. `angle` defaults to the direction from the instance to the center, which
//!   mirrors the first position across the center (object761 at (1840, 912) around (1840, 976)
//!   starts at (1840, 1040), TR-MECH-05, not run). `unit` is `degrees` (object761; the loader
//!   writes `speed` as minus its `qspd`, which the original subtracts) or `radians` (objIronball,
//!   where the angle wraps into 0 to 2 pi).
//! - `mode = "spawner"` (poronkusema objMakeIronball, kaenbyou roundingCherry): Create spawns
//!   `count` instances of `child` (a `self` orbit) at angles `angle + 360 / count * i`, then
//!   destroys itself unless `keep_spawner`.
//!
//! OPEN: the radians wrap of a negative angle; whether kaenbyou roundingCherry (which keeps
//! its cherries as children and moves them itself) differs from separate orbiters (same
//! positions); U-MECH-6 (the last bit of the trig).

use serde::{Deserialize, Serialize};

use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_error, param_kind,
    param_real, state_mut, state_of,
};
use crate::def::{KindDef, KindId, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::trig::{dcos, degtorad, dsin, point_direction, point_distance};
use crate::num::{self, Real};
use crate::world::{Ctx, Flow, InstId, SpawnInit};

const MODES: &[&str] = &["self", "spawner"];
const UNITS: &[&str] = &["degrees", "radians"];

const fn real(name: &'static str) -> ParamSpec {
    ParamSpec {
        name,
        ty: ParamType::Real,
        required: false,
    }
}

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "mode",
        ty: ParamType::Enum(MODES),
        required: false,
    },
    real("center_x"),
    real("center_y"),
    real("radius"),
    real("angle"),
    real("speed"),
    ParamSpec {
        name: "unit",
        ty: ParamType::Enum(UNITS),
        required: false,
    },
    real("count"),
    ParamSpec {
        name: "child",
        ty: ParamType::Kind,
        required: false,
    },
    ParamSpec {
        name: "keep_spawner",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "destroy_outside_room",
        ty: ParamType::Bool,
        required: false,
    },
];

/// An orbit settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct OrbitState {
    /// Center x.
    pub cx: Real,
    /// Center y.
    pub cy: Real,
    /// Radius; `None` is taken from the placement at Create.
    pub radius: Option<Real>,
    /// Angle (the unit below); `None` is the direction to the center at Create.
    pub angle: Option<Real>,
    /// Added to the angle each Step.
    pub speed: Real,
    /// The angle is in radians (else degrees).
    pub radians: bool,
    /// Spawner: the child kind and count.
    pub spawner: Option<(KindId, u32)>,
    /// A spawner stays after spawning.
    pub keep_spawner: bool,
    /// Destroyed outside the room.
    pub destroy_outside: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step for an orbiter, Outside Room when asked.
pub fn event_set(kind: &KindDef) -> EventSet {
    let spawner = matches!(kind.params.0.get("mode"), Some(ParamValue::Str(w)) if w == "spawner");
    EventSet {
        step: !spawner,
        outside_room: matches!(
            kind.params.0.get("destroy_outside_room"),
            Some(ParamValue::Bool(true))
        ),
        ..EventSet::default()
    }
}

/// A new instance state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let (Some(cx), Some(cy)) = (param_real(params, "center_x")?, param_real(params, "center_y")?) else {
        return Err(param_error("center_x", "an orbit needs center_x and center_y"));
    };
    let spawner = if param_enum(params, "mode", MODES)? == Some("spawner") {
        let child = param_kind(params, "child")?
            .ok_or_else(|| param_error("child", "a spawner needs `child` (an orbiting kind)"))?;
        let n = param_real(params, "count")?.map_or(1.0, Real::get);
        if n.fract() != 0.0 || !(1.0..=4096.0).contains(&n) {
            return Err(param_error("count", "must be a whole number from 1 to 4096"));
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked whole, in range"
        )]
        Some((child, n as u32))
    } else {
        None
    };
    let radius = param_real(params, "radius")?;
    // gm-compare: ok (load-time param validation, not a GML comparison)
    if radius.is_some_and(|r| r.get() < 0.0) {
        return Err(param_error("radius", "must not be negative"));
    }
    Ok(BuiltinState::Orbit(OrbitState {
        cx,
        cy,
        radius,
        angle: param_real(params, "angle")?,
        speed: param_real(params, "speed")?.unwrap_or(Real::ZERO),
        radians: param_enum(params, "unit", UNITS)? == Some("radians"),
        spawner,
        keep_spawner: param_bool(params, "keep_spawner")?.unwrap_or(false),
        destroy_outside: param_bool(params, "destroy_outside_room")?.unwrap_or(false),
    }))
}

/// Runs one event on an orbiter or spawner.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Orbit(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => match s.spawner {
            Some((child, n)) => spawn_children(ctx, me, &s, child, n),
            None => {
                create_orbiter(ctx, me, &s);
                Ok(Flow::Continue)
            }
        },
        EventKind::Step if s.spawner.is_none() => step(ctx, me, &s),
        EventKind::OutsideRoom if s.destroy_outside => Ok(ctx.destroy(me)),
        _ => Ok(Flow::Continue),
    }
}

/// Fills in the radius and angle the placement gives.
fn create_orbiter(ctx: &mut Ctx, me: InstId, s: &OrbitState) {
    let Some(inst) = ctx.w.get(me) else {
        return;
    };
    let (x, y) = (inst.m.x(), inst.m.y());
    let radius = s.radius.unwrap_or_else(|| point_distance(x, y, s.cx, s.cy));
    let angle = s.angle.unwrap_or_else(|| {
        let deg = point_direction(x, y, s.cx, s.cy);
        if s.radians { degtorad(deg) } else { deg }
    });
    if let Some(BuiltinState::Orbit(st)) = state_mut(ctx, me) {
        st.radius = Some(radius);
        st.angle = Some(angle);
    }
}

fn spawn_children(
    ctx: &mut Ctx,
    me: InstId,
    s: &OrbitState,
    child: KindId,
    n: u32,
) -> Result<Flow, SimError> {
    let start = s.angle.unwrap_or(Real::ZERO);
    let full = if s.radians {
        Real::new(std::f64::consts::TAU)
    } else {
        Real::new(360.0)
    };
    let (x, y) = ctx
        .w
        .get(me)
        .map_or((Real::ZERO, Real::ZERO), |i| (i.m.x(), i.m.y()));
    for i in 0..n {
        let angle = start + full / Real::new(f64::from(n)) * Real::new(f64::from(i));
        let mut p = Params::default();
        let mut put = |k: &str, v: Real| {
            p.0.insert(k.to_owned(), ParamValue::Real(v));
        };
        put("center_x", s.cx);
        put("center_y", s.cy);
        put("angle", angle);
        put("speed", s.speed);
        if let Some(r) = s.radius {
            put("radius", r);
        }
        p.0.insert(
            "unit".to_owned(),
            ParamValue::Str(if s.radians { "radians" } else { "degrees" }.into()),
        );
        ctx.spawn(
            child,
            x,
            y,
            SpawnInit {
                params: p,
                ..SpawnInit::default()
            },
        )?;
    }
    if s.keep_spawner {
        Ok(Flow::Continue)
    } else {
        Ok(ctx.destroy(me))
    }
}

fn step(ctx: &mut Ctx, me: InstId, s: &OrbitState) -> Result<Flow, SimError> {
    let (Some(radius), Some(angle)) = (s.radius, s.angle) else {
        return Ok(Flow::Continue);
    };
    let (cos, sin) = if s.radians {
        (num::cos(angle), num::sin(angle))
    } else {
        (dcos(angle), dsin(angle))
    };
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.m.set_x(s.cx + radius * cos)?;
        inst.m.set_y(s.cy - radius * sin)?;
    }
    let mut next = angle + s.speed;
    if s.radians {
        let tau = Real::new(std::f64::consts::TAU);
        let rules = &ctx.rules.num;
        if num::gm_ge(next, tau, rules) {
            next -= tau;
        } else if num::gm_lt(next, Real::new(0.0), rules) {
            next += tau;
        }
    }
    if let Some(BuiltinState::Orbit(st)) = state_mut(ctx, me) {
        st.angle = Some(next);
    }
    Ok(Flow::Continue)
}
