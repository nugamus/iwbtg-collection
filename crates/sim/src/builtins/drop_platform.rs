//! Built-in `drop_platform` (mechanics `drop_platform`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/drop_platform.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! A platform that falls or rises when the kid stands on it. The kid is the first kid in the
//! room. `mode`:
//!
//! - `fall` (KOT blockFall): each Step, when the kid is met at `(x, y - kid.vspeed - 1)`, the
//!   platform's `vspeed` becomes `speed` (2).
//! - `rise` (KOT blockRise, LR with `speed`): under the same test `yspeed = -speed` and the kid's
//!   `y` moves by `-speed`. Then every Step `y += yspeed`, so it keeps rising after the kid
//!   leaves.
//! - `rise_kae` (KAE blockRise, a solid block): Step: when `yspeed` is 0, the kid is falling
//!   (`vspeed > 0`) and met at `(x, y + vsp - 3)`, `yspeed = vsp` (the kid's vspeed). End Step:
//!   when the kid is met at `(x, y + vsp - 3)`, the kid's `y += yspeed`; then the block's
//!   `y += yspeed`.
//!
//! `solid` is applied in Create when given. OPEN, not done (nugamus/iwbtg-collection-research#52): the flipped kid (the specs test one
//! kid object only).

use serde::{Deserialize, Serialize};

use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_real, state_mut, state_of,
};
use crate::collide::Target;
use crate::collide::query::place_meeting;
use crate::def::{KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::{self, Real};
use crate::world::{Ctx, Flow, InstId};

const MODES: &[&str] = &["fall", "rise", "rise_kae"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "mode",
        ty: ParamType::Enum(MODES),
        required: false,
    },
    ParamSpec {
        name: "speed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "solid",
        ty: ParamType::Bool,
        required: false,
    },
];

/// What the platform does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Mode {
    /// Falls when stood on.
    Fall,
    /// Rises when stood on.
    Rise,
    /// KAE's solid rising block.
    RiseKae,
}

/// A drop platform's settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DropState {
    /// The mode.
    pub mode: Mode,
    /// Speed (default 2).
    pub speed: Real,
    /// `solid` override.
    pub solid: Option<bool>,
    /// The hand-applied vertical speed.
    pub yspeed: Real,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step, plus End Step for `rise_kae`.
pub fn event_set(kind: &KindDef) -> EventSet {
    EventSet {
        step: true,
        end_step: matches!(kind.params.0.get("mode"), Some(ParamValue::Str(w)) if w == "rise_kae"),
        ..EventSet::default()
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    Ok(BuiltinState::DropPlatform(DropState {
        mode: match param_enum(params, "mode", MODES)? {
            Some("rise") => Mode::Rise,
            Some("rise_kae") => Mode::RiseKae,
            _ => Mode::Fall,
        },
        speed: param_real(params, "speed")?.unwrap_or(Real::new(2.0)),
        solid: param_bool(params, "solid")?,
        yspeed: Real::ZERO,
    }))
}

/// Runs one event on a drop platform.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::DropPlatform(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            if let (Some(v), Some(inst)) = (s.solid, ctx.w.get_mut(me)) {
                inst.solid = v;
            }
        }
        EventKind::Step => step(ctx, me, &s)?,
        EventKind::EndStep if s.mode == Mode::RiseKae => end_step(ctx, me, &s)?,
        _ => {}
    }
    Ok(Flow::Continue)
}

/// The first kid and its `vspeed`.
fn kid_of(ctx: &Ctx) -> Option<(InstId, Real)> {
    let id = ctx.w.kid(&ctx.rules.tick)?;
    Some((id, ctx.w.get(id)?.m.vspeed()))
}

fn move_y(ctx: &mut Ctx, id: InstId, dy: Real) -> Result<(), SimError> {
    if let Some(inst) = ctx.w.get_mut(id) {
        let y = inst.m.y() + dy;
        inst.m.set_y(y)?;
    }
    Ok(())
}

fn set_yspeed(ctx: &mut Ctx, me: InstId, v: Real) {
    if let Some(BuiltinState::DropPlatform(s)) = state_mut(ctx, me) {
        s.yspeed = v;
    }
}

fn step(ctx: &mut Ctx, me: InstId, s: &DropState) -> Result<(), SimError> {
    let Some(inst) = ctx.w.get(me) else {
        return Ok(());
    };
    let (x, y) = (inst.m.x(), inst.m.y());
    let kid = kid_of(ctx);
    match s.mode {
        Mode::Fall => {
            if let Some((k, vs)) = kid
                && place_meeting(ctx, me, x, y - vs - Real::ONE, Target::Inst(k))?
                && let Some(inst) = ctx.w.get_mut(me)
            {
                inst.m.set_vspeed(s.speed)?;
            }
        }
        Mode::Rise => {
            if let Some((k, vs)) = kid
                && place_meeting(ctx, me, x, y - vs - Real::ONE, Target::Inst(k))?
            {
                set_yspeed(ctx, me, -s.speed);
                move_y(ctx, k, -s.speed)?;
                move_y(ctx, me, -s.speed)?;
            } else {
                move_y(ctx, me, s.yspeed)?;
            }
        }
        Mode::RiseKae => {
            if num::gm_eq(s.yspeed, Real::ZERO, &ctx.rules.num)
                && let Some((k, vs)) = kid
                && num::gm_gt(vs, Real::ZERO, &ctx.rules.num)
                && place_meeting(ctx, me, x, y + vs - Real::new(3.0), Target::Inst(k))?
            {
                set_yspeed(ctx, me, vs);
            }
        }
    }
    Ok(())
}

fn end_step(ctx: &mut Ctx, me: InstId, s: &DropState) -> Result<(), SimError> {
    let Some(inst) = ctx.w.get(me) else {
        return Ok(());
    };
    let (x, y) = (inst.m.x(), inst.m.y());
    // Read after Step: the yspeed it may just have set.
    let ys = match state_of(ctx, me) {
        Some(BuiltinState::DropPlatform(d)) => d.yspeed,
        _ => s.yspeed,
    };
    if let Some((k, vs)) = kid_of(ctx)
        && place_meeting(ctx, me, x, y + vs - Real::new(3.0), Target::Inst(k))?
    {
        move_y(ctx, k, ys)?;
    }
    move_y(ctx, me, ys)
}
