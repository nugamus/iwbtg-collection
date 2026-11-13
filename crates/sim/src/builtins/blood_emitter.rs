//! Built-in `blood_emitter` (kid specs "Death"; CLONE_SPEC sprites BLOOD-COUNT and numbers hidden
//! consumers; docs/plans/sim-core.md 6.1; docs/format/builtins/blood_emitter.md). Tier 1. WP0
//! creates the file; WP6a owns it.
//!
//! **Alarm plus Step**, not a hard-coded count: Create sets alarm 0 to `life` (20), Alarm 0
//! destroys the emitter, and each Step spawns the blood. How many Steps run (19 or 20) follows
//! from the alarm phase coming before Step and from when the emitter was created: made in a
//! collision event, its first Step is the next tick and it lives 19 Steps (760 blood); made in a
//! Key press event, its Step runs on the creation tick and it lives 20 (800). Confirmed
//! (recorder) rely `num-rng-a.csv` and `t6-input-key-event-order-b.csv`.
//!
//! Modes:
//! - `random_dir` (rely and most games): `per_step` blood, each a spawn (the blood's own Create
//!   draws the frame, then the gravity), then the emitter draws the direction `irandom(35) * 10`
//!   and the speed `random(speed_max)` and sets them. Confirmed order (rely `num-rng-b.csv`). OPEN:
//!   the original's `random(8)` branch (CLONE_SPEC numbers) is not named, so the speed cap is a
//!   param.
//! - `angle_sweep` (A2 Animus, A3 wanna): for each angle 0, 10, ..., 360, with chance `chance`
//!   (0.2), `per_angle` blood in that direction with speed `random(speed_max)`. OPEN: the order of
//!   the chance draw, the blood's Create and the speed draw are not in the spec; this draws the
//!   chance first, then per blood the spawn and then the speed.
//! - `particles` (family B, kaenbyou, eat-the-lemon): a particle stream. Client output only
//!   (`BuiltinOutput::DeathParticles` each Step). Particle systems draw from the gameplay RNG
//!   (N10, `crate::particles`), but this mode does not create one yet: with
//!   `NumRules::particles_draw_rng` set it is `Unsupported` (#760).
//!
//! `destroyed_by_restart` (kaenbyou, eat-the-lemon): a Restart key press destroys the emitter.

use serde::{Deserialize, Serialize};

use super::{
    BuiltinOutput, BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_real,
    state_of,
};
use crate::alarm;
use crate::def::{EngineKind, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::input::Action;
use crate::num::{self, Real};
use crate::output::OutputEvent;
use crate::world::{Ctx, Flow, InstId, SpawnInit};

const MODES: &[&str] = &["random_dir", "angle_sweep", "particles"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "mode",
        ty: ParamType::Enum(MODES),
        required: false,
    },
    ParamSpec {
        name: "life",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "per_step",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "speed_max",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "chance",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "per_angle",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "flipped",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "destroyed_by_restart",
        ty: ParamType::Bool,
        required: false,
    },
];

/// How the emitter makes blood.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Mode {
    /// A random multiple of 10 degrees per drop.
    RandomDir,
    /// A sweep over the angles with a chance each.
    AngleSweep,
    /// A client-side particle stream.
    Particles,
}

/// An emitter's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BloodEmitterState {
    /// How it makes blood.
    pub mode: Mode,
    /// Alarm 0 at Create.
    pub life: Real,
    /// Blood per Step (`random_dir`), particles per Step (`particles`).
    pub per_step: u32,
    /// The speed is `random(speed_max)`.
    pub speed_max: Real,
    /// `angle_sweep`: chance per angle.
    pub chance: Real,
    /// `angle_sweep`: blood per chosen angle.
    pub per_angle: u32,
    /// The kid was flipped: blood gravity is negative.
    pub flipped: bool,
    /// A Restart key press destroys it.
    pub destroyed_by_restart: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: alarm 0 and Step, plus the Restart key press when asked.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        step: true,
        alarms: 1,
        ..EventSet::default()
    };
    if matches!(
        kind.params.0.get("destroyed_by_restart"),
        Some(ParamValue::Bool(true))
    ) {
        set.key_press |= Action::Restart.bit();
    }
    set
}

