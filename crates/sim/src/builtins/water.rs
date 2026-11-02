//! Built-in `water` (mechanics `water`; kid specs per game; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/water.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! Collision with the kid (either kind, one event per kid). In this order:
//!
//! 1. `fall_cap`: a downward speed above the cap is set to it (2). For the flipped kid "down" is
//!    up: the speed times its gravity sign is capped (xia, KAE). `fall_cap` absent is no cap
//!    (LR tipDJump, RBF object945).
//! 2. `refresh = "on_touch"`: `kid::api::refresh_air_jump` (objwater, ANI objwater).
//! 3. `air_jump = "press_if_no_djump"`: when Jump was pressed this tick and the kid has no
//!    double jump left, `vspeed` becomes minus `air_jump_speed` (7, or the kid's second jump
//!    speed with `kid_jump2`; LR tipWater, SHO mizu01, ANI mugenjump). The double-jump sound is
//!    the client's.
//!
//! The full jump from 1 px above the water (`ground_jump`) is the kid's: it tests the class
//! (`water1` allows it, `water2` and `water3` do not), which the loader sets from this param.
//!
//! OPEN, not done: `air_jump = "djump_ignoring_flag"` (JUN objwater2: the -7 jump without
//! `djump`, then `djump` is cleared; needs a `kid::api` setter for `djump`), the same-tick
//! release clause (the kid's variable-jump release), NY's rectangle `area` (tests the kid's
//! origin against a rectangle in Step) and family B's `curJumps = 2`.

use serde::{Deserialize, Serialize};

use super::warp::KIDS;
use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_real, state_of};
use crate::def::{ClassMask, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::input::Action;
use crate::kid::api::{self, KidField};
use crate::num::{self, Real};
use crate::world::{Ctx, Flow, InstId, Value};

const REFRESH: &[&str] = &["none", "on_touch"];
const AIR_JUMP: &[&str] = &["none", "djump_ignoring_flag", "press_if_no_djump"];
const AIR_SPEED: &[&str] = &["fixed_7", "kid_jump2"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "variant",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "fall_cap",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "refresh",
        ty: ParamType::Enum(REFRESH),
        required: false,
    },
    ParamSpec {
        name: "ground_jump",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "air_jump",
        ty: ParamType::Enum(AIR_JUMP),
        required: false,
    },
    ParamSpec {
        name: "air_jump_speed",
        ty: ParamType::Enum(AIR_SPEED),
        required: false,
    },
];

/// A water's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WaterState {
    /// The downward speed cap; `None` is no cap.
    pub fall_cap: Option<Real>,
    /// Refresh the air jump on touch.
    pub refresh: bool,
    /// Jump press with no double jump left gives an air jump.
    pub air_jump: bool,
    /// The air jump uses the kid's second jump speed instead of 7.
    pub kid_jump2: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: the kid touching it.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    set.collisions.push(KIDS);
    set
}

/// A new instance's state from its params. The default cap is 2 (A1 objwater and objwater2);
/// `fall_cap = -1` means no cap.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    param_bool(params, "ground_jump")?;
    // `variant` (1, 2, 3) picks the class water1, water2 or water3: the loader's.
    param_real(params, "variant")?;
    let cap = param_real(params, "fall_cap")?.unwrap_or(Real::new(2.0));
    let air = param_enum(params, "air_jump", AIR_JUMP)?;
    if air == Some("djump_ignoring_flag") {
        return Err(super::param_error(
            "air_jump",
            "djump_ignoring_flag needs a kid::api setter for djump (OPEN)",
        ));
    }
    Ok(BuiltinState::Water(WaterState {
        fall_cap: if cap.get() < 0.0 { None } else { Some(cap) },
        refresh: param_enum(params, "refresh", REFRESH)? == Some("on_touch"),
        air_jump: air == Some("press_if_no_djump"),
        kid_jump2: param_enum(params, "air_jump_speed", AIR_SPEED)? == Some("kid_jump2"),
    }))
}

/// Whether the kid has its double jump left.
fn has_djump(ctx: &Ctx, kid: InstId) -> bool {
    let f = KidField {
        name: "djump".to_owned(),
        index: None,
    };
    matches!(api::read_field(ctx.w, kid, &f), Some(Value::Real(v)) if v.get() > 0.0)
}

/// The kid's second jump speed (`jump[1]`), 7 when the profile has none.
fn jump2(ctx: &Ctx, kid: InstId) -> Real {
    let f = KidField {
        name: "jump".to_owned(),
        index: Some(1),
    };
    match api::read_field(ctx.w, kid, &f) {
        Some(Value::Real(v)) => v,
        _ => Real::new(7.0),
    }
}

/// Runs one event on a water.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let EventKind::Collision { with, other } = ev else {
        return Ok(Flow::Continue);
    };
    if !with.intersects(KIDS) {
        return Ok(Flow::Continue);
    }
    let Some(BuiltinState::Water(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    let kid = *other;
    let Some(k) = ctx.w.get(kid) else {
        return Ok(Flow::Continue);
    };
    let sign = if k.classes.intersects(ClassMask::KID_FLIPPED) || ctx.w.engine.kid.grav {
        Real::new(-1.0)
    } else {
        Real::ONE
    };
    let v = k.m.vspeed();
    if let Some(cap) = s.fall_cap
        && num::gm_gt(v * sign, cap, &ctx.rules.num)
        && let Some(k) = ctx.w.get_mut(kid)
    {
        k.m.set_vspeed(cap * sign)?;
    }
    if s.refresh {
        api::refresh_air_jump(ctx, kid);
    }
    if s.air_jump && ctx.w.input.pressed(Action::Jump) && !has_djump(ctx, kid) {
        let speed = if s.kid_jump2 {
            jump2(ctx, kid)
        } else {
            Real::new(7.0)
        };
        if let Some(k) = ctx.w.get_mut(kid) {
            k.m.set_vspeed(-speed * sign)?;
        }
    }
    Ok(Flow::Continue)
}

/// True for a kind declared with a `fall_cap` of its own (for tools that list water kinds).
pub fn declares_cap(kind: &KindDef) -> bool {
    matches!(kind.params.0.get("fall_cap"), Some(ParamValue::Real(_)))
}
