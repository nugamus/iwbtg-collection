//! Built-in `trap` (mechanics `trap`; docs/plans/sim-core.md 6.1; docs/format/builtins/trap.md).
//! Tier 2. WP0 creates the file; WP6b owns it.
//!
//! A trap launches once, when its trigger has fired (`trigger_model`, see `trigger::fired`) and
//! it is idle: `hspeed` and `vspeed` both 0, and with `flag_guard` (or a stop chain) its own
//! launch flag still 0. `launch`:
//!
//! - `velocity`: `hspeed`, `vspeed` (the fixed-speed forms too).
//! - `angle`: `motion_set(dir, speed)` (NY object321, object322).
//! - `path`: Step sets alarm 0 to 1; Alarm 0 starts the path (`path_speed`, `path_end`,
//!   `path_absolute`, `path_scale` when not 0) and sets the flag (FED path3, FT object866).
//! - `move_to` (little-runner object749, object751-object755): runs only while `endx` or `endy`
//!   is 0 and `global.stop[trg]` is set. For each axis with a target it moves `hspd` (`vspd`)
//!   toward it per tick, then within half a step snaps to the target, zeroes the step and
//!   replaces the target with 1.
//!
//! Stop chain (`alm` and friends; animus rise01 family): launch sets flag 1, then arms Alarm 0
//! (`alm`, only when above 0) and flag 2. Alarm 0 stops it and arms Alarm 1 (`alm2`). Alarm 1
//! relaunches with `h2`, `v2` when idle, flag 2 and the trigger still matches (flag 3), then
//! arms Alarm 2 (`alm3`, flag 4). Alarm 2 stops it and arms Alarm 3 (`alm4`). Alarm 3
//! relaunches with `h3`, `v3` when idle, flag 4 and the trigger still matches (flag 5).
//!
//! `depth_reveal`: Create sets depth 1000002; once launched and more than 32 px from the start
//! in x or y, depth becomes 0. `stop_on_stopper`: a touch of a `trap_stopper` class instance
//! stops it (`zero_speed`) or first snaps it flush to the stopper edge on the side it moves
//! toward, horizontal first (`flush`). `destroy_outside_room` destroys it outside the room.
//! The sound and `kind` (the class the loader gives it) are the loader's and the client's.
//!
//! OPEN, not done: the kaenbyou pathFree End Step conversion of a running path to straight
//! motion; U-MECH-4 (the path start tick) and U-MECH-8 (stop chain timing) follow the generic
//! alarm and path rules until traces answer them.

use serde::{Deserialize, Serialize};

use super::trigger::{self, MODEL_WORDS, Model};
use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_error, param_kind,
    param_real, state_mut, state_of,
};
use crate::alarm;
use crate::collide::bbox::{instance_box, real_of};
use crate::def::{ClassMask, KindDef, KindId, ParamValue, Params, PathId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::{self, Real};
use crate::path::{self, PathEnd};
use crate::world::{Ctx, Flow, InstId};

const LAUNCH: &[&str] = &["velocity", "angle", "path", "move_to"];
const STOPPER: &[&str] = &["none", "zero_speed", "flush"];
const PATH_END: &[&str] = &["stop", "restart", "continue", "reverse"];

const fn real(name: &'static str) -> ParamSpec {
    ParamSpec {
        name,
        ty: ParamType::Real,
        required: false,
    }
}

const fn flag(name: &'static str) -> ParamSpec {
    ParamSpec {
        name,
        ty: ParamType::Bool,
        required: false,
    }
}

const SPECS: &[ParamSpec] = &[
    real("trigger_id"),
    ParamSpec {
        name: "trigger_model",
        ty: ParamType::Enum(MODEL_WORDS),
        required: false,
    },
    ParamSpec {
        name: "trigger_kind",
        ty: ParamType::Kind,
        required: false,
    },
    ParamSpec {
        name: "trigger_kind_b",
        ty: ParamType::Kind,
        required: false,
    },
    ParamSpec {
        name: "trigger_kind_c",
        ty: ParamType::Kind,
        required: false,
    },
    ParamSpec {
        name: "launch",
        ty: ParamType::Enum(LAUNCH),
        required: false,
    },
    real("hspeed"),
    real("vspeed"),
    real("dir"),
    real("speed"),
    real("path"),
    real("path_speed"),
    ParamSpec {
        name: "path_end",
        ty: ParamType::Enum(PATH_END),
        required: false,
    },
    flag("path_absolute"),
    real("path_scale"),
    real("endx"),
    real("endy"),
    real("hspd"),
    real("vspd"),
    real("alm"),
    real("alm2"),
    real("alm3"),
    real("alm4"),
    real("h2"),
    real("v2"),
    real("h3"),
    real("v3"),
    flag("flag_guard"),
    ParamSpec {
        name: "stop_on_stopper",
        ty: ParamType::Enum(STOPPER),
        required: false,
    },
    flag("depth_reveal"),
    flag("destroy_outside_room"),
    ParamSpec {
        name: "kind",
        ty: ParamType::Enum(&["killer", "block", "cover", "other"]),
        required: false,
    },
    ParamSpec {
        name: "sound",
        ty: ParamType::Sound,
        required: false,
    },
    flag("sound_if_trigger"),
];

/// How the trap launches.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Launch {
    /// `hspeed`, `vspeed`.
    Velocity,
    /// `motion_set(dir, speed)`.
    Angle,
    /// A path, started by Alarm 0.
    Path,
    /// Toward `endx`, `endy`.
    MoveTo,
}

