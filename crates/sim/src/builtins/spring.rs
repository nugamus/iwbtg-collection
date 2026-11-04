//! Built-in `spring` (mechanics `spring`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/spring.md). Tier 3.
//!
//! Showdown JumpBlockD and JumpBlockS: on touch the kid motion is set to direction 90 at
//! `speed` (10): `hspeed` 0 and `vspeed` minus the speed. Then `djump` is set (D, `djump =
//! "set"`, `kid::api::refresh_air_jump`) or cleared (S, `djump = "clear"`,
//! `kid::api::set_air_jump`). Destroyed outside the room.

use serde::{Deserialize, Serialize};

use super::warp::KIDS;
use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_real, state_of};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::kid::api;
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

const DJUMP: &[&str] = &["keep", "set", "clear"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "speed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "djump",
        ty: ParamType::Enum(DJUMP),
        required: false,
    },
];

/// A spring settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SpringState {
    /// Launch speed (upward).
    pub speed: Real,
    /// Sets `djump` after the launch.
    pub refresh: bool,
    /// Clears `djump` after the launch.
    pub clear: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: the kid touching it, and Outside Room.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        outside_room: true,
        ..EventSet::default()
    };
    set.collisions.push(KIDS);
    set
}

/// A new instance state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let djump = param_enum(params, "djump", DJUMP)?;
    Ok(BuiltinState::Spring(SpringState {
        speed: param_real(params, "speed")?.unwrap_or(Real::new(10.0)),
        refresh: djump == Some("set"),
        clear: djump == Some("clear"),
    }))
}

/// Runs one event on a spring.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Spring(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Collision { with, other } if with.intersects(KIDS) => {
            if let Some(k) = ctx.w.get_mut(*other) {
                k.m.set_hspeed(Real::ZERO)?;
                k.m.set_vspeed(-s.speed)?;
            }
            if s.refresh {
                api::refresh_air_jump(ctx, *other);
            } else if s.clear {
                api::set_air_jump(ctx, *other, false);
            }
            Ok(Flow::Continue)
        }
        EventKind::OutsideRoom => Ok(ctx.destroy(me)),
        _ => Ok(Flow::Continue),
    }
}
