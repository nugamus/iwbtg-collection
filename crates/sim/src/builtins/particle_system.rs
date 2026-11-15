//! Built-in `particle_system` (docs/format/builtins/particle_system.md; N10, #765).
//!
//! An object whose code builds a particle system (rely objEffect, keep-celebrating objcpart,
//! go-the-spook-house oRainSys, eat-the-lemon docobj2, docobj3, object131, object132). Create
//! builds its `particles/<id>.toml` system in `World::particles` (`Particles::build`). In GM a
//! system outlives the instance and the room until code clears or destroys it, so what happens
//! later is explicit: `on_room_start`, `on_room_end` and `on_destroy` each name one call
//! (CLONE_SPEC numbers, "Particle draw rules" 8, Confirmed (recorder)):
//!
//! - `none`: nothing.
//! - `clear` (`part_particles_clear`): the live particles go; the emitters keep streaming.
//! - `destroy_emitters` (`part_emitter_destroy_all`): the streams stop; live particles stay and
//!   still fire their death particles.
//! - `destroy` (`part_system_destroy`): the system is gone and draws nothing again.
//!
//! None of the calls draws. Without `NumRules::particles_draw_rng` the built-in does nothing
//! (the particles are then the client's only).

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_error, state_mut, state_of};
use crate::def::{KindDef, ParamValue, Params, PartSystemDefId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::particles::PartSystemId;
use crate::world::{Ctx, Flow, InstId};

const LIFECYCLE: &[&str] = &["none", "clear", "destroy_emitters", "destroy"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "system",
        ty: ParamType::Particles,
        required: true,
    },
    ParamSpec {
        name: "on_room_start",
        ty: ParamType::Enum(LIFECYCLE),
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

/// A particle system instance's settings and the system it built.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ParticleSystemState {
    /// The definition.
    pub def: PartSystemDefId,
    /// Room Start call.
    pub on_room_start: Lifecycle,
    /// Room End call.
    pub on_room_end: Lifecycle,
    /// Destroy call.
    pub on_destroy: Lifecycle,
    /// The system Create built, until a `destroy` call.
    pub system: Option<PartSystemId>,
}

/// The params this built-in accepts.
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// Room Start and Room End when they call something (Create and Destroy always run).
pub fn event_set(kind: &KindDef) -> EventSet {
    let calls = |name: &str| matches!(kind.params.0.get(name), Some(ParamValue::Str(w)) if w != "none");
    EventSet {
        room_start: calls("on_room_start"),
        room_end: calls("on_room_end"),
        ..EventSet::default()
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let def = match params.0.get("system") {
        Some(ParamValue::Particles(d)) => *d,
        _ => {
            return Err(param_error(
                "system",
                "a particle_system needs `system` (a particles/<id> file)",
            ));
        }
    };
    Ok(BuiltinState::ParticleSystem(ParticleSystemState {
        def,
        on_room_start: lifecycle(params, "on_room_start")?,
        on_room_end: lifecycle(params, "on_room_end")?,
        on_destroy: lifecycle(params, "on_destroy")?,
        system: None,
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
    let call = match ev {
        EventKind::Create => {
            let def = s
                .def
                .index()
                .and_then(|i| ctx.def.particle_systems.get(i))
                .ok_or_else(|| {
                    SimError::invariant(format!("particle system definition {} does not exist", s.def.0))
                })?;
            let w = &mut *ctx.w;
            let built = w.particles.build(&mut w.rng, def)?;
            if let Some(BuiltinState::ParticleSystem(st)) = state_mut(ctx, me) {
                st.system = Some(built);
            }
            return Ok(Flow::Continue);
        }
        EventKind::RoomStart => s.on_room_start,
        EventKind::RoomEnd => s.on_room_end,
        EventKind::Destroy => s.on_destroy,
        _ => return Ok(Flow::Continue),
    };
    let Some(sys) = s.system else {
        return Ok(Flow::Continue);
    };
    let p = &mut ctx.w.particles;
    match call {
        Lifecycle::None => {}
        Lifecycle::Clear => p.clear(sys)?,
        Lifecycle::DestroyEmitters => p.destroy_all_emitters(sys)?,
        Lifecycle::Destroy => {
            p.destroy_system(sys)?;
            if let Some(BuiltinState::ParticleSystem(st)) = state_mut(ctx, me) {
                st.system = None;
            }
        }
    }
    Ok(Flow::Continue)
}