/// What a trap stopper does to it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Stopper {
    /// Nothing.
    None,
    /// Speed 0.
    ZeroSpeed,
    /// Flush to the stopper, then speed 0.
    Flush,
}

/// The stop chain's times and speeds.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Chain {
    /// Alarm 0, 1, 2, 3 values (`alm`, `alm2`, `alm3`, `alm4`); 0 is unset.
    pub alarms: [Real; 4],
    /// The second leg's `(h, v)`.
    pub leg2: (Real, Real),
    /// The third leg's `(h, v)`.
    pub leg3: (Real, Real),
}

/// A path launch's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PathLaunch {
    /// The path.
    pub path: PathId,
    /// `path_speed`.
    pub speed: Real,
    /// End action.
    pub end: PathEnd,
    /// Absolute coordinates.
    pub absolute: bool,
    /// `path_scale`, applied when not 0.
    pub scale: Real,
}

/// A trap's settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TrapState {
    /// The trigger id it waits for.
    pub trg: Real,
    /// How its trigger tells it.
    pub model: Model,
    /// The trigger kinds it reads (`shared_last`, `per_object`).
    pub kinds: Vec<KindId>,
    /// The launch form.
    pub launch: Launch,
    /// Velocity or angle launch: `(hspeed, vspeed)` or `(dir, speed)`.
    pub first: (Real, Real),
    /// Path settings.
    pub path: Option<PathLaunch>,
    /// Move-to `(endx, endy, hspd, vspd)`; the targets and steps change as it moves.
    pub move_to: (Real, Real, Real, Real),
    /// The stop chain.
    pub chain: Option<Chain>,
    /// Needs its own flag 0 to launch.
    pub flag_guard: bool,
    /// Stopper response.
    pub stopper: Stopper,
    /// Reveal after 32 px.
    pub depth_reveal: bool,
    /// Destroy outside the room.
    pub destroy_outside: bool,
    /// The launch flag (0 idle, 1 launched, chain 2-5).
    pub flag: u8,
    /// The depth has been revealed.
    pub revealed: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

fn kind_real(k: &KindDef, name: &str) -> bool {
    matches!(k.params.0.get(name), Some(ParamValue::Real(_)))
}

/// What a kind with this base handles: Step, the chain and path alarms, Outside Room and the
/// trap stopper.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        step: true,
        ..EventSet::default()
    };
    let path = matches!(kind.params.0.get("launch"), Some(ParamValue::Str(w)) if w == "path");
    let chain = ["alm", "alm2", "alm3", "alm4"].iter().any(|n| kind_real(kind, n));
    if path || chain {
        set.alarms = 0b1111;
    }
    if matches!(
        kind.params.0.get("destroy_outside_room"),
        Some(ParamValue::Bool(true))
    ) {
        set.outside_room = true;
    }
    if matches!(kind.params.0.get("stop_on_stopper"), Some(ParamValue::Str(w)) if w != "none") {
        set.collisions.push(ClassMask::TRAP_STOPPER);
    }
    set
}

