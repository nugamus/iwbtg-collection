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
//!   `StepPressed { skip_while_paused }` skips the retry while the game is paused.
//! - **Boot-room load**: `restart::boot_step` every Step (the init object's Step).
//! - **Latch clearing** (`latch_cleared_by_world`; kaenbyou, eat-the-lemon, xia): two
//!   `keyboard_check_direct` reads, Left then Right, at the end of the Step, result discarded.
//! - **Pause** (`pause = "deactivate"`, keep-celebrating-for-xia): a Pause press in Step, gated by
//!   a delay counter, deactivates every other instance; the next press reactivates the ones it
//!   deactivated and calls `io_clear` (held keys and the latch are dropped). The other 13 games
//!   pause with a modal message box, `keyboard_wait` (showdown) or a sleep loop (kaenbyou): they
//!   block the runner, so the client freezes the sim and the world has nothing to do (`pause =
//!   "none"`, the default).
//! - **Mute** (`mute`): a Mute press flips `MechGlobals::music_muted` (the original's
//!   `global.music`, 1 is muted); music objects read it. The Ctrl in Ctrl+M (wanna, xia) is the
//!   client's binding of the action.
//! - **Play timer** (family B): `timer_per_tick` (2: centiseconds at 50 ticks) is added to
//!   `RunGlobals::time` every Step while not paused, `timer_on_restart` (2) on the retry.
//! - **Real-time reads** (R28): the world reads no clock. `tick::real_time_fps` and
//!   `real_time_ms` give the fixed values for the games that display them (kaenbyou and
//!   eat-the-lemon world Step).
//!
//! OPEN: what the pause-delay counter starts at and how it counts (the spec names a counter, not
//! its values: `pause_delay` ticks after each toggle here); the effect of kaenbyou and
//! eat-the-lemon's cheat-detection check (the spec gives the test, not what it does); whether
//! the B timer counts in menu rooms and while dead (here: every Step); the sounds a mute or an
//! unpause stops (`sound::stop_all`, WP10, which is not implemented yet).

use serde::{Deserialize, Serialize};

use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_real, state_mut, state_of,
};
use crate::def::{KindDef, ParamValue, Params, RestartTrigger};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::input::Action;
use crate::num::Real;
use crate::restart;
use crate::world::{Ctx, Flow, InstId, KindState, World};

const PAUSES: &[&str] = &["none", "deactivate"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "latch_cleared_by_world",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "pause",
        ty: ParamType::Enum(PAUSES),
        required: false,
    },
    ParamSpec {
        name: "pause_delay",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "mute",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "timer_per_tick",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "timer_on_restart",
        ty: ParamType::Real,
        required: false,
    },
];

/// The world's settings and pause state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WorldState {
    /// Discard a direct read of Left and Right at the end of each Step.
    pub latch_cleared_by_world: bool,
    /// A Pause press deactivates every other instance (xia).
    pub pause_deactivates: bool,
    /// Ticks after a pause toggle before the next one is accepted.
    pub pause_delay: u32,
    /// A Mute press flips the music flag.
    pub mute: bool,
    /// Added to the play time every Step (family B: 2).
    pub timer_per_tick: u32,
    /// Added to the play time on the retry (family B: 2).
    pub timer_on_restart: u32,
    /// The game is paused (instances deactivated).
    pub paused: bool,
    /// Ticks until a pause toggle is accepted.
    pub pause_counter: u32,
    /// The instances the pause deactivated, so unpause wakes exactly those.
    pub deactivated: Vec<InstId>,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step, Room Start and the Restart and Reset key presses
/// (which of them acts depends on the game's models), plus Mute when the kind has `mute`.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut key_press = Action::Restart.bit() | Action::Reset.bit();
    if matches!(kind.params.0.get("mute"), Some(ParamValue::Bool(true))) {
        key_press |= Action::Mute.bit();
    }
    EventSet {
        step: true,
        room_start: true,
        key_press,
        ..EventSet::default()
    }
}

