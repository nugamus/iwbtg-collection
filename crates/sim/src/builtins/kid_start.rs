//! Built-in `kid_start` (playerStart; mechanics `player_start`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/kid_start.md). Tier 1. WP0 creates the file; WP6a owns it.
//!
//! Room Start creates the kid at the start's position plus the spawn offset when no kid exists
//! (the kid check counts both kid kinds, active and alive). Where the offset is added depends on
//! the game: in Create (`shift_at = "create"`, rely, every A1 and A3 game: the start moves
//! itself) or in Room Start (`"room_start"`, animus). `creates` and `grav_gated` pick which kid:
//!
//! - `creates = "player"`, `grav_gated = true` (rely and the other A1 and A2 games): only when
//!   `global.grav` is 0; with grav 1 nothing is created.
//! - `creates = "player"`, `grav_gated = false` (RORI, showdown and A3/A4): always the normal kid.
//! - `creates = "player_or_player2"` (animus): grav 0 gives the kid at (+17, +23), grav 1 the
//!   flipped kid at (+23, +17). Needs `shift_at = "room_start"`.
//!
//! The kid's Create runs inside this call, so it sees the room's `room_speed` (the file speed,
//! R14: Confirmed (recorder) rely `t-room-1.csv` f2, speed 30 before world's Room Start sets 50).

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_real, state_of};
use crate::def::{EngineKind, KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId, SpawnInit};

const CREATES: &[&str] = &["player", "player_or_player2"];
const SHIFT_AT: &[&str] = &["create", "room_start"];

const SPECS: &[ParamSpec] = &[
    ParamSpec { name: "offset_x", ty: ParamType::Real, required: false },
    ParamSpec { name: "offset_y", ty: ParamType::Real, required: false },
    ParamSpec { name: "flipped_offset_x", ty: ParamType::Real, required: false },
    ParamSpec { name: "flipped_offset_y", ty: ParamType::Real, required: false },
    ParamSpec { name: "creates", ty: ParamType::Enum(CREATES), required: false },
    ParamSpec { name: "grav_gated", ty: ParamType::Bool, required: false },
    ParamSpec { name: "shift_at", ty: ParamType::Enum(SHIFT_AT), required: false },
];

/// A kid start's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KidStartState {
    /// Offset of the normal kid (default 17, 23; machinary 16, 23).
    pub offset: (Real, Real),
    /// Offset of the flipped kid (default 23, 17).
    pub flipped_offset: (Real, Real),
    /// Creates `player2` when `global.grav` is set.
    pub creates_player2: bool,
    /// Creates nothing while `global.grav` is set (A1, A2).
    pub grav_gated: bool,
    /// The offset is added in Room Start instead of Create.
    pub shift_in_room_start: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Room Start.
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet {
        room_start: true,
        ..EventSet::default()
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let r = |name: &str, default: f64| Ok::<Real, ParamError>(param_real(params, name)?.unwrap_or(Real::new(default)));
    let offset = (r("offset_x", 17.0)?, r("offset_y", 23.0)?);
    let flipped_offset = (
        param_real(params, "flipped_offset_x")?.unwrap_or(offset.1),
        param_real(params, "flipped_offset_y")?.unwrap_or(offset.0),
    );
    let creates_player2 = param_enum(params, "creates", CREATES)? == Some("player_or_player2");
    let shift_in_room_start = param_enum(params, "shift_at", SHIFT_AT)? == Some("room_start");
    if creates_player2 && !shift_in_room_start {
        return Err(super::param_error(
            "shift_at",
            "creates = player_or_player2 needs shift_at = room_start (the shift depends on gravity)",
        ));
    }
    Ok(BuiltinState::KidStart(KidStartState {
        offset,
        flipped_offset,
        creates_player2,
        grav_gated: param_bool(params, "grav_gated")?.unwrap_or(true),
        shift_in_room_start,
    }))
}

/// Runs one event: Create shifts the start (when the game does it there); Room Start creates the
/// kid when none exists.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::KidStart(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create if !s.shift_in_room_start => {
            if let Some(inst) = ctx.w.get_mut(me) {
                let (x, y) = (inst.m.x() + s.offset.0, inst.m.y() + s.offset.1);
                inst.m.set_x(x)?;
                inst.m.set_y(y)?;
            }
        }
        EventKind::RoomStart => {
            let rules = ctx.rules;
            if ctx.w.kid(&rules.tick).is_some() {
                return Ok(Flow::Continue);
            }
            let Some(inst) = ctx.w.get(me) else {
                return Ok(Flow::Continue);
            };
            let (mut x, mut y) = (inst.m.x(), inst.m.y());
            let grav = ctx.w.engine.kid.grav;
            let role = if s.creates_player2 && grav {
                EngineKind::KidFlipped
            } else if s.grav_gated && grav {
                return Ok(Flow::Continue);
            } else {
                EngineKind::Kid
            };
            if s.shift_in_room_start {
                let off = if role == EngineKind::KidFlipped { s.flipped_offset } else { s.offset };
                x += off.0;
                y += off.1;
            }
            ctx.spawn_engine(role, x, y, SpawnInit::default())?
                .ok_or_else(|| SimError::invariant("kid_start: the game has no kid kind for this role"))?;
        }
        _ => {}
    }
    Ok(Flow::Continue)
}
