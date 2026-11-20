//! Built-in `warp` (mechanics `warp`, roomChanger; docs/architecture/sim/builtins.md;
//! docs/format/builtins/warp.md). Tier 1. WP0 creates the file; WP6a owns it.
//!
//! Collision with the kid, when `on_collide`: set the transition for the client, request the
//! room (deferred to tick row 15, Confirmed), then in the same handler either destroy the kid
//! and the bow when `warp_x` and `warp_y` are both 0 (the new room's `kid_start` makes a fresh
//! kid) or move the kid (he keeps his speed and djump). Returns `AbortTick`.
//!
//! The move, in order (KID-B roomChanger family; KID-A4 NY roomChanger): with `clear_rising`, a
//! negative vspeed becomes 0 (not adjusted for reversed gravity). Then each axis: `zero_axis =
//! "set"` sets x to `warp_x` and y to `warp_y`; `zero_axis = "keep"` sets only an axis whose
//! target is not 0 and moves a 0 axis by `screen_shift` screens (800 px in x, 608 px in y).
//!
//! Gate (LR warpToBoss, KAE warpCount, XIA objWarpfin; mechanics builtin.warp "Gated forms"):
//! with `gate_bosses` or `gate_items`, every Step counts the listed boss and item flags that are
//! set; the gate is open while the count reaches `gate_need` (default: all listed). Closed,
//! `gate_closed = "hide"` makes the warp invisible and its touch do nothing; `"destroy"`
//! destroys it. It starts closed and only Step opens it, so a flag set during a tick opens the
//! gate at the next Step.

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_real, state_of};
use crate::def::{ClassMask, EngineKind, KindDef, ParamValue, Params, RoomId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::{Real, gm_eq, gm_lt};
use crate::output::OutputEvent;
use crate::room::{RoomOutput, RoomRequest};
use crate::world::{Ctx, Flow, InstId};

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "room_to",
        ty: ParamType::Room,
        required: true,
    },
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
    ParamSpec {
        name: "on_collide",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "visible",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "zero_axis",
        ty: ParamType::Enum(ZERO_AXIS),
        required: false,
    },
    ParamSpec {
        name: "screen_shift",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "clear_rising",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "gate_bosses",
        ty: ParamType::List(&ParamType::Real),
        required: false,
    },
    ParamSpec {
        name: "gate_items",
        ty: ParamType::List(&ParamType::Real),
        required: false,
    },
    ParamSpec {
        name: "gate_need",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "gate_closed",
        ty: ParamType::Enum(GATE_CLOSED),
        required: false,
    },
];

/// `gate_closed` values.
const GATE_CLOSED: &[&str] = &["hide", "destroy"];

/// The params that give a warp a gate (and so a Step event).
pub(crate) const GATE_PARAMS: &[&str] = &["gate_bosses", "gate_items"];

/// `zero_axis` values: `set` writes both targets, `keep` leaves a 0 axis (plus `screen_shift`).
const ZERO_AXIS: &[&str] = &["set", "keep"];

/// One screen in x and in y, for `screen_shift` (KID-B roomChanger: screens times 800 and 608).
const SCREEN_W: f64 = 800.0;
/// See [`SCREEN_W`].
const SCREEN_H: f64 = 608.0;

/// A warp's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WarpState {
    /// Where it goes.
    pub room_to: RoomId,
    /// Kid x there; 0 with `warp_y` 0 means destroy the kid.
    pub warp_x: Real,
    /// Kid y there.
    pub warp_y: Real,
    /// `transition_kind` (0 none, 21 fade out and in).
    pub transition: u8,
    /// `transition_steps`.
    pub transition_steps: u32,
    /// `onCollide`: the touch does something (warp sets it true in Create).
    pub on_collide: bool,
    /// `visible` override.
    pub visible: Option<bool>,
    /// How the kid is moved when he is kept.
    pub moves: WarpMove,
    /// The boss and item gate, if any.
    pub gate: Option<WarpGate>,
}

/// A warp's boss and item gate.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WarpGate {
    /// The listed boss flags, one bit each (`MechGlobals::bosses`).
    pub bosses: u16,
    /// The listed item flags, one bit each (`MechGlobals::items`).
    pub items: u16,
    /// How many listed flags must be set.
    pub need: u8,
    /// Closed means destroyed (`gate_closed = "destroy"`), not hidden.
    pub destroy: bool,
    /// Open as of the last Step.
    pub open: bool,
}

