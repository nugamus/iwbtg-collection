//! Built-in `blood_emitter` (kid specs "Death"; CLONE_SPEC sprites BLOOD-COUNT and numbers hidden
//! consumers; docs/architecture/sim/builtins.md; docs/format/builtins/blood_emitter.md). Tier 1. WP0
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
//! - `particles` (family B, kaenbyou, eat-the-lemon): a particle stream. The client draws it
//!   (`BuiltinOutput::DeathParticles` each Step). With `NumRules::particles_draw_rng` (N10, on in
//!   engine_rev 0, #766) Create also builds its system in `World::particles`, as the original's
//!   Create does: one type (size, speed 1 to 5 and direction 0 to 360 ranges, life 50: 4 draws),
//!   one emitter on a linear rectangle (2 draws) streaming `per_step` (15), so each tick's particle update draws
//!   `per_step` x 6 (90), from the creation tick on (kid/B.md Death; CLONE_SPEC numbers
//!   "Particle draw rules"; Confirmed (recorder) kaenbyou `num-part-rng-3-k1.csv`, frames 45-50
//!   replayed bit for bit). The original removes the emitter when alarm 0 fires and destroys the
//!   system on Restart and Room End (item 8); here the instance goes at alarm 0 and its Destroy
//!   destroys the system, which draws the same: the type has no step or death particles, so the
//!   live particles left over draw nothing. Room End destroys it too. The Step draws nothing.
//!
//! `destroyed_by_restart` (kaenbyou, eat-the-lemon): a Restart key press destroys the emitter.
//!
//! `outlives_stream` (family B, `particles` mode only): Alarm 0 only ends the stream, as the
//! original's Alarm 0 (`part_emitter_destroy`) does; the instance stays until Restart, Room End
//! or a room change (kid/B.md Death step 2; trace famb-death-timing-b: `instance_number
//! (bloodEmitter)` 1 from frame 12 to 80; #877). The emitter in `World::particles` is destroyed,
//! so the 90 draws a tick stop; the system and its live particles stay until Destroy or Room End.
//! The ended stream is kept as `per_step` 0 (refused at load in this mode).

use serde::{Deserialize, Serialize};

