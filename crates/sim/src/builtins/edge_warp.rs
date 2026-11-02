//! Built-in `edge_warp` (outsideroomChanger, roomout, oRoomSwitch; mechanics `edge_warp`;
//! docs/plans/sim-core.md 6.1, K10; docs/format/builtins/edge_warp.md). Tier 2. WP0 creates the
//! file; WP6b owns it.
//!
//! The kid code reads an edge warp when he crosses the room boundary (Intersect Boundary, class
//! `EDGE_WARP`); the built-in itself has no events except in `room_assign` mode. Two forms:
//!
//! - **Single target** (`room_to`, `warp_x`, `warp_y`): kept in a [`WarpState`] (variant
//!   `BuiltinState::Warp`), the same state a `warp` has, so the kid reads `room_to`, `warp_x`,
//!   `warp_y` from it (#250).
//! - **Per-edge table** (NY roomout, kid/A4.md): `left_room`, `right_room`, `up_room`,
//!   `down_room` (each optional) in an [`EdgeTable`] (variant `BuiltinState::EdgeTable`).
//!   [`EdgeTable::target`] answers for the edge the kid crossed: left when `x < 0`, right when
//!   `x > room_width`, up when `y < 0`, down when `y > room_height`, checked in that order; an
//!   unset edge is `None` (the kid kills, or at the top does nothing; that is the kid rule).
//!
//! `mode = "room_assign"` (spook house oRoomSwitch, K10): a kid touch queues the room change like
//! `room_goto` and puts the kid at `warp_x` (and at `warp_y` when it is not 0), keeping his
//! speeds (Confirmed (recorder), trace SH-roomswitch). An instance with no `room_to` (the 24
//! switches without a table entry) does nothing. The loader resolves the original table: it
//! knows the room and the side of each placed instance. Returns `AbortTick` as `warp` does
//! (Inferred: the room-change tick skips one End Step).

use serde::{Deserialize, Serialize};

use super::warp::{KIDS, WarpState};
use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_error, param_real, state_of};
use crate::def::{KindDef, ParamValue, Params, RoomId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::room::RoomRequest;
use crate::world::{Ctx, Flow, InstId};

const MODES: &[&str] = &["boundary", "room_assign"];

const fn room(name: &'static str) -> ParamSpec {
    ParamSpec {
        name,
        ty: ParamType::Room,
        required: false,
    }
}

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "mode",
        ty: ParamType::Enum(MODES),
        required: false,
    },
    room("room_to"),
    ParamSpec {
        name: "warp_x",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "warp_y",
        ty: ParamType::Real,
        required: false,
    },
    room("left_room"),
    room("right_room"),
    room("up_room"),
    room("down_room"),
];

/// The edge a kid crossed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    /// `x < 0`.
    Left,
    /// `x > room_width`.
    Right,
    /// `y < 0`.
    Up,
    /// `y > room_height`.
    Down,
}

/// A per-edge table of rooms (NY roomout h, m, u, s).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct EdgeTable {
    /// Room when `x < 0` (`h`).
    pub left: Option<RoomId>,
    /// Room when `x > room_width` (`m`).
    pub right: Option<RoomId>,
    /// Room when `y < 0` (`u`).
    pub up: Option<RoomId>,
    /// Room when `y > room_height` (`s`).
    pub down: Option<RoomId>,
}

impl EdgeTable {
    /// The room for the crossed `edge`; `None` when that edge has no target.
    pub fn target(&self, edge: Edge) -> Option<RoomId> {
        match edge {
            Edge::Left => self.left,
            Edge::Right => self.right,
            Edge::Up => self.up,
            Edge::Down => self.down,
        }
    }
}

/// A `room_assign` switch: where it sends the kid.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RoomAssign {
    /// The room.
    pub room_to: RoomId,
    /// The kid x there.
    pub x: Real,
    /// The kid y there when not 0.
    pub y: Real,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: the kid touching it, in `room_assign` mode only.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    if matches!(kind.params.0.get("mode"), Some(ParamValue::Str(w)) if w == "room_assign") {
        set.collisions.push(KIDS);
    }
    set
}

fn room_param(params: &Params, name: &str) -> Result<Option<RoomId>, ParamError> {
    match params.0.get(name) {
        None => Ok(None),
        Some(ParamValue::Room(r)) => Ok(Some(*r)),
        Some(_) => Err(param_error(name, "must name a room")),
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let room_to = room_param(params, "room_to")?;
    let zero = Real::ZERO;
    let x = param_real(params, "warp_x")?.unwrap_or(zero);
    let y = param_real(params, "warp_y")?.unwrap_or(zero);
    if param_enum(params, "mode", MODES)? == Some("room_assign") {
        return Ok(match room_to {
            Some(room_to) => BuiltinState::RoomAssign(RoomAssign { room_to, x, y }),
            None => BuiltinState::Stateless,
        });
    }
    let table = EdgeTable {
        left: room_param(params, "left_room")?,
        right: room_param(params, "right_room")?,
        up: room_param(params, "up_room")?,
        down: room_param(params, "down_room")?,
    };
    match (room_to, table == EdgeTable::default()) {
        (Some(_), false) => Err(param_error(
            "room_to",
            "give either room_to (one target) or the per-edge rooms, not both",
        )),
        (Some(room_to), true) => Ok(BuiltinState::Warp(WarpState {
            room_to,
            warp_x: x,
            warp_y: y,
            transition: 0,
            transition_steps: 0,
            on_collide: false,
            visible: None,
        })),
        (None, false) => Ok(BuiltinState::EdgeTable(table)),
        (None, true) => Err(param_error(
            "room_to",
            "an edge warp needs room_to or at least one per-edge room",
        )),
    }
}

/// Runs one event on an edge warp.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let EventKind::Collision { with, other } = ev else {
        return Ok(Flow::Continue);
    };
    if !with.intersects(KIDS) {
        return Ok(Flow::Continue);
    }
    let Some(BuiltinState::RoomAssign(a)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let a = a.clone();
    ctx.request_room(RoomRequest::Goto(a.room_to));
    if let Some(kid) = ctx.w.get_mut(*other) {
        kid.m.set_x(a.x)?;
        if a.y.get() != 0.0 {
            kid.m.set_y(a.y)?;
        }
    }
    Ok(Flow::AbortTick)
}
