//! Built-in `particle_system` (docs/format/builtins/particle_system.md; N10, #765).
//!
//! An object whose code builds a particle system (rely objEffect, keep-celebrating objcpart,
//! go-the-spook-house oRainSys, eat-the-lemon docobj2, docobj3, object131, object132). Create
//! builds its `particles/<id>.toml` system in `World::particles` (`Particles::build`); a list of
//! files (rely objEffect's two systems, client#826) builds each in list order, so the draw order
//! is defined. In GM a system outlives the instance and the room until code clears or destroys
//! it, so what happens later is explicit: `on_room_start`, `on_room_end` and `on_destroy` each
//! name one call, made on every system the instance built (CLONE_SPEC numbers, "Particle draw
//! rules" 8, Confirmed (recorder)):
//!
//! - `none`: nothing.
//! - `clear` (`part_particles_clear`): the live particles go; the emitters keep streaming.
//! - `destroy_emitters` (`part_emitter_destroy_all`): the streams stop; live particles stay and
//!   still fire their death particles.
//! - `destroy` (`part_system_destroy`): the system is gone and draws nothing again.
//!
//! `on_room_start_except` lists rooms where Room Start does nothing at all, and
//! `on_room_start_destroys_instance` destroys the instance after the Room Start call (its Destroy
//! runs, so `on_destroy` too): go-the-spook-house oRainSys (client#826).
//!
//! None of the calls draws. Without `NumRules::particles_draw_rng` the built-in does nothing
//! (the particles are then the client's only).

use serde::{Deserialize, Serialize};

use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_error, state_mut, state_of,
};
use crate::def::{KindDef, ParamValue, Params, PartSystemDefId, RoomId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::particles::PartSystemId;
use crate::world::{Ctx, Flow, InstId};

const LIFECYCLE: &[&str] = &["none", "clear", "destroy_emitters", "destroy"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "system",
        ty: ParamType::List(&ParamType::Particles),
        required: true,
    },
    ParamSpec {
        name: "on_room_start",
        ty: ParamType::Enum(LIFECYCLE),
        required: false,
    },
    ParamSpec {
        name: "on_room_start_except",
        ty: ParamType::List(&ParamType::Room),
        required: false,
    },
    ParamSpec {
        name: "on_room_start_destroys_instance",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "on_room_end",
        ty: ParamType::Enum(LIFECYCLE),
        required: false,
    },
    ParamSpec {
        name: "on_destroy",
        ty: ParamType::Enum(LIFECYCLE),
        required: false,
    },
];

/// One of the lifecycle calls.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Lifecycle {
    /// Nothing.
    #[default]
    None,
    /// `part_particles_clear`.
    Clear,
    /// `part_emitter_destroy_all`.
    DestroyEmitters,
    /// `part_system_destroy`.
    Destroy,
}

fn lifecycle(params: &Params, name: &str) -> Result<Lifecycle, ParamError> {
    Ok(match param_enum(params, name, LIFECYCLE)? {
        None | Some("none") => Lifecycle::None,
        Some("clear") => Lifecycle::Clear,
        Some("destroy_emitters") => Lifecycle::DestroyEmitters,
        Some(_) => Lifecycle::Destroy,
    })
}

/// A particle system instance's settings and the systems it built.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ParticleSystemState {
    /// The definitions, built in this order.
    pub defs: Vec<PartSystemDefId>,
    /// Room Start call.
    pub on_room_start: Lifecycle,
    /// Rooms where Room Start does nothing (no call, no destroy).
    pub on_room_start_except: Vec<RoomId>,
    /// Room Start destroys the instance after its call.
    pub on_room_start_destroys_instance: bool,
    /// Room End call.
    pub on_room_end: Lifecycle,
    /// Destroy call.
    pub on_destroy: Lifecycle,
    /// The systems Create built, one per definition in order, until a `destroy` call.
    pub systems: Vec<PartSystemId>,
}

impl ParticleSystemState {
    /// Whether Room Start does something in `room`.
    pub fn room_start_runs_in(&self, room: RoomId) -> bool {
        !self.on_room_start_except.contains(&room)
    }
}