fn r0(p: &Params, name: &str) -> Result<Real, ParamError> {
    Ok(param_real(p, name)?.unwrap_or(Real::ZERO))
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let launch = match param_enum(params, "launch", LAUNCH)? {
        Some("angle") => Launch::Angle,
        Some("path") => Launch::Path,
        Some("move_to") => Launch::MoveTo,
        _ => Launch::Velocity,
    };
    let first = if launch == Launch::Angle {
        (r0(params, "dir")?, r0(params, "speed")?)
    } else {
        (r0(params, "hspeed")?, r0(params, "vspeed")?)
    };
    let path = if launch == Launch::Path {
        let idx =
            param_real(params, "path")?.ok_or_else(|| param_error("path", "the path launch needs `path`"))?;
        let id = num::to_int_round(idx)
            .ok()
            .and_then(|i| u32::try_from(i).ok())
            .ok_or_else(|| param_error("path", "must be a path index"))?;
        Some(PathLaunch {
            path: PathId(id),
            speed: r0(params, "path_speed")?,
            end: match param_enum(params, "path_end", PATH_END)? {
                Some("restart") => PathEnd::Restart,
                Some("continue") => PathEnd::Continue,
                Some("reverse") => PathEnd::Reverse,
                _ => PathEnd::Stop,
            },
            absolute: param_bool(params, "path_absolute")?.unwrap_or(false),
            scale: r0(params, "path_scale")?,
        })
    } else {
        None
    };
    let chain = ["alm", "alm2", "alm3", "alm4"]
        .iter()
        .any(|n| params.0.contains_key(*n))
        .then(|| -> Result<Chain, ParamError> {
            Ok(Chain {
                alarms: [
                    r0(params, "alm")?,
                    r0(params, "alm2")?,
                    r0(params, "alm3")?,
                    r0(params, "alm4")?,
                ],
                leg2: (r0(params, "h2")?, r0(params, "v2")?),
                leg3: (r0(params, "h3")?, r0(params, "v3")?),
            })
        })
        .transpose()?;
    if chain.is_some() && launch != Launch::Velocity {
        return Err(param_error("alm", "a stop chain needs the velocity launch"));
    }
    let mut kinds = Vec::new();
    for n in ["trigger_kind", "trigger_kind_b", "trigger_kind_c"] {
        kinds.extend(param_kind(params, n)?);
    }
    Ok(BuiltinState::Trap(TrapState {
        trg: r0(params, "trigger_id")?,
        model: Model::parse(param_enum(params, "trigger_model", MODEL_WORDS)?),
        kinds,
        launch,
        first,
        path,
        move_to: (
            r0(params, "endx")?,
            r0(params, "endy")?,
            r0(params, "hspd")?,
            r0(params, "vspd")?,
        ),
        chain,
        flag_guard: param_bool(params, "flag_guard")?.unwrap_or(false),
        stopper: match param_enum(params, "stop_on_stopper", STOPPER)? {
            Some("zero_speed") => Stopper::ZeroSpeed,
            Some("flush") => Stopper::Flush,
            _ => Stopper::None,
        },
        depth_reveal: param_bool(params, "depth_reveal")?.unwrap_or(false),
        destroy_outside: param_bool(params, "destroy_outside_room")?.unwrap_or(false),
        flag: 0,
        revealed: false,
    }))
}

/// Runs one event on a trap.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Trap(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            if s.depth_reveal
                && let Some(inst) = ctx.w.get_mut(me)
            {
                inst.depth = Real::new(1_000_002.0);
            }
        }
        EventKind::Step => step(ctx, me, &s)?,
        EventKind::Alarm(n) => alarm_event(ctx, me, &s, *n)?,
        EventKind::OutsideRoom => {
            if s.destroy_outside {
                ctx.destroy(me);
            }
        }
        EventKind::Collision { with, other } if with.intersects(ClassMask::TRAP_STOPPER) => {
            stopper(ctx, me, &s, *other)?;
        }
        _ => {}
    }
    Ok(Flow::Continue)
}

fn set_flag(ctx: &mut Ctx, me: InstId, f: u8) {
    if let Some(BuiltinState::Trap(t)) = state_mut(ctx, me) {
        t.flag = f;
    }
}

fn speeds(ctx: &Ctx, me: InstId) -> (Real, Real) {
    ctx.w
        .get(me)
        .map_or((Real::ZERO, Real::ZERO), |i| (i.m.hspeed(), i.m.vspeed()))
}