/// How a warp moves a kid it keeps (not used by `edge_warp` or the kid's boundary).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct WarpMove {
    /// `zero_axis = "keep"`: an axis whose target is 0 is not set (NY XX and YY, KAE).
    pub keep_zero_axis: bool,
    /// Screens a kept 0 axis moves by (KAE `screens`); 0 unless `keep_zero_axis`.
    pub screen_shift: Real,
    /// A negative vspeed becomes 0 first (KAE).
    pub clear_rising: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// The kid classes a touch handler listens for.
pub(crate) const KIDS: ClassMask = ClassMask(ClassMask::KID.0 | ClassMask::KID_FLIPPED.0);

/// What a kind with this base handles: the kid touching it, and Step for a gate.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    set.collisions.push(KIDS);
    set.step = GATE_PARAMS.iter().any(|p| kind.params.0.contains_key(*p));
    set
}

/// A non-negative whole number param as `u32`.
pub(crate) fn whole(v: Real, name: &str) -> Result<u32, ParamError> {
    let x = v.get();
    if x < 0.0 || x.fract() != 0.0 {
        return Err(super::param_error(name, "must be a whole number, 0 or more"));
    }
    u32::try_from(crate::num::to_int_trunc(v).map_err(|_| super::param_error(name, "out of range"))?)
        .map_err(|_| super::param_error(name, "out of range"))
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let room_to = match params.0.get("room_to") {
        Some(ParamValue::Room(r)) => *r,
        _ => return Err(super::param_error("room_to", "a warp needs the room it goes to")),
    };
    let zero = Real::ZERO;
    Ok(BuiltinState::Warp(WarpState {
        room_to,
        warp_x: param_real(params, "warp_x")?.unwrap_or(zero),
        warp_y: param_real(params, "warp_y")?.unwrap_or(zero),
        transition: u8::try_from(whole(
            param_real(params, "transition")?.unwrap_or(zero),
            "transition",
        )?)
        .map_err(|_| super::param_error("transition", "must be 0 to 255"))?,
        transition_steps: whole(
            param_real(params, "transition_steps")?.unwrap_or(zero),
            "transition_steps",
        )?,
        on_collide: param_bool(params, "on_collide")?.unwrap_or(true),
        visible: param_bool(params, "visible")?,
        moves: move_params(params)?,
        gate: gate_params(params)?,
    }))
}

/// A flag list param as bits and their count: one whole number 0 to 15 or a list of them.
fn flag_list(params: &Params, name: &str) -> Result<(u16, u8), ParamError> {
    let one = |v: &ParamValue| -> Result<u16, ParamError> {
        let mut p = Params::default();
        p.0.insert(name.to_owned(), v.clone());
        Ok(1u16 << super::flag_index(&p, name)?)
    };
    let mut bits = 0u16;
    match params.0.get(name) {
        None => {}
        Some(ParamValue::List(l)) => {
            for v in l {
                let b = one(v)?;
                if bits & b != 0 {
                    return Err(super::param_error(name, "lists a flag twice"));
                }
                bits |= b;
            }
        }
        Some(v) => bits = one(v)?,
    }
    Ok((bits, u8::try_from(bits.count_ones()).unwrap_or(u8::MAX)))
}

/// The gate params. `gate_need` and `gate_closed` need a flag list; a hiding gate drives
/// `visible`, so `visible` cannot be set too.
fn gate_params(params: &Params) -> Result<Option<WarpGate>, ParamError> {
    let (bosses, nb) = flag_list(params, "gate_bosses")?;
    let (items, ni) = flag_list(params, "gate_items")?;
    let listed = nb + ni;
    let need = param_real(params, "gate_need")?;
    let closed = param_enum(params, "gate_closed", GATE_CLOSED)?;
    if listed == 0 {
        if need.is_some() || closed.is_some() {
            return Err(super::param_error(
                "gate_need",
                "gate_need and gate_closed need gate_bosses or gate_items",
            ));
        }
        return Ok(None);
    }
    let need = match need {
        None => listed,
        Some(v) => u8::try_from(whole(v, "gate_need")?)
            .ok()
            .filter(|n| *n <= listed)
            .ok_or_else(|| super::param_error("gate_need", "is more than the listed flags"))?,
    };
    let destroy = closed == Some("destroy");
    if !destroy && params.0.contains_key("visible") {
        return Err(super::param_error(
            "visible",
            "a gate with gate_closed = \"hide\" sets visible itself",
        ));
    }
    Ok(Some(WarpGate {
        bosses,
        items,
        need,
        destroy,
        open: false,
    }))
}
/// The move params; `screen_shift` needs `zero_axis = "keep"`.
fn move_params(params: &Params) -> Result<WarpMove, ParamError> {
    let keep_zero_axis = param_enum(params, "zero_axis", ZERO_AXIS)? == Some("keep");
    let screen_shift = param_real(params, "screen_shift")?.unwrap_or(Real::ZERO);
    // gm-compare: ok (a param check, not a GML test)
    if screen_shift.get() != 0.0 && !keep_zero_axis {
        return Err(super::param_error(
            "screen_shift",
            "moves only an axis that is kept: set zero_axis = \"keep\"",
        ));
    }
    Ok(WarpMove {
        keep_zero_axis,
        screen_shift,
        clear_rising: param_bool(params, "clear_rising")?.unwrap_or(false),
    })
}