fn count(params: &Params, name: &str) -> Result<u32, ParamError> {
    match param_real(params, name)? {
        None => Ok(0),
        Some(v) => super::warp::whole(v, name),
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    Ok(BuiltinState::World(WorldState {
        latch_cleared_by_world: param_bool(params, "latch_cleared_by_world")?.unwrap_or(false),
        pause_deactivates: param_enum(params, "pause", PAUSES)? == Some("deactivate"),
        pause_delay: count(params, "pause_delay")?,
        mute: param_bool(params, "mute")?.unwrap_or(false),
        timer_per_tick: count(params, "timer_per_tick")?,
        timer_on_restart: count(params, "timer_on_restart")?,
        paused: false,
        pause_counter: 0,
        deactivated: Vec::new(),
    }))
}

/// Whether the world instance `world` has the game paused (false for anything else).
pub fn is_paused(w: &World, world: InstId) -> bool {
    match w.get(world).map(|i| &i.state) {
        Some(KindState::Builtin(BuiltinState::World(s))) => s.paused,
        _ => false,
    }
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
            retry(ctx, me, &s)
        }
        EventKind::KeyPress(Action::Reset) if def.reset.trigger == RestartTrigger::KeyPressEvent => {
            restart::run_reset(ctx, me)
        }
        EventKind::KeyPress(Action::Mute) if s.mute => {
            let m = &mut ctx.w.engine.mech.music_muted;
            *m = !*m;
            Ok(Flow::Continue)
        }
        EventKind::Step => step(ctx, me, &s),
        _ => Ok(Flow::Continue),
    }
}

/// The retry: the timer's +2 first (family B, kid/B.md "R"), then the model.
fn retry(ctx: &mut Ctx, me: InstId, s: &WorldState) -> Result<Flow, SimError> {
    if s.timer_on_restart > 0 {
        ctx.w.engine.run.add_time(s.timer_on_restart)?;
    }
    restart::run_restart(ctx, me)
}

/// Whether a Step-phase trigger fires this tick for `key`.
fn step_trigger(ctx: &Ctx, trigger: RestartTrigger, key: Action, paused: bool) -> bool {
    match trigger {
        RestartTrigger::KeyPressEvent => false,
        RestartTrigger::StepHeld => ctx.w.input.held(key),
        RestartTrigger::StepPressed { skip_while_paused } => {
            !(skip_while_paused && paused) && ctx.w.input.pressed(key)
        }
    }
}

fn step(ctx: &mut Ctx, me: InstId, s: &WorldState) -> Result<Flow, SimError> {
    let def = ctx.def;
    let mut flow = restart::boot_step(ctx)?;
    if s.pause_deactivates {
        toggle_pause(ctx, me)?;
    }
    let paused = is_paused(ctx.w, me);
    if s.timer_per_tick > 0 && !paused {
        ctx.w.engine.run.add_time(s.timer_per_tick)?;
    }
    if step_trigger(ctx, def.restart.trigger, Action::Restart, paused) {
        flow = flow.combine(retry(ctx, me, s)?);
    }
    if step_trigger(ctx, def.reset.trigger, Action::Reset, paused) {
        flow = flow.combine(restart::run_reset(ctx, me)?);
    }
    if s.latch_cleared_by_world {
        // "fix player reset movement bug": two direct reads, result discarded.
        let _ = ctx.w.input.direct(Action::Left);
        let _ = ctx.w.input.direct(Action::Right);
    }
    Ok(flow)
}

/// xia's pause: a Pause press, once the delay counter has run out, deactivates every other active
/// instance or wakes the ones it deactivated (and drops held keys, `io_clear`).
fn toggle_pause(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let Some(BuiltinState::World(s)) = state_mut(ctx, me) else {
        return Ok(());
    };
    s.pause_counter = s.pause_counter.saturating_sub(1);
    let ready = s.pause_counter == 0;
    let delay = s.pause_delay;
    let was_paused = s.paused;
    let pressed = ctx.w.input.pressed(Action::Pause);
    if !(pressed && ready) {
        return Ok(());
    }
    if was_paused {
        let woken = match state_mut(ctx, me) {
            Some(BuiltinState::World(s)) => std::mem::take(&mut s.deactivated),
            _ => Vec::new(),
        };
        for id in woken {
            if let Some(inst) = ctx.w.get_mut(id) {
                inst.active = true;
            }
        }
        ctx.w.input.clear();
    } else {
        let ids: Vec<InstId> = ctx
            .w
            .iter_alive()
            .filter(|i| i.active && i.id != me)
            .map(|i| i.id)
            .collect();
        for id in &ids {
            if let Some(inst) = ctx.w.get_mut(*id) {
                inst.active = false;
            }
        }
        if let Some(BuiltinState::World(s)) = state_mut(ctx, me) {
            s.deactivated = ids;
        }
    }
    if let Some(BuiltinState::World(s)) = state_mut(ctx, me) {
        s.paused = !was_paused;
        s.pause_counter = delay;
    }
    Ok(())
}