fn moving(ctx: &Ctx, me: InstId) -> bool {
    let (h, v) = speeds(ctx, me);
    let nr = &ctx.rules.num;
    num::gm_ne(h, Real::ZERO, nr) || num::gm_ne(v, Real::ZERO, nr)
}

fn set_speeds(ctx: &mut Ctx, me: InstId, h: Real, v: Real) -> Result<(), SimError> {
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.m.set_hspeed(h)?;
        inst.m.set_vspeed(v)?;
    }
    Ok(())
}

fn arm(ctx: &mut Ctx, me: InstId, n: u8, v: Real) -> Result<(), SimError> {
    // gm-compare: ok (an alarm param of 0 means not armed, not a GML test)
    if v.get() > 0.0 {
        let rules = ctx.rules;
        if let Some(inst) = ctx.w.get_mut(me) {
            alarm::set(inst, n, v, &rules.num)?;
        }
    }
    Ok(())
}

fn step(ctx: &mut Ctx, me: InstId, s: &TrapState) -> Result<(), SimError> {
    if s.launch == Launch::MoveTo {
        return move_to(ctx, me, s);
    }
    let guarded = s.flag_guard || s.chain.is_some() || s.launch == Launch::Path;
    if !moving(ctx, me) && (!guarded || s.flag == 0) && trigger::fired(ctx, s.model, s.trg, &s.kinds)? {
        match s.launch {
            Launch::Velocity => set_speeds(ctx, me, s.first.0, s.first.1)?,
            Launch::Angle => {
                if let Some(inst) = ctx.w.get_mut(me) {
                    inst.m.motion_set(s.first.0, s.first.1)?;
                }
            }
            Launch::Path => arm(ctx, me, 0, Real::ONE)?,
            Launch::MoveTo => {}
        }
        if s.launch != Launch::Path {
            if let Some(chain) = &s.chain {
                arm(ctx, me, 0, chain.alarms[0])?;
                set_flag(ctx, me, 2);
            } else {
                set_flag(ctx, me, 1);
            }
        }
    }
    reveal_depth(ctx, me, s)
}

fn reveal_depth(ctx: &mut Ctx, me: InstId, s: &TrapState) -> Result<(), SimError> {
    let flag = match state_of(ctx, me) {
        Some(BuiltinState::Trap(t)) => t.flag,
        _ => 0,
    };
    if !s.depth_reveal || s.revealed || flag == 0 {
        return Ok(());
    }
    let Some(inst) = ctx.w.get(me) else {
        return Ok(());
    };
    let far = |a: Real, b: Real| num::gm_gt(num::abs(a - b), Real::new(32.0), &ctx.rules.num);
    if far(inst.m.x(), inst.m.xstart()) || far(inst.m.y(), inst.m.ystart()) {
        if let Some(inst) = ctx.w.get_mut(me) {
            inst.depth = Real::ZERO;
        }
        if let Some(BuiltinState::Trap(t)) = state_mut(ctx, me) {
            t.revealed = true;
        }
    }
    Ok(())
}

fn alarm_event(ctx: &mut Ctx, me: InstId, s: &TrapState, n: u8) -> Result<(), SimError> {
    if s.launch == Launch::Path {
        if n == 0
            && let Some(p) = &s.path
        {
            if ctx.def.paths.get(p.path.index().unwrap_or(usize::MAX)).is_none() {
                return Err(SimError::invariant("trap path does not exist"));
            }
            if let Some(inst) = ctx.w.get_mut(me) {
                let mut run = path::start(&inst.m, p.path, p.speed, p.end, p.absolute)?;
                // gm-compare: ok (a scale param of 0 means not set, not a GML test)
                if p.scale.get() != 0.0 {
                    run.scale = p.scale;
                }
                inst.path = Some(run);
            }
            set_flag(ctx, me, 1);
        }
        return Ok(());
    }
    let Some(chain) = &s.chain else {
        return Ok(());
    };
    let flag = match state_of(ctx, me) {
        Some(BuiltinState::Trap(t)) => t.flag,
        _ => return Ok(()),
    };
    let fired = trigger::fired(ctx, s.model, s.trg, &s.kinds)?;
    match n {
        0 => {
            set_speeds(ctx, me, Real::ZERO, Real::ZERO)?;
            arm(ctx, me, 1, chain.alarms[1])?;
        }
        1 => {
            let mut flag = flag;
            if !moving(ctx, me) && flag == 2 && fired {
                set_speeds(ctx, me, chain.leg2.0, chain.leg2.1)?;
                flag = 3;
            }
            if flag == 3 {
                arm(ctx, me, 2, chain.alarms[2])?;
                flag = 4;
            }
            set_flag(ctx, me, flag);
        }
        2 => {
            set_speeds(ctx, me, Real::ZERO, Real::ZERO)?;
            arm(ctx, me, 3, chain.alarms[3])?;
        }
        3 if !moving(ctx, me) && flag == 4 && fired => {
            set_speeds(ctx, me, chain.leg3.0, chain.leg3.1)?;
            set_flag(ctx, me, 5);
        }
        _ => {}
    }
    Ok(())
}

