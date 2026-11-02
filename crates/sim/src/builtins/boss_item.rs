//! Built-in `boss_item` (mechanics `boss_item`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/boss_item.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! The reward of a boss (KOT bossitem1, MIKU, XIA objBossItem). Create makes it invisible. Each
//! Step, in this order: it becomes visible when no instance of `watched_object` (the boss kind)
//! exists any more (with no `watched_object` that is the first Step); it destroys itself when
//! `global.boss[boss_index]` is already set. A kid touch, only while it is visible, sets
//! `global.boss[boss_index]` and destroys it (the sound is the client's).

use super::warp::KIDS;
use super::{BuiltinState, ParamError, ParamSpec, ParamType, flag_index, param_kind, state_of};
use crate::def::{KindDef, KindId, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "boss_index",
        ty: ParamType::Real,
        required: true,
    },
    ParamSpec {
        name: "watched_object",
        ty: ParamType::Kind,
        required: false,
    },
    ParamSpec {
        name: "sprite",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "sound",
        ty: ParamType::Sound,
        required: false,
    },
];

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step and the kid touching it.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        step: true,
        ..EventSet::default()
    };
    set.collisions.push(KIDS);
    set
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    Ok(BuiltinState::BossItem {
        index: flag_index(params, "boss_index")?,
        watched: param_kind(params, "watched_object")?,
    })
}

/// Runs one event on a boss item.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::BossItem { index, watched }) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let (index, watched): (u8, Option<KindId>) = (*index, *watched);
    match ev {
        EventKind::Create => {
            if let Some(inst) = ctx.w.get_mut(me) {
                inst.visible = false;
            }
        }
        EventKind::Step => {
            let boss_gone = watched.is_none_or(|k| !ctx.w.iter_alive().any(|i| i.kind == k));
            if boss_gone && let Some(inst) = ctx.w.get_mut(me) {
                inst.visible = true;
            }
            if ctx.w.engine.mech.boss(index) {
                ctx.destroy(me);
            }
        }
        EventKind::Collision { with, .. }
            if with.intersects(KIDS) && ctx.w.get(me).is_some_and(|i| i.visible) =>
        {
            ctx.w.engine.mech.set_boss(index);
            ctx.destroy(me);
        }
        _ => {}
    }
    Ok(Flow::Continue)
}