use super::{
    BuiltinOutput, BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_error,
    param_real, state_mut, state_of,
};
use crate::alarm;
use crate::def::{EngineKind, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::input::Action;
use crate::num::{self, Real};
use crate::output::OutputEvent;
use crate::particles::{PartRange, PartSystemId, PartType, PartTypeId, Region, RegionDistr, RegionShape};
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
    ParamSpec {
        name: "outlives_stream",
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
    /// Blood per Step (`random_dir`), particles per Step (`particles`). In `particles` mode 0
    /// means alarm 0 ended the stream (`outlives_stream`); the loader refuses 0 there.
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
    /// `particles`: alarm 0 ends the stream, not the instance.
    pub outlives_stream: bool,
    /// `particles` with `NumRules::particles_draw_rng`: the system Create built, until Destroy or
    /// Room End destroys it.
    pub system: Option<PartSystemId>,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: alarm 0 and Step, plus the Restart key press when asked,
/// plus Room End in `particles` mode (it destroys the system).
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        step: true,
        alarms: 1,
        room_end: matches!(kind.params.0.get("mode"), Some(ParamValue::Str(m)) if m == "particles"),
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
    let per_step = count(params, "per_step", if mode == Mode::Particles { 15 } else { 40 })?;
    if mode == Mode::Particles && (per_step == 0 || i32::try_from(per_step).is_err()) {
        // A stream of 0 is not traced (N10); the model refuses it, so the loader does too.
        return Err(param_error(
            "per_step",
            "particles mode needs a per_step from 1 to 2147483647",
        ));
    }
    let outlives_stream = param_bool(params, "outlives_stream")?.unwrap_or(false);
    if outlives_stream && mode != Mode::Particles {
        // Only family B's particle stream outlives its alarm (kid/B.md Death); no traced game
        // keeps a `random_dir` or `angle_sweep` emitter alive.
        return Err(param_error(
            "outlives_stream",
            "only particles mode can outlive its stream",
        ));
    }
    Ok(BuiltinState::BloodEmitter(BloodEmitterState {
        mode,
        life: param_real(params, "life")?.unwrap_or(Real::new(20.0)),
        per_step,
        speed_max: param_real(params, "speed_max")?.unwrap_or(Real::new(6.0)),
        chance: param_real(params, "chance")?.unwrap_or(Real::new(0.2)),
        per_angle: count(params, "per_angle", 8)?,
        flipped: param_bool(params, "flipped")?.unwrap_or(false),
        destroyed_by_restart: param_bool(params, "destroyed_by_restart")?.unwrap_or(false),
        outlives_stream,
        system: None,
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
            if s.mode == Mode::Particles && ctx.rules.num.particles_draw_rng {
                let sys = build_system(ctx, s.per_step)?;
                if let Some(BuiltinState::BloodEmitter(st)) = state_mut(ctx, me) {
                    st.system = Some(sys);
                }
            }
            Ok(Flow::Continue)
        }
        EventKind::Alarm(0) if s.outlives_stream => {
            end_stream(ctx, me, &s)?;
            Ok(Flow::Continue)
        }
        EventKind::Alarm(0) => Ok(ctx.destroy(me)),
        EventKind::KeyPress(Action::Restart) if s.destroyed_by_restart => Ok(ctx.destroy(me)),
        EventKind::Destroy | EventKind::RoomEnd => {
            if let Some(sys) = s.system {
                ctx.w.particles.destroy_system(sys)?;
                if let Some(BuiltinState::BloodEmitter(st)) = state_mut(ctx, me) {
                    st.system = None;
                }
            }
            Ok(Flow::Continue)
        }
        EventKind::Step => step(ctx, me, &s),
        _ => Ok(Flow::Continue),
    }
}

/// Alarm 0 with `outlives_stream`: `part_emitter_destroy` on the one emitter (its system keeps
/// its live particles, which draw nothing more), and the Step stops sending the client its
/// particles. Draws nothing.
fn end_stream(ctx: &mut Ctx, me: InstId, s: &BloodEmitterState) -> Result<(), SimError> {
    if let Some(sys) = s.system {
        ctx.w.particles.destroy_all_emitters(sys)?;
    }
    if let Some(BuiltinState::BloodEmitter(st)) = state_mut(ctx, me) {
        st.per_step = 0;
    }
    Ok(())
}

fn step(ctx: &mut Ctx, me: InstId, s: &BloodEmitterState) -> Result<Flow, SimError> {
    let Some(inst) = ctx.w.get(me) else {
        return Ok(Flow::Continue);
    };
    let (x, y) = (inst.m.x(), inst.m.y());
    match s.mode {
        Mode::Particles if s.per_step == 0 => {} // alarm 0 ended the stream (`outlives_stream`)
        Mode::Particles => {
            // The stream draws in the tick's particle update (`World::particles`), not here.
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

/// The blood particle type. The spec gives its draws, not every bound: size is a non-empty range
/// (one draw; the sim draws no particles, so the bounds are not needed), speed 1 to 5, direction
/// 0 to 360, life 50 (kid/B.md Death; NUM-PART-RNG-3: "size, speed and direction ranges: 6 draws
/// a particle", 2 of them the region's).
fn blood_type() -> PartType {
    PartType {
        size: PartRange::new(0.0, 1.0),
        speed: PartRange::new(1.0, 5.0),
        direction: PartRange::new(0.0, 360.0),
        life: PartRange::fixed(50.0),
        ..PartType::default()
    }
}

/// Builds the emitter's system: the blood type (one type shared by every emitter, as type ids
/// change no draws), a new system after the others, one emitter on a linear rectangle streaming
/// `per_step`. Draws nothing.
fn build_system(ctx: &mut Ctx, per_step: u32) -> Result<PartSystemId, SimError> {
    let p = &mut ctx.w.particles;
    let want = blood_type();
    let ty = match p.types.iter().position(|t| *t == want) {
        Some(i) => PartTypeId(u32::try_from(i).map_err(|_| SimError::invariant("too many particle types"))?),
        None => p.add_type(want),
    };
    let sys = p.add_system();
    let region = Region {
        shape: RegionShape::Rectangle,
        distr: RegionDistr::Linear,
    };
    let e = p.add_emitter(sys, region)?;
    let n = i32::try_from(per_step).map_err(|_| SimError::invariant("blood_emitter: per_step too large"))?;
    p.stream(e, ty, n)?;
    Ok(sys)
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