fn move_to(ctx: &mut Ctx, me: InstId, s: &TrapState) -> Result<(), SimError> {
    let (mut ex, mut ey, mut hs, mut vs) = s.move_to;
    let nr = &ctx.rules.num;
    if !(num::gm_eq(ex, Real::ZERO, nr) || num::gm_eq(ey, Real::ZERO, nr)) || !trigger::stop_set(ctx, s.trg)?
    {
        return Ok(());
    }
    let rules = ctx.rules;
    let Some(inst) = ctx.w.get_mut(me) else {
        return Ok(());
    };
    let half = |step: Real| num::abs(step) * Real::new(0.5);
    if num::gm_ne(ex, Real::ZERO, &rules.num) {
        let x = inst.m.x() + num::sign(ex - inst.m.x(), &rules.num) * hs;
        inst.m.set_x(x)?;
        if num::gm_le(num::abs(ex - x), half(hs), &rules.num) {
            inst.m.set_x(ex)?;
            hs = Real::ZERO;
            ex = Real::ONE;
        }
    }
    if num::gm_ne(ey, Real::ZERO, &rules.num) {
        let y = inst.m.y() + num::sign(ey - inst.m.y(), &rules.num) * vs;
        inst.m.set_y(y)?;
        if num::gm_le(num::abs(ey - y), half(vs), &rules.num) {
            inst.m.set_y(ey)?;
            vs = Real::ZERO;
            ey = Real::ONE;
        }
    }
    if let Some(BuiltinState::Trap(t)) = state_mut(ctx, me) {
        t.move_to = (ex, ey, hs, vs);
    }
    Ok(())
}

fn stopper(ctx: &mut Ctx, me: InstId, s: &TrapState, other: InstId) -> Result<(), SimError> {
    if s.stopper == Stopper::None {
        return Ok(());
    }
    if s.stopper == Stopper::Flush {
        flush(ctx, me, other)?;
    }
    set_speeds(ctx, me, Real::ZERO, Real::ZERO)
}

/// Snaps `me` flush to `other` on the side it moves toward, horizontal first.
fn flush(ctx: &mut Ctx, me: InstId, other: InstId) -> Result<(), SimError> {
    let (Some(a), Some(b)) = (ctx.w.get(me), ctx.w.get(other)) else {
        return Ok(());
    };
    let rules = &ctx.rules.collide;
    let (Some(ma), Some(mb)) = (
        instance_box(ctx.def, a, (a.m.x(), a.m.y()), rules)?,
        instance_box(ctx.def, b, (b.m.x(), b.m.y()), rules)?,
    ) else {
        return Ok(());
    };
    let (ba, bb) = (ma.bbox, mb.bbox);
    let (h, v) = (a.m.hspeed().get(), a.m.vspeed().get());
    let (x, y) = (a.m.x(), a.m.y());
    let (nx, ny) = if h > 0.0 {
        (x + real_of(bb.left - (ba.right + 1)), y)
    } else if h < 0.0 {
        (x + real_of((bb.right + 1) - ba.left), y)
    } else if v > 0.0 {
        (x, y + real_of(bb.top - (ba.bottom + 1)))
    } else if v < 0.0 {
        (x, y + real_of((bb.bottom + 1) - ba.top))
    } else {
        (x, y)
    };
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.m.set_x(nx)?;
        inst.m.set_y(ny)?;
    }
    Ok(())
}
