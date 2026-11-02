//! Built-in `kid_param_zone` (mechanics `kid_param_zone`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/kid_param_zone.md). Tier 3.
//!
//! While a kid overlaps it, it writes the kid numbers it names through
//! `kid::api::apply_param_zone` on every tick of the overlap (`run_speed`, `max_vspeed`,
//! `max_fall_speed`, `jump`, `jump2`, `jump3`). `target = "toucher"` acts on the kid that
//! touched it; `target = "player"` acts on the first kid even when the second kid touched it
//! (rainbow-feather object867 and object869). The values persist until another zone, or a new
//! kid Create, changes them (Inferred).

use serde::{Deserialize, Serialize};

use super::warp::KIDS;
use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_error, state_of};
use crate::def::{ClassMask, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::kid::api::{self, ParamZone};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

const TARGETS: &[&str] = &["toucher", "player"];
const NUMBERS: [&str; 6] = [
    "run_speed",
    "max_vspeed",
    "max_fall_speed",
    "jump",
    "jump2",
    "jump3",
];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "run_speed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "max_vspeed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "max_fall_speed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "jump",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "jump2",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "jump3",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "target",
        ty: ParamType::Enum(TARGETS),
        required: false,
    },
];

/// A zone numbers and target.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ZoneState {
    /// The kid numbers it sets.
    pub numbers: Vec<(String, Real)>,
    /// Acts on the first kid, not the toucher.
    pub player: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: the kid touching it.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    set.collisions.push(KIDS);
    set
}

/// A new instance state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let mut numbers = Vec::new();
    for name in NUMBERS {
        match params.0.get(name) {
            None => {}
            Some(ParamValue::Real(r)) if r.is_finite() => numbers.push((name.to_owned(), *r)),
            Some(_) => return Err(param_error(name, "must be a finite number")),
        }
    }
    if numbers.is_empty() {
        return Err(param_error("run_speed", "a zone needs at least one kid number"));
    }
    Ok(BuiltinState::ParamZone(ZoneState {
        numbers,
        player: param_enum(params, "target", TARGETS)? == Some("player"),
    }))
}

/// Runs one event on a zone.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let EventKind::Collision { with, other } = ev else {
        return Ok(Flow::Continue);
    };
    if !with.intersects(KIDS) {
        return Ok(Flow::Continue);
    }
    let Some(BuiltinState::ParamZone(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    let kid = if s.player {
        match ctx
            .w
            .instances()
            .iter()
            .find(|i| i.alive && i.classes.intersects(ClassMask::KID))
        {
            Some(i) => i.id,
            None => return Ok(Flow::Continue),
        }
    } else {
        *other
    };
    let mut params = Params::default();
    for (name, v) in s.numbers {
        params.0.insert(name, ParamValue::Real(v));
    }
    api::apply_param_zone(ctx, kid, &ParamZone { params })?;
    Ok(Flow::Continue)
}
