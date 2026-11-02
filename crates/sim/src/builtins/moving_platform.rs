//! Built-in `moving_platform` (mechanics `moving_platform`; kid/A1, A3, C; docs/plans/sim-core.md
//! 6.1; docs/format/builtins/moving_platform.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! The platform is non-solid and moves by hand in Step plus the runner's automatic motion:
//! upward motion is applied through `yspeed` in Step, downward motion through `vspeed` by
//! automatic motion afterwards (step 5 moves a negative `vspeed` into `yspeed` and a positive
//! `yspeed` into `vspeed`). Solids are the `place_free` test (solids only).
//!
//! `carry_rule`:
//! - `yuuutu` (deride-kotaro, A1 games, A4): 1. reverse `hspeed` if `(x + hspeed, y)` is not free;
//!   2. if `(x, y + vspeed + yspeed)` is not free, swap the vertical motion; 3. `y += yspeed`;
//!   4. a kid touching the platform mask moved 2 px up gets `y += vspeed + yspeed` and then
//!   `x += hspeed` if that spot is free for him; the flipped kid is probed 17 px below; 5. as
//!   above.
//! - `showdown_split` (showdown, kid/A3.md): the same Step, but the carry is two parts: while
//!   `hspeed` is not 0, a kid 2 px above gets `y += vspeed + yspeed` and `x += hspeed` if free;
//!   separately, while `vspeed` is not 0, he gets `y += vspeed + yspeed` and then `y -= vspeed`
//!   again if that spot is free.
//! - `xia_grav` (keep-celebrating-for-xia): runs only while `hspeed`, `vspeed` or `yspeed` is
//!   not 0; reverses (when `bounce`), carries a kid found at `y - 2 * grav`, then `y += yspeed`,
//!   then step 5. OPEN: the sign of `grav` is taken from `global.grav`.
//!
//! `bounce = false` skips the reversals (objMovingPlatformNoBounce). `start_on_touch`: the
//! platform stays still until a kid touches it, then takes `hspeed` and `vspeed` as its speeds.
//! `marker_reverse` is what a touch by a fake block marker does ([`reverse_on_marker`], called by
//! `fake_block`).
//!
//! OPEN, not done: `b_lines` (kaenbyou, End Step, two `collision_line` probes at the kid's
//! `yflag` heights): `Unsupported` at load.

use serde::{Deserialize, Serialize};

use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_real, state_mut, state_of,
};
use crate::collide::Target;
use crate::collide::query::{instance_place, place_free};
use crate::def::{ClassMask, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId, KindState, World};

const CARRY: &[&str] = &["yuuutu", "showdown_split", "b_lines", "xia_grav"];
const MARKER: &[&str] = &["both", "vertical", "none"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "hspeed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "vspeed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "bounce",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "start_on_touch",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "marker_reverse",
        ty: ParamType::Enum(MARKER),
        required: false,
    },
    ParamSpec {
        name: "carry_rule",
        ty: ParamType::Enum(CARRY),
        required: false,
    },
];

/// How the platform carries a kid.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Carry {
    /// Deride-kotaro and the A1, A4 games.
    Yuuutu,
    /// Showdown.
    ShowdownSplit,
    /// Keep-celebrating-for-xia.
    XiaGrav,
}

/// What a marker (fake block) touch does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Marker {
    /// Reverse every direction.
    Both,
    /// Reverse the vertical motion only (animus).
    Vertical,
    /// Nothing.
    None,
}

/// A moving platform's settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MovingPlatformState {
    /// The carry rule.
    pub carry: Carry,
    /// Reverse at solids.
    pub bounce: bool,
    /// What a marker touch does.
    pub marker: Marker,
    /// The speeds Create gives it (`hspeed`, `vspeed` params).
    pub start: (Real, Real),
    /// Not started yet (`start_on_touch`): the first kid touch gives it `start`.
    pub waiting: bool,
    /// The hand-applied vertical speed (`yspeed`).
    pub yspeed: Real,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step, and the kid touching it with `start_on_touch`.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        step: true,
        ..EventSet::default()
    };
    if matches!(kind.params.0.get("start_on_touch"), Some(ParamValue::Bool(true))) {
        set.collisions.push(super::warp::KIDS);
    }
    set
}