/// The params this built-in accepts.
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// Room Start and Room End when they call something or destroy the instance (Create and
/// Destroy always run).
pub fn event_set(kind: &KindDef) -> EventSet {
    let calls = |name: &str| matches!(kind.params.0.get(name), Some(ParamValue::Str(w)) if w != "none");
    let destroys = matches!(
        kind.params.0.get("on_room_start_destroys_instance"),
        Some(ParamValue::Bool(true))
    );
    EventSet {
        room_start: calls("on_room_start") || destroys,
        room_end: calls("on_room_end"),
        ..EventSet::default()
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let need = || {
        param_error(
            "system",
            "a particle_system needs `system` (a particles/<id> file or a list of them)",
        )
    };
    let defs = match params.0.get("system") {
        Some(ParamValue::Particles(d)) => vec![*d],
        Some(ParamValue::List(l)) if !l.is_empty() => l
            .iter()
            .map(|v| match v {
                ParamValue::Particles(d) => Ok(*d),
                _ => Err(need()),
            })
            .collect::<Result<_, _>>()?,
        _ => return Err(need()),
    };
    let rooms = || param_error("on_room_start_except", "must be a room or a list of rooms");
    let on_room_start_except = match params.0.get("on_room_start_except") {
        None => Vec::new(),
        Some(ParamValue::Room(r)) => vec![*r],
        Some(ParamValue::List(l)) if !l.is_empty() => l
            .iter()
            .map(|v| match v {
                ParamValue::Room(r) => Ok(*r),
                _ => Err(rooms()),
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(rooms()),
    };
    Ok(BuiltinState::ParticleSystem(ParticleSystemState {
        defs,
        on_room_start: lifecycle(params, "on_room_start")?,
        on_room_start_except,
        on_room_start_destroys_instance: param_bool(params, "on_room_start_destroys_instance")?
            .unwrap_or(false),
        on_room_end: lifecycle(params, "on_room_end")?,
        on_destroy: lifecycle(params, "on_destroy")?,
        systems: Vec::new(),
    }))
}

/// Runs one event.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    if !ctx.rules.num.particles_draw_rng {
        return Ok(Flow::Continue);
    }
    let Some(BuiltinState::ParticleSystem(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    let what = match ev {
        EventKind::Create => {
            let mut built = Vec::with_capacity(s.defs.len());
            for d in &s.defs {
                let def = d
                    .index()
                    .and_then(|i| ctx.def.particle_systems.get(i))
                    .ok_or_else(|| {
                        SimError::invariant(format!("particle system definition {} does not exist", d.0))
                    })?;
                let w = &mut *ctx.w;
                built.push(w.particles.build(&mut w.rng, def)?);
            }
            if let Some(BuiltinState::ParticleSystem(st)) = state_mut(ctx, me) {
                st.systems = built;
            }
            return Ok(Flow::Continue);
        }
        // In an excepted room Room Start does nothing at all.
        EventKind::RoomStart if !s.room_start_runs_in(ctx.w.room) => return Ok(Flow::Continue),
        EventKind::RoomStart => {
            call(ctx, me, &s.systems, s.on_room_start)?;
            if s.on_room_start_destroys_instance {
                return Ok(ctx.destroy(me));
            }
            return Ok(Flow::Continue);
        }
        EventKind::RoomEnd => s.on_room_end,
        EventKind::Destroy => s.on_destroy,
        _ => return Ok(Flow::Continue),
    };
    call(ctx, me, &s.systems, what)?;
    Ok(Flow::Continue)
}

/// Makes call `what` on each of `systems`, in order.
fn call(ctx: &mut Ctx, me: InstId, systems: &[PartSystemId], what: Lifecycle) -> Result<(), SimError> {
    let p = &mut ctx.w.particles;
    for &sys in systems {
        match what {
            Lifecycle::None => {}
            Lifecycle::Clear => p.clear(sys)?,
            Lifecycle::DestroyEmitters => p.destroy_all_emitters(sys)?,
            Lifecycle::Destroy => p.destroy_system(sys)?,
        }
    }
    if what == Lifecycle::Destroy
        && let Some(BuiltinState::ParticleSystem(st)) = state_mut(ctx, me)
    {
        st.systems.clear();
    }
    Ok(())
}
