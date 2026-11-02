//! Built-in `boss_gate` (mechanics `boss_gate`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/boss_gate.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! A solid block that destroys itself once `global.boss[boss_index]` is set (KOT bossblock1,
//! KAE bossBlock, LR blockFlagBoss). It checks in Create and in every Step (the original only
//! checks in Step for some games; the Create check is the same test one Step earlier, KAE).
//! `boss_index` is 0 to 15, a bit of `MechGlobals::bosses`. The sprite and solidity are the
//! loader's.

use super::{BuiltinState, ParamError, ParamSpec, ParamType, flag_index};
use crate::def::{KindDef, Params};
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

/// A new instance's state from its params: the boss index.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    Ok(BuiltinState::Flag(flag_index(params, "boss_index")?))
}

/// Runs one event on a boss gate.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    if !matches!(ev, EventKind::Create | EventKind::Step) {
        return Ok(Flow::Continue);
    }
    let Some(BuiltinState::Flag(i)) = super::state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    if ctx.w.engine.mech.boss(*i) {
        ctx.destroy(me);
    }
    Ok(Flow::Continue)
}