fn count(params: &Params, name: &str, default: u32) -> Result<u32, ParamError> {
    match param_real(params, name)? {
        None => Ok(default),
        Some(v) => super::warp::whole(v, name),
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let mode = match param_enum(params, "mode", MODES)? {
        Some("angle_sweep") => Mode::AngleSweep,
        Some("particles") => Mode::Particles,
        _ => Mode::RandomDir,
    };
    Ok(BuiltinState::BloodEmitter(BloodEmitterState {
        mode,
        life: param_real(params, "life")?.unwrap_or(Real::new(20.0)),
        per_step: count(params, "per_step", if mode == Mode::Particles { 15 } else { 40 })?,
        speed_max: param_real(params, "speed_max")?.unwrap_or(Real::new(6.0)),
        chance: param_real(params, "chance")?.unwrap_or(Real::new(0.2)),
        per_angle: count(params, "per_angle", 8)?,
        flipped: param_bool(params, "flipped")?.unwrap_or(false),
        destroyed_by_restart: param_bool(params, "destroyed_by_restart")?.unwrap_or(false),
    }))
}

/// Runs one event on an emitter.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::BloodEmitter(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            let rules = &ctx.rules.num;
            if let Some(inst) = ctx.w.get_mut(me) {
                alarm::set(inst, 0, s.life, rules)?;
            }
            Ok(Flow::Continue)
        }
        EventKind::Alarm(0) => Ok(ctx.destroy(me)),
        EventKind::KeyPress(Action::Restart) if s.destroyed_by_restart => Ok(ctx.destroy(me)),
        EventKind::Step => step(ctx, me, &s),
        _ => Ok(Flow::Continue),
    }
}

fn step(ctx: &mut Ctx, me: InstId, s: &BloodEmitterState) -> Result<Flow, SimError> {
    let Some(inst) = ctx.w.get(me) else {
        return Ok(Flow::Continue);
    };
    let (x, y) = (inst.m.x(), inst.m.y());
    match s.mode {
        Mode::Particles => {
            if ctx.rules.num.particles_draw_rng {
                return Err(SimError::unsupported(
                    "blood_emitter particles with particles_draw_rng: not wired to sim::particles yet (N10, #760)",
                ));
            }
            ctx.emit(OutputEvent::Builtin(BuiltinOutput::DeathParticles {
                x,
                y,
                count: s.per_step,
                flipped: s.flipped,
            }));
        }
        Mode::RandomDir => {
            for _ in 0..s.per_step {
                let id = spawn_blood(ctx, x, y, s.flipped)?;
                let dir = ctx.w.rng.irandom(Real::new(35.0))? * Real::new(10.0);
                let speed = ctx.w.rng.random(s.speed_max);
                launch(ctx, id, dir, speed)?;
            }
        }
        Mode::AngleSweep => {
            for step in 0..=36u32 {
                let roll = ctx.w.rng.random(Real::ONE);
                if num::gm_ge(roll, s.chance, &ctx.rules.num) {
                    continue;
                }
                let dir = Real::new(f64::from(step) * 10.0);
                for _ in 0..s.per_angle {
                    let id = spawn_blood(ctx, x, y, s.flipped)?;
                    let speed = ctx.w.rng.random(s.speed_max);
                    launch(ctx, id, dir, speed)?;
                }
            }
        }
    }
    Ok(Flow::Continue)
}

/// Creates one blood drop; its Create makes its own draws.
fn spawn_blood(ctx: &mut Ctx, x: Real, y: Real, flipped: bool) -> Result<InstId, SimError> {
    let mut params = Params::default();
    params.0.insert("flipped".to_owned(), ParamValue::Bool(flipped));
    let init = SpawnInit {
        params,
        ..SpawnInit::default()
    };
    ctx.spawn_engine(EngineKind::Blood, x, y, init)?
        .ok_or_else(|| SimError::invariant("blood_emitter: the game has no blood kind"))
}

/// Sets a drop's direction and speed (`direction = d; speed = s`).
fn launch(ctx: &mut Ctx, id: InstId, dir: Real, speed: Real) -> Result<(), SimError> {
    if let Some(inst) = ctx.w.get_mut(id) {
        inst.m.motion_set(dir, speed)?;
    }
    Ok(())
}
