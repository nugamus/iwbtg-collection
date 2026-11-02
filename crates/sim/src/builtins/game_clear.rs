//! Built-in `game_clear` (mechanics `game_clear`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/game_clear.md): raises the `run_end` timing trigger. Tier 2. WP0 creates
//! the file; WP6b owns it.
//!
//! A touch by either kid sets `global.clear` and runs the game's saveGame script, then raises
//! `run_end` (timing row 16). The save is `save::save_now` from the touching kid with the game's
//! position model (`saved_position`, `fixed_x`, `fixed_y` as the `save` built-in). With
//! `save_with_position = false` it writes only the counters and the clear flag into the existing
//! save slot, or nothing when there is no slot.
//!
//! OPEN: which of the two the nine games do (`save_with_position`, mechanics.md): the param is
//! the converter's to set per game.

use super::save::{POSITION, model_of};
use super::warp::KIDS;
use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_error, param_real, state_of,
};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "save_with_position",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "saved_position",
        ty: ParamType::Enum(POSITION),
        required: false,
    },
    ParamSpec {
        name: "fixed_x",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "fixed_y",
        ty: ParamType::Real,
        required: false,
    },
];

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

/// A game clear's settings.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct GameClearState {
    /// Save at the kid position (else counters and the clear flag only).
    pub with_position: bool,
    /// Position model by name (as `save`).
    pub position: String,
    /// The fixed point for `fixed_point`.
    pub fixed: Option<(Real, Real)>,
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let position = param_enum(params, "saved_position", POSITION)?.unwrap_or("xy_base100");
    let fixed = match (param_real(params, "fixed_x")?, param_real(params, "fixed_y")?) {
        (Some(x), Some(y)) => Some((x, y)),
        (None, None) => None,
        _ => return Err(param_error("fixed_x", "give fixed_x and fixed_y together")),
    };
    if position == "fixed_point" && fixed.is_none() {
        return Err(param_error("fixed_x", "fixed_point needs fixed_x and fixed_y"));
    }
    Ok(BuiltinState::GameClear(GameClearState {
        with_position: param_bool(params, "save_with_position")?.unwrap_or(true),
        position: position.to_owned(),
        fixed,
    }))
}

/// Runs one event on a game clear.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let EventKind::Collision { with, other } = ev else {
        return Ok(Flow::Continue);
    };
    if !with.intersects(KIDS) {
        return Ok(Flow::Continue);
    }
    let Some(BuiltinState::GameClear(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    ctx.w.engine.run.clear = true;
    if s.with_position {
        crate::save::save_now(ctx, *other, model_of(&s.position), s.fixed)?;
    } else {
        crate::save::save_counters(ctx)?;
        if let Some(slot) = ctx.w.save.as_mut() {
            slot.clear = true;
        }
    }
    crate::timing::raise_run_end(ctx.w);
    Ok(Flow::Continue)
}
