//! Built-in `camera` (mechanics `camera`; CLONE_SPEC rooms "Views"; docs/plans/sim-core.md 4.5,
//! 6.1; docs/format/builtins/camera.md). Tier 1. WP0 creates the file; WP6a owns it.
//!
//! Camera objects write view 0. Their Step placement gives the 1-tick lag the spec infers: the
//! camera's Step reads the kid before this tick's motion (Confirmed (recorder) little-runner
//! `tr-room-11.csv`); the lag is the dispatch order, not code here. The first kid (either kind,
//! R21) is the target.
//!
//! - `screen_snap` (rely, poronkusema, machinary, most games): `view_xview = floor(kid x /
//!   screen_w) * screen_w` and `view_yview = floor(kid y / screen_h) * screen_h + y_offset`
//!   (machinary +4), in the events the `on_*` params name (Step; Room Start and Room End in some
//!   games).
//! - `smooth_follow` (kaenbyou objSmoothView): Create makes the camera view 0's followed object
//!   (`border_x`, `border_y`: 400, 304) and jumps to the kid. Each Step with a kid, its hspeed
//!   is the x distance over `h_div` (16) and its vspeed the y distance over `v_div` (12), each
//!   zeroed when the kid is within `tolerance` (2, inclusive) on that axis; with no kid both are
//!   0. Automatic motion and the runner's view follow (WP4b) do the rest.
//! - `end_step_clamp` (keep-celebrating-for-xia objCamera): like `screen_snap` with the view's
//!   own size, after clamping the followed point to the room (0 to size - 1), in Create and End
//!   Step.
//!
//! OPEN: xia's `leaveRoom` switch (no clamp) and the flipped kid's own camera variant (rely
//! follows `player2` when flipped: the first kid of either kind covers it).

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_real, state_of};
use crate::collide::Target;
use crate::def::{KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::{self, Real};
use crate::world::{Ctx, Flow, InstId};

const MODES: &[&str] = &["screen_snap", "smooth_follow", "end_step_clamp"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "mode",
        ty: ParamType::Enum(MODES),
        required: false,
    },
    ParamSpec {
        name: "screen_w",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "screen_h",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "y_offset",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "on_step",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "on_room_start",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "on_room_end",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "h_div",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "v_div",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "border_x",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "border_y",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "tolerance",
        ty: ParamType::Real,
        required: false,
    },
];

/// What the camera does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Mode {
    /// One screen at a time.
    ScreenSnap,
    /// A drifting target for the runner's view follow.
    SmoothFollow,
    /// Screen snap with the view's size, clamped, in End Step.
    EndStepClamp,
}

/// A camera's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CameraState {
    /// The mode.
    pub mode: Mode,
    /// Screen size for `screen_snap`.
    pub screen: (Real, Real),
    /// Added to the y of the snapped view.
    pub y_offset: Real,
    /// Snap in Step.
    pub on_step: bool,
    /// Snap in Room Start.
    pub on_room_start: bool,
    /// Snap in Room End.
    pub on_room_end: bool,
    /// `smooth_follow`: x speed is the distance over this.
    pub h_div: Real,
    /// `smooth_follow`: y speed is the distance over this.
    pub v_div: Real,
    /// `smooth_follow`: view 0 borders.
    pub border: (Real, Real),
    /// `smooth_follow`: no speed within this distance (inclusive).
    pub tolerance: Real,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

fn mode_of(kind: &KindDef) -> Mode {
    match kind.params.0.get("mode") {
        Some(ParamValue::Str(s)) if s == "smooth_follow" => Mode::SmoothFollow,
        Some(ParamValue::Str(s)) if s == "end_step_clamp" => Mode::EndStepClamp,
        _ => Mode::ScreenSnap,
    }
}

fn flag(kind: &KindDef, name: &str, default: bool) -> bool {
    match kind.params.0.get(name) {
        Some(ParamValue::Bool(b)) => *b,
        _ => default,
    }
}

/// What a kind with this base handles, from the kind's params.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    match mode_of(kind) {
        Mode::ScreenSnap => {
            set.step = flag(kind, "on_step", true);
            set.room_start = flag(kind, "on_room_start", false);
            set.room_end = flag(kind, "on_room_end", false);
        }
        Mode::SmoothFollow => set.step = true,
        Mode::EndStepClamp => set.end_step = true,
    }
    set
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let r = |name: &str, d: f64| Ok::<Real, ParamError>(param_real(params, name)?.unwrap_or(Real::new(d)));
    let mode = match param_enum(params, "mode", MODES)? {
        Some("smooth_follow") => Mode::SmoothFollow,
        Some("end_step_clamp") => Mode::EndStepClamp,
        _ => Mode::ScreenSnap,
    };
    let screen = (r("screen_w", 800.0)?, r("screen_h", 608.0)?);
    let (h_div, v_div) = (r("h_div", 16.0)?, r("v_div", 12.0)?);
    // gm-compare: ok (load-time param validation)
    if screen.0.get() <= 0.0 || screen.1.get() <= 0.0 {
        return Err(super::param_error("screen_w", "the screen size must be positive"));
    }
    // gm-compare: ok (load-time param validation)
    if h_div.get() <= 0.0 || v_div.get() <= 0.0 {
        return Err(super::param_error("h_div", "the divisors must be positive"));
    }
    Ok(BuiltinState::Camera(CameraState {
        mode,
        screen,
        y_offset: r("y_offset", 0.0)?,
        on_step: param_bool(params, "on_step")?.unwrap_or(true),
        on_room_start: param_bool(params, "on_room_start")?.unwrap_or(false),
        on_room_end: param_bool(params, "on_room_end")?.unwrap_or(false),
        h_div,
        v_div,
        border: (r("border_x", 400.0)?, r("border_y", 304.0)?),
        tolerance: r("tolerance", 2.0)?,
    }))
}

