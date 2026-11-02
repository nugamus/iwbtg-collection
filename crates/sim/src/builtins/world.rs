//! Built-in `world`: the engine controller (docs/plans/sim-core.md 6.1; docs/format/builtins/world.md).
//! Tier 1. WP0 creates the file; WP6a owns it.
//!
//! Not a GM port: the engine services the infra objects did, at the rank of the original world
//! object. This file does:
//!
//! - **Create**: slot 1, deaths 0, time 0 (little-runner world Create; harmless elsewhere, the
//!   load restores the counters).
//! - **Room Start**: `room_speed` becomes the game's tick rate (50; Confirmed (recorder) rely
//!   `t-room-1.csv`: the kid's Create sees the file's 30, then world's Room Start sets 50, R14).
//! - **Retry and reset**: `restart::run_restart` / `run_reset` in the phase the game's models
//!   name: the Key press event (`KeyPressEvent`) or the Step (`StepHeld`, `StepPressed`).
//! - **Boot-room load**: `restart::boot_step` every Step (the init object's Step).
//! - **Latch clearing** (`latch_cleared_by_world`; kaenbyou, eat-the-lemon, xia): two
//!   `keyboard_check_direct` reads, Left then Right, at the end of the Step, result discarded.
//!
//! OPEN, not done here (filed as a follow-up): pause (P; xia deactivation, io_clear), the mute
//! flag (Ctrl+M), the family B timer, fixed real-time reads (R28). They need the pause state,
//! the sound model (WP10) and the B kid fields.

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, state_of};
use crate::def::{KindDef, Params, RestartTrigger};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::input::Action;
use crate::num::Real;
use crate::restart;
use crate::world::{Ctx, Flow, InstId};

const SPECS: &[ParamSpec] = &[ParamSpec {
    name: "latch_cleared_by_world",
    ty: ParamType::Bool,
    required: false,
}];

/// The world's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WorldState {
    /// Discard a direct read of Left and Right at the end of each Step.
    pub latch_cleared_by_world: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step, Room Start and the Restart and Reset key presses.
/// Which of them acts depends on the game's models, so all are declared.
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet {
        step: true,
        room_start: true,
        key_press: Action::Restart.bit() | Action::Reset.bit(),
        ..EventSet::default()
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    Ok(BuiltinState::World(WorldState {
        latch_cleared_by_world: param_bool(params, "latch_cleared_by_world")?.unwrap_or(false),
    }))
}

/// Runs one event on the world.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::World(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    let def = ctx.def;
    match ev {
        EventKind::Create => {
            let run = &mut ctx.w.engine.run;
            run.slot = 1;
            run.deaths = 0;
            run.time = 0;
            Ok(Flow::Continue)
        }
        EventKind::RoomStart => {
            ctx.w.room_speed = Real::new(f64::from(def.tick_rate));
            Ok(Flow::Continue)
        }
        EventKind::KeyPress(Action::Restart) if def.restart.trigger == RestartTrigger::KeyPressEvent => {
            restart::run_restart(ctx, me)
        }
        EventKind::KeyPress(Action::Reset) if def.reset.trigger == RestartTrigger::KeyPressEvent => {
            restart::run_reset(ctx, me)
        }
        EventKind::Step => step(ctx, me, &s),
        _ => Ok(Flow::Continue),
    }
}

/// Whether a Step-phase trigger fires this tick for `key`.
fn step_trigger(ctx: &Ctx, trigger: RestartTrigger, key: Action) -> bool {
    match trigger {
        RestartTrigger::KeyPressEvent => false,
        RestartTrigger::StepHeld => ctx.w.input.held(key),
        // No pause state yet, so `skip_while_paused` has nothing to skip (see the OPEN list).
        RestartTrigger::StepPressed { .. } => ctx.w.input.pressed(key),
    }
}

fn step(ctx: &mut Ctx, me: InstId, s: &WorldState) -> Result<Flow, SimError> {
    let def = ctx.def;
    let mut flow = restart::boot_step(ctx)?;
    if step_trigger(ctx, def.restart.trigger, Action::Restart) {
        flow = flow.combine(restart::run_restart(ctx, me)?);
    }
    if step_trigger(ctx, def.reset.trigger, Action::Reset) {
        flow = flow.combine(restart::run_reset(ctx, me)?);
    }
    if s.latch_cleared_by_world {
        // "fix player reset movement bug": two direct reads, result discarded.
        let _ = ctx.w.input.direct(Action::Left);
        let _ = ctx.w.input.direct(Action::Right);
    }
    Ok(flow)
}