/// A new instance's state from its params. The initial `hspeed` and `vspeed` are applied (or
/// held back by `start_on_touch`) in Create.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let carry = match param_enum(params, "carry_rule", CARRY)? {
        Some("showdown_split") => Carry::ShowdownSplit,
        Some("xia_grav") => Carry::XiaGrav,
        Some("b_lines") => {
            return Err(super::param_error(
                "carry_rule",
                "b_lines (kaenbyou End Step lines) is not supported yet (OPEN)",
            ));
        }
        _ => Carry::Yuuutu,
    };
    let marker = match param_enum(params, "marker_reverse", MARKER)? {
        Some("vertical") => Marker::Vertical,
        Some("none") => Marker::None,
        _ => Marker::Both,
    };
    let start = (
        param_real(params, "hspeed")?.unwrap_or(Real::ZERO),
        param_real(params, "vspeed")?.unwrap_or(Real::ZERO),
    );
    Ok(BuiltinState::MovingPlatform(MovingPlatformState {
        carry,
        bounce: param_bool(params, "bounce")?.unwrap_or(true),
        marker,
        start,
        waiting: param_bool(params, "start_on_touch")?.unwrap_or(false),
        yspeed: Real::ZERO,
    }))
}

/// Runs one event on a moving platform.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::MovingPlatform(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            if !s.waiting {
                set_motion(ctx, me, s.start.0, s.start.1)?;
            }
            Ok(Flow::Continue)
        }
        EventKind::Collision { with, .. } if with.intersects(super::warp::KIDS) => {
            if s.waiting {
                set_motion(ctx, me, s.start.0, s.start.1)?;
                if let Some(BuiltinState::MovingPlatform(st)) = state_mut(ctx, me) {
                    st.waiting = false;
                }
            }
            Ok(Flow::Continue)
        }
        EventKind::Step => step(ctx, me, &s),
        _ => Ok(Flow::Continue),
    }
}

fn step(ctx: &mut Ctx, me: InstId, s: &MovingPlatformState) -> Result<Flow, SimError> {
    let Some(inst) = ctx.w.get(me) else {
        return Ok(Flow::Continue);
    };
    let (x, y) = (inst.m.x(), inst.m.y());
    let (h, v) = (inst.m.hspeed(), inst.m.vspeed());
    let moving = h.get().abs() > 0.0 || v.get().abs() > 0.0 || s.yspeed.get().abs() > 0.0;
    if s.carry == Carry::XiaGrav && !moving {
        return Ok(Flow::Continue);
    }
    let (mut h, mut v, mut ys) = (h, v, s.yspeed);
    if s.bounce {
        if !place_free(ctx, me, x + h, y)? {
            h = -h;
        }
        if !place_free(ctx, me, x, y + v + ys)? {
            if v.get().abs() > 0.0 {
                ys = -v;
                v = Real::ZERO;
            } else {
                v = -ys;
                ys = Real::ZERO;
            }
        }
    }
    set_motion(ctx, me, h, v)?;
    match s.carry {
        Carry::Yuuutu => {
            let new_y = y + ys;
            set_y(ctx, me, new_y)?;
            carry_yuuutu(ctx, me, x, new_y, h, v, ys)?;
        }
        Carry::ShowdownSplit => {
            let new_y = y + ys;
            set_y(ctx, me, new_y)?;
            carry_split(ctx, me, x, new_y, h, v, ys)?;
        }
        Carry::XiaGrav => {
            let grav = if ctx.w.engine.kid.grav { -2.0 } else { 2.0 };
            carry_one(
                ctx,
                me,
                ClassMask::KID | ClassMask::KID_FLIPPED,
                x,
                y - Real::new(grav),
                h,
                v + ys,
                true,
            )?;
            set_y(ctx, me, y + ys)?;
        }
    }
    // Step 5: negative vspeed moves into yspeed, positive yspeed into vspeed.
    if v.get() < 0.0 {
        ys = v;
        v = Real::ZERO;
    }
    if ys.get() > 0.0 {
        v = ys;
        ys = Real::ZERO;
    }
    set_motion(ctx, me, h, v)?;
    if let Some(BuiltinState::MovingPlatform(st)) = state_mut(ctx, me) {
        st.yspeed = ys;
    }
    Ok(Flow::Continue)
}

fn set_motion(ctx: &mut Ctx, me: InstId, h: Real, v: Real) -> Result<(), SimError> {
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.m.set_hspeed(h)?;
        inst.m.set_vspeed(v)?;
    }
    Ok(())
}

fn set_y(ctx: &mut Ctx, me: InstId, y: Real) -> Result<(), SimError> {
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.m.set_y(y)?;
    }
    Ok(())
}

/// The kid of class `class` touched when the platform's mask sits at `(x, y)`.
fn kid_at(ctx: &Ctx, me: InstId, x: Real, y: Real, class: ClassMask) -> Result<Option<InstId>, SimError> {
    instance_place(ctx, me, x, y, Target::Class(class))
}