/// Moves a kept kid: `clear_rising`, then each axis by `zero_axis` and `screen_shift`.
fn move_kid(ctx: &mut Ctx, kid: InstId, s: &WarpState) -> Result<(), SimError> {
    let n = &ctx.rules.num;
    let Some(k) = ctx.w.get_mut(kid) else {
        return Ok(());
    };
    let m = &s.moves;
    if m.clear_rising && gm_lt(k.m.vspeed(), Real::ZERO, n) {
        k.m.set_vspeed(Real::ZERO)?;
    }
    if !m.keep_zero_axis {
        k.m.set_x(s.warp_x)?;
        k.m.set_y(s.warp_y)?;
        return Ok(());
    }
    let x = if gm_eq(s.warp_x, Real::ZERO, n) {
        k.m.x() + m.screen_shift * Real::new(SCREEN_W)
    } else {
        s.warp_x
    };
    k.m.set_x(x)?;
    let y = if gm_eq(s.warp_y, Real::ZERO, n) {
        k.m.y() + m.screen_shift * Real::new(SCREEN_H)
    } else {
        s.warp_y
    };
    k.m.set_y(y)
}

/// Destroys every kid (both kinds) and the bow (what the roomChanger does for a warp to (0, 0)).
pub(crate) fn destroy_kid_and_bow(ctx: &mut Ctx) -> Result<(), SimError> {
    let bow = ctx.def.engine_kind(EngineKind::Bow);
    let ids: Vec<InstId> = ctx
        .w
        .iter_alive()
        .filter(|i| i.classes.intersects(KIDS) || Some(i.kind) == bow)
        .map(|i| i.id)
        .collect();
    for id in ids {
        if ctx.destroy(id).is_halt() {
            return Err(ctx.halted_error());
        }
    }
    Ok(())
}

/// Sets the transition for the next room change (client-visual only). Kind 0 is none.
pub(crate) fn emit_transition(ctx: &mut Ctx, kind: u8, steps: u32) {
    if kind > 0 {
        ctx.emit(OutputEvent::Room(RoomOutput::Transition { kind, steps }));
    }
}

/// A gate's Step: count the listed flags that are set, then open, hide or destroy.
fn gate_step(ctx: &mut Ctx, me: InstId, g: &WarpGate) -> Flow {
    let mech = &ctx.w.engine.mech;
    let set = (g.bosses & mech.bosses).count_ones() + (g.items & mech.items).count_ones();
    let open = set >= u32::from(g.need);
    if let Some(BuiltinState::Warp(WarpState { gate: Some(st), .. })) = super::state_mut(ctx, me) {
        st.open = open;
    }
    if g.destroy {
        return if open { Flow::Continue } else { ctx.destroy(me) };
    }
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.visible = open;
    }
    Flow::Continue
}

/// Runs one event on a warp.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Warp(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            let hidden = s.gate.as_ref().is_some_and(|g| !g.destroy);
            let visible = if hidden { Some(false) } else { s.visible };
            if let (Some(v), Some(inst)) = (visible, ctx.w.get_mut(me)) {
                inst.visible = v;
            }
            Ok(Flow::Continue)
        }
        EventKind::Step => Ok(s.gate.as_ref().map_or(Flow::Continue, |g| gate_step(ctx, me, g))),
        EventKind::Collision { with, other }
            if with.intersects(KIDS) && s.on_collide && s.gate.as_ref().is_none_or(|g| g.open) =>
        {
            emit_transition(ctx, s.transition, s.transition_steps);
            ctx.request_room(RoomRequest::Goto(s.room_to));
            // gm-compare: ok (the (0, 0) sentinel of the params, not a GML test)
            if s.warp_x.get() == 0.0 && s.warp_y.get() == 0.0 {
                destroy_kid_and_bow(ctx)?;
            } else {
                move_kid(ctx, *other, &s)?;
            }
            Ok(Flow::AbortTick)
        }
        _ => Ok(Flow::Continue),
    }
}
