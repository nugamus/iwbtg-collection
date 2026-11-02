//! Built-in `bow` (the Medium-mode ribbon; CLONE_SPEC sprites "Bow"; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/bow.md). Tier 1. WP0 creates the file; WP6a owns it.
//!
//! Persistent, cosmetic. In the phases it handles it destroys itself unless the difficulty is 0
//! (`medium_only`, default on) and when the room's role is Select (`destroy_in_select`, default
//! on: rely, A2, A3, A5). In its `follow` phase it copies the first kid's position, scales and
//! alpha through `kid::api::bow_snapshot`; the 1-tick lag the traces show (Inferred) comes from
//! the dispatch order (the bow's rank against the kid's), not from a stored copy. `follow`:
//! `step` (rely, kaenbyou, LR), `begin_step` (wanna, A3), `end_step` (eat-the-lemon, xia, which
//! also snaps in Step). With no kid it keeps its last place. OPEN: the destroy check runs in all
//! three phases here (A3 does; the others only name Step), which differs only if the difficulty
//! changes mid-tick; the flipped kid's `y + 1` and scale handling (xia) and the lemon sprite
//! swap are not done.

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, state_of};
use crate::def::{KindDef, Params, RoomRole};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::kid::api::bow_snapshot;
use crate::world::{Ctx, Flow, InstId};

const FOLLOW: &[&str] = &["step", "begin_step", "end_step"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "follow",
        ty: ParamType::Enum(FOLLOW),
        required: false,
    },
    ParamSpec {
        name: "medium_only",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "destroy_in_select",
        ty: ParamType::Bool,
        required: false,
    },
];

/// The phase in which the bow copies the kid.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Follow {
    /// Begin Step.
    BeginStep,
    /// Step.
    Step,
    /// End Step.
    EndStep,
}

/// A bow's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BowState {
    /// Where it copies the kid.
    pub follow: Follow,
    /// Destroyed unless the difficulty is 0.
    pub medium_only: bool,
    /// Destroyed in a Select-role room.
    pub destroy_in_select: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: the three Step phases.
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet {
        begin_step: true,
        step: true,
        end_step: true,
        ..EventSet::default()
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let follow = match param_enum(params, "follow", FOLLOW)? {
        Some("begin_step") => Follow::BeginStep,
        Some("end_step") => Follow::EndStep,
        _ => Follow::Step,
    };
    Ok(BuiltinState::Bow(BowState {
        follow,
        medium_only: param_bool(params, "medium_only")?.unwrap_or(true),
        destroy_in_select: param_bool(params, "destroy_in_select")?.unwrap_or(true),
    }))
}

/// Runs one event on a bow.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let phase = match ev {
        EventKind::BeginStep => Follow::BeginStep,
        EventKind::Step => Follow::Step,
        EventKind::EndStep => Follow::EndStep,
        _ => return Ok(Flow::Continue),
    };
    let Some(BuiltinState::Bow(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    let select = ctx
        .def
        .room(ctx.w.room)
        .is_some_and(|r| r.role == RoomRole::Select);
    if (s.medium_only && ctx.w.engine.run.difficulty != 0) || (s.destroy_in_select && select) {
        return Ok(ctx.destroy(me));
    }
    if phase != s.follow {
        return Ok(Flow::Continue);
    }
    let rules = ctx.rules;
    let Some(kid) = ctx.w.kid(&rules.tick) else {
        return Ok(Flow::Continue);
    };
    let snap = bow_snapshot(ctx.w, kid);
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.m.set_x(snap.x)?;
        inst.m.set_y(snap.y)?;
        inst.img.set_xscale(snap.xscale)?;
        inst.img.set_yscale(snap.yscale)?;
        inst.img.set_alpha(snap.alpha)?;
    }
    Ok(Flow::Continue)
}
