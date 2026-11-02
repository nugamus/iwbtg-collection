//! Built-in `item_viewer` (mechanics `item_viewer`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/item_viewer.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! Shows a collected boss reward or item: visible only while `global.boss[index]`
//! (`kind = "boss"`) or `global.item[index]` (`kind = "item"`) is set (KAE bossItemViewer, JUN
//! itemview5, LR, RBF). Set in Create and in every Step.

use super::{BuiltinState, ParamError, ParamSpec, ParamType, flag_index, param_enum, param_error, state_of};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

const KINDS: &[&str] = &["boss", "item"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "kind",
        ty: ParamType::Enum(KINDS),
        required: true,
    },
    ParamSpec {
        name: "index",
        ty: ParamType::Real,
        required: true,
    },
    ParamSpec {
        name: "sprite",
        ty: ParamType::Real,
        required: false,
    },
];

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step.
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet {
        step: true,
        ..EventSet::default()
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let boss = param_enum(params, "kind", KINDS)?
        .ok_or_else(|| param_error("kind", "an item viewer needs `kind` (boss or item)"))?
        == "boss";
    Ok(BuiltinState::ItemViewer {
        boss,
        index: flag_index(params, "index")?,
    })
}

/// Runs one event on an item viewer.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    if !matches!(ev, EventKind::Create | EventKind::Step) {
        return Ok(Flow::Continue);
    }
    let Some(BuiltinState::ItemViewer { boss, index }) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let set = if *boss {
        ctx.w.engine.mech.boss(*index)
    } else {
        ctx.w.engine.mech.item(*index)
    };
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.visible = set;
    }
    Ok(Flow::Continue)
}
