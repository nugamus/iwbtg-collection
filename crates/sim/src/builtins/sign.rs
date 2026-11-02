//! Built-in `sign` (mechanics `builtin.sign`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/sign.md). Tier 3.
//!
//! Raises a client text box. `mode`:
//!
//! - `on_up_press` (FT object796, ANI kanbanED variant): each Step, when Up is pressed this tick
//!   and the sign overlaps a kid, it emits `BuiltinOutput::TextBox` with its `text`.
//! - `while_overlapping` (KID-C objSign): the kid touching it emits the text every tick it does.
//!
//! Showing, wrapping and closing the box is the client's (the sim never reads anything back).

use serde::{Deserialize, Serialize};

use super::warp::KIDS;
use super::{
    BuiltinOutput, BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_error, state_of,
};
use crate::collide::Target;
use crate::collide::query::place_meeting;
use crate::def::{KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::input::Action;
use crate::output::OutputEvent;
use crate::world::{Ctx, Flow, InstId};

const MODES: &[&str] = &["on_up_press", "while_overlapping"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "text",
        ty: ParamType::Text,
        required: true,
    },
    ParamSpec {
        name: "mode",
        ty: ParamType::Enum(MODES),
        required: false,
    },
];

/// A sign's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SignState {
    /// The message.
    pub text: String,
    /// Shows on an Up press (else while a kid overlaps).
    pub on_up_press: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step (the Up press test) and a kid touching it. Both are
/// listed; the mode decides which one acts.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        step: true,
        ..EventSet::default()
    };
    set.collisions.push(KIDS);
    set
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let text = match params.0.get("text") {
        Some(ParamValue::Str(s)) => s.clone(),
        _ => return Err(param_error("text", "a sign needs its text")),
    };
    let on_up_press = param_enum(params, "mode", MODES)?.unwrap_or("on_up_press") == "on_up_press";
    Ok(BuiltinState::Sign(SignState { text, on_up_press }))
}

/// Runs one event on a sign.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Sign(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let text = s.text.clone();
    let show = match ev {
        EventKind::Step if s.on_up_press && ctx.w.input.pressed(Action::Up) => {
            let Some(inst) = ctx.w.get(me) else {
                return Ok(Flow::Continue);
            };
            let (x, y) = (inst.m.x(), inst.m.y());
            place_meeting(ctx, me, x, y, Target::Class(KIDS))?
        }
        EventKind::Collision { with, .. } => with.intersects(KIDS) && !s.on_up_press,
        _ => false,
    };
    if show {
        ctx.emit(OutputEvent::Builtin(BuiltinOutput::TextBox { text }));
    }
    Ok(Flow::Continue)
}