/// Moves `kid` by `dy` in y, then by `dx` in x when that spot is free for him.
fn carry_kid(ctx: &mut Ctx, kid: InstId, dx: Real, dy: Real) -> Result<(), SimError> {
    let Some(k) = ctx.w.get(kid) else {
        return Ok(());
    };
    let (kx, ky) = (k.m.x(), k.m.y() + dy);
    if let Some(k) = ctx.w.get_mut(kid) {
        k.m.set_y(ky)?;
    }
    if place_free(ctx, kid, kx + dx, ky)?
        && let Some(k) = ctx.w.get_mut(kid)
    {
        k.m.set_x(kx + dx)?;
    }
    Ok(())
}

/// A kid found with the platform at `(x, py)`: carried by `(dx, dy)`.
#[allow(clippy::too_many_arguments, reason = "one carry, several probes")]
fn carry_one(
    ctx: &mut Ctx,
    me: InstId,
    class: ClassMask,
    x: Real,
    py: Real,
    dx: Real,
    dy: Real,
    _x_free: bool,
) -> Result<(), SimError> {
    if let Some(kid) = kid_at(ctx, me, x, py, class)? {
        carry_kid(ctx, kid, dx, dy)?;
    }
    Ok(())
}

/// The Yuuutu carry (step 4): the kid 2 px above, the flipped kid probed 17 px below.
fn carry_yuuutu(
    ctx: &mut Ctx,
    me: InstId,
    x: Real,
    y: Real,
    h: Real,
    v: Real,
    ys: Real,
) -> Result<(), SimError> {
    let dy = v + ys;
    carry_one(ctx, me, ClassMask::KID, x, y - Real::new(2.0), h, dy, true)?;
    carry_one(
        ctx,
        me,
        ClassMask::KID_FLIPPED,
        x,
        y + Real::new(17.0),
        h,
        dy,
        true,
    )
}

/// The showdown carry: a horizontal part while `hspeed` is not 0 and a vertical part while
/// `vspeed` is not 0.
fn carry_split(
    ctx: &mut Ctx,
    me: InstId,
    x: Real,
    y: Real,
    h: Real,
    v: Real,
    ys: Real,
) -> Result<(), SimError> {
    let probe = y - Real::new(2.0);
    if h.get().abs() > 0.0
        && let Some(kid) = kid_at(ctx, me, x, probe, ClassMask::KID)?
    {
        carry_kid(ctx, kid, h, v + ys)?;
    }
    if v.get().abs() > 0.0
        && let Some(kid) = kid_at(ctx, me, x, probe, ClassMask::KID)?
    {
        carry_kid(ctx, kid, Real::ZERO, v + ys)?;
        // "Then subtract the platform's vspeed again if that spot is free."
        if let Some(k) = ctx.w.get(kid) {
            let (kx, ky) = (k.m.x(), k.m.y() - v);
            if place_free(ctx, kid, kx, ky)?
                && let Some(k) = ctx.w.get_mut(kid)
            {
                k.m.set_y(ky)?;
            }
        }
    }
    Ok(())
}

/// What a fake-block marker does to the platform (`marker_reverse`): `both` reverses every
/// direction, `vertical` only the vertical motion, `none` nothing. Called by `fake_block`.
pub fn reverse_on_marker(ctx: &mut Ctx, id: InstId) -> Result<(), SimError> {
    let Some(BuiltinState::MovingPlatform(s)) = state_of(ctx, id) else {
        return Ok(());
    };
    let (marker, ys) = (s.marker, s.yspeed);
    let Some(inst) = ctx.w.get(id) else {
        return Ok(());
    };
    let (h, v) = (inst.m.hspeed(), inst.m.vspeed());
    match marker {
        Marker::None => return Ok(()),
        Marker::Both => {
            if let Some(inst) = ctx.w.get_mut(id) {
                inst.m.set_hspeed(-h)?;
            }
        }
        Marker::Vertical => {}
    }
    if let Some(inst) = ctx.w.get_mut(id) {
        inst.m.set_vspeed(-v)?;
    }
    if let Some(BuiltinState::MovingPlatform(st)) = state_mut(ctx, id) {
        st.yspeed = -ys;
    }
    Ok(())
}

/// The hand-applied vertical speed of a moving platform (0 for anything else).
pub fn yspeed_of(w: &World, id: InstId) -> Real {
    match w.get(id).map(|i| &i.state) {
        Some(KindState::Builtin(BuiltinState::MovingPlatform(s))) => s.yspeed,
        _ => Real::ZERO,
    }
}