/// Runs one event on a camera.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Camera(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match (s.mode, ev) {
        (Mode::ScreenSnap, EventKind::Step) if s.on_step => snap(ctx, &s, s.screen, false)?,
        (Mode::ScreenSnap, EventKind::RoomStart) if s.on_room_start => snap(ctx, &s, s.screen, false)?,
        (Mode::ScreenSnap, EventKind::RoomEnd) if s.on_room_end => snap(ctx, &s, s.screen, false)?,
        (Mode::EndStepClamp, EventKind::Create | EventKind::EndStep) => {
            let v = &ctx.w.views[0];
            let size = (v.w, v.h);
            snap(ctx, &s, size, true)?;
        }
        (Mode::SmoothFollow, EventKind::Create) => smooth_create(ctx, me, &s)?,
        (Mode::SmoothFollow, EventKind::Step) => smooth_step(ctx, me, &s)?,
        _ => {}
    }
    Ok(Flow::Continue)
}

/// Snaps view 0 to the screen that holds the kid; `clamp` first clamps the kid's point to the
/// room.
fn snap(ctx: &mut Ctx, s: &CameraState, size: (Real, Real), clamp: bool) -> Result<(), SimError> {
    let rules = ctx.rules;
    let Some(kid) = ctx.w.kid(&rules.tick) else {
        return Ok(());
    };
    let Some(k) = ctx.w.get(kid) else {
        return Ok(());
    };
    let (mut px, mut py) = (k.m.x(), k.m.y());
    if clamp {
        let room = ctx.def.room(ctx.w.room);
        let (rw, rh) = room.map_or((Real::ZERO, Real::ZERO), |r| {
            (Real::new(f64::from(r.width)), Real::new(f64::from(r.height)))
        });
        let n = &rules.num;
        px = num::gm_max(num::gm_min(px, rw - Real::ONE, n), Real::ZERO, n);
        py = num::gm_max(num::gm_min(py, rh - Real::ONE, n), Real::ZERO, n);
    }
    // gm-compare: ok (load-time param validation)
    if size.0.get() <= 0.0 || size.1.get() <= 0.0 {
        return Ok(());
    }
    let n = &rules.num;
    let x = num::floor(px / size.0, n) * size.0;
    let y = num::floor(py / size.1, n) * size.1 + s.y_offset;
    let v = &mut ctx.w.views[0];
    v.x = x;
    v.y = y;
    Ok(())
}

fn smooth_create(ctx: &mut Ctx, me: InstId, s: &CameraState) -> Result<(), SimError> {
    let v = &mut ctx.w.views[0];
    v.follow = Some(Target::Inst(me));
    v.hborder = s.border.0;
    v.vborder = s.border.1;
    let rules = ctx.rules;
    if let Some(kid) = ctx.w.kid(&rules.tick)
        && let Some(k) = ctx.w.get(kid)
    {
        let (kx, ky) = (k.m.x(), k.m.y());
        if let Some(inst) = ctx.w.get_mut(me) {
            inst.m.set_x(kx)?;
            inst.m.set_y(ky)?;
        }
    }
    Ok(())
}

fn smooth_step(ctx: &mut Ctx, me: InstId, s: &CameraState) -> Result<(), SimError> {
    let rules = ctx.rules;
    let target = ctx
        .w
        .kid(&rules.tick)
        .and_then(|kid| ctx.w.get(kid))
        .map(|k| (k.m.x(), k.m.y()));
    let Some(inst) = ctx.w.get(me) else {
        return Ok(());
    };
    let (x, y) = (inst.m.x(), inst.m.y());
    let (h, v) = match target {
        None => (Real::ZERO, Real::ZERO),
        Some((kx, ky)) => {
            let n = &rules.num;
            let axis = |d: Real, div: Real| {
                if num::gm_le(num::abs(d), s.tolerance, n) {
                    Real::ZERO
                } else {
                    d / div
                }
            };
            (axis(kx - x, s.h_div), axis(ky - y, s.v_div))
        }
    };
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.m.set_hspeed(h)?;
        inst.m.set_vspeed(v)?;
    }
    Ok(())
}
