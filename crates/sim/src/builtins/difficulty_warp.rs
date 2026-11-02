//! Built-in `difficulty_warp` (warpStart, objWarpStart; rooms; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/difficulty_warp.md). Tier 1. WP0 creates the file; WP6a owns it.
//!
//! Collision with the kid: set the difficulty from `dif`, delete the save, zero the death and
//! time counters (the original writes 0 into the slot's death and time entries; the new kid's
//! Create resets them again because the save is gone), destroy the kid and the bow, and go to
//! the new-game room (`StartPoint::NewGame` does the same from the client, CR2). Source: kid/A1.md
//! "Saving", "New game".
//!
//! OPEN: eat-the-new-year picks the difficulty by the kid's x in selectroom; machinary never
//! assigns one; kaenbyou's warpStart does `room_restart` when no save exists.

use serde::{Deserialize, Serialize};

use super::warp::{KIDS, destroy_kid_and_bow, emit_transition, whole};
use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_real, state_of};
use crate::def::{KindDef, ParamValue, Params, RoomId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::room::RoomRequest;
use crate::world::{Ctx, Flow, InstId};

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "dif",
        ty: ParamType::Real,
        required: true,
    },
    ParamSpec {
        name: "room_to",
        ty: ParamType::Room,
        required: false,
    },
    ParamSpec {
        name: "transition",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "transition_steps",
        ty: ParamType::Real,
        required: false,
    },
];

/// A difficulty warp's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DifficultyWarpState {
    /// The difficulty it sets.
    pub dif: u8,
    /// Where it goes; the game's new-game room when absent.
    pub room_to: Option<RoomId>,
    /// `transition_kind`.
    pub transition: u8,
    /// `transition_steps`.
    pub transition_steps: u32,
}

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

fn narrow(v: u32, name: &str) -> Result<u8, ParamError> {
    u8::try_from(v).map_err(|_| super::param_error(name, "must be 0 to 255"))
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let dif =
        param_real(params, "dif")?.ok_or_else(|| super::param_error("dif", "the difficulty is required"))?;
    let zero = Real::ZERO;
    Ok(BuiltinState::DifficultyWarp(DifficultyWarpState {
        dif: narrow(whole(dif, "dif")?, "dif")?,
        room_to: match params.0.get("room_to") {
            Some(ParamValue::Room(r)) => Some(*r),
            None => None,
            Some(_) => return Err(super::param_error("room_to", "must name a room")),
        },
        transition: narrow(
            whole(param_real(params, "transition")?.unwrap_or(zero), "transition")?,
            "transition",
        )?,
        transition_steps: whole(
            param_real(params, "transition_steps")?.unwrap_or(zero),
            "transition_steps",
        )?,
    }))
}

/// Runs one event on a difficulty warp.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::DifficultyWarp(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Collision { with, .. } if with.intersects(KIDS) => {
            let run = &mut ctx.w.engine.run;
            run.difficulty = s.dif;
            run.deaths = 0;
            run.time = 0;
            ctx.w.save = None;
            emit_transition(ctx, s.transition, s.transition_steps);
            let to = s.room_to.unwrap_or(ctx.def.new_game_room);
            ctx.request_room(RoomRequest::Goto(to));
            destroy_kid_and_bow(ctx)?;
            Ok(Flow::AbortTick)
        }
        _ => Ok(Flow::Continue),
    }
}
