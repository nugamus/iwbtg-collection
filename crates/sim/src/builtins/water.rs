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
//! `area` (NY water, kid/A4.md): with `area_left`, `area_right`, `area_top` and `area_bottom` the
//! water has no collision event; each Step, every kid whose origin is strictly inside the
//! rectangle (`left < x < right`, `top < y < bottom`) gets the same effects (cap, refresh,
//! air jump). OPEN: the spec leaves the edge handling untested (kid-a4-water stayed inside).
//!
//! Also, in this order after the refresh and the air jump:
//!
//! - `air_jump = "djump_ignoring_flag"` (JUN objwater2): on a Jump press the -7 jump happens
//!   whether or not `djump` is set, then `djump` is cleared (`kid::api::set_air_jump`).
//! - `cur_jumps = n` (kaenbyou objWater2: 2): `kid::api::set_cur_jumps`.
//! - `release_same_tick = true` (WAN water, ANI mugenjump, RBF object945): when Jump was released
//!   this tick the kid's variable-jump release runs now (`kid::api::run_jump_release`).

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

const AREA: [&str; 4] = ["area_left", "area_right", "area_top", "area_bottom"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "cur_jumps",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "release_same_tick",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "area_left",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "area_right",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "area_top",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "area_bottom",
        ty: ParamType::Real,
        required: false,
    },
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
    /// NY: the rectangle `(left, right, top, bottom)` tested against each kid's origin in Step,
    /// instead of a collision event.
    pub area: Option<(Real, Real, Real, Real)>,
    /// `djump_ignoring_flag`: the press jumps without `djump`, then clears it.
    pub air_ignores_flag: bool,
    /// Family B: `curJumps` is set to this on touch.
    pub cur_jumps: Option<u8>,
    /// A Jump release this tick cuts the jump now.
    pub release_same_tick: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: the kid touching it.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    if AREA.iter().any(|n| kind.params.0.contains_key(*n)) {
        set.step = true;
    } else {
        set.collisions.push(KIDS);
    }
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
    let cur_jumps = match param_real(params, "cur_jumps")? {
        None => None,
        // gm-compare: ok (load-time param validation)
        Some(v) => match (0..=3u8).find(|w| (f64::from(*w) - v.get()).abs() < 1e-9) {
            Some(n) => Some(n),
            None => return Err(super::param_error("cur_jumps", "0, 1, 2 or 3")),
        },
    };
    let corners: Vec<Option<Real>> = AREA
        .iter()
        .map(|n| param_real(params, n))
        .collect::<Result<_, _>>()?;
    let area = match corners.as_slice() {
        [None, None, None, None] => None,
        [Some(l), Some(r), Some(t), Some(b)] => Some((*l, *r, *t, *b)),
        _ => {
            return Err(super::param_error(
                "area_left",
                "give all of area_left, area_right, area_top and area_bottom, or none",
            ));
        }
    };
    Ok(BuiltinState::Water(WaterState {
        area,
        // gm-compare: ok (param: a negative cap means none)
        fall_cap: if cap.get() < 0.0 { None } else { Some(cap) },
        refresh: param_enum(params, "refresh", REFRESH)? == Some("on_touch"),
        air_jump: air == Some("press_if_no_djump") || air == Some("djump_ignoring_flag"),
        air_ignores_flag: air == Some("djump_ignoring_flag"),
        cur_jumps,
        release_same_tick: param_bool(params, "release_same_tick")?.unwrap_or(false),
        kid_jump2: param_enum(params, "air_jump_speed", AIR_SPEED)? == Some("kid_jump2"),
    }))
}

/// Whether the kid has its double jump left.
fn has_djump(ctx: &Ctx, kid: InstId) -> bool {
    let f = KidField {
        name: "djump".to_owned(),
        index: None,
    };
    // gm-compare: ok (a kid field that reads 1 or 0 as a flag)
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
    let Some(BuiltinState::Water(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match (ev, s.area) {
        (EventKind::Collision { with, other }, None) if with.intersects(KIDS) => affect(ctx, &s, *other),
        (EventKind::Step, Some((left, right, top, bottom))) => {
            let kids: Vec<InstId> = ctx
                .w
                .instances()
                .iter()
                .filter(|i| i.alive && i.classes.intersects(KIDS))
                .filter(|i| {
                    let (x, y) = (i.m.x(), i.m.y());
                    num::gm_lt(left, x, &ctx.rules.num)
                        && num::gm_lt(x, right, &ctx.rules.num)
                        && num::gm_lt(top, y, &ctx.rules.num)
                        && num::gm_lt(y, bottom, &ctx.rules.num)
                })
                .map(|i| i.id)
                .collect();
            for kid in kids {
                affect(ctx, &s, kid)?;
            }
            Ok(Flow::Continue)
        }
        _ => Ok(Flow::Continue),
    }
}

/// What the water does to one kid: cap, refresh, air jump.
fn affect(ctx: &mut Ctx, s: &WaterState, kid: InstId) -> Result<Flow, SimError> {
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
    if s.air_jump && ctx.w.input.pressed(Action::Jump) && (s.air_ignores_flag || !has_djump(ctx, kid)) {
        let speed = if s.kid_jump2 {
            jump2(ctx, kid)
        } else {
            Real::new(7.0)
        };
        if let Some(k) = ctx.w.get_mut(kid) {
            k.m.set_vspeed(-speed * sign)?;
        }
        if s.air_ignores_flag {
            api::set_air_jump(ctx, kid, false);
        }
    }
    if let Some(n) = s.cur_jumps {
        api::set_cur_jumps(ctx, kid, n);
    }
    if s.release_same_tick && ctx.w.input.released(Action::Jump) {
        api::run_jump_release(ctx, kid)?;
    }
    Ok(Flow::Continue)
}

/// True for a kind declared with a `fall_cap` of its own (for tools that list water kinds).
pub fn declares_cap(kind: &KindDef) -> bool {
    matches!(kind.params.0.get("fall_cap"), Some(ParamValue::Real(_)))
}
