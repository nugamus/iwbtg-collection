//! Built-in `killer` (mechanics `spike` and `cherry`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/killer.md). Tier 1. WP0 creates the file; WP6a owns it.
//!
//! A generic killer: class `killer` (set by the loader) with its own sprite and mask, so the
//! kid's "Collision with killer" kills; no kill code here. Create applies the instance settings
//! (the cherry: `image_speed` 1/15, a move, a scale) and Outside Room destroys when
//! `destroy_outside_room` (the cherry, default off for a plain killer). The bounce rule of the
//! bouncing cherries (U-MECH-5) is tier 3 and not here.

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_real, state_of};
use crate::def::{KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "image_speed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "hspeed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "vspeed",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "scale",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "visible",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "depth",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "destroy_outside_room",
        ty: ParamType::Bool,
        required: false,
    },
];

/// A killer's per-instance settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KillerState {
    /// `image_speed` (the cherry: 1/15); the default image speed otherwise.
    pub image_speed: Option<Real>,
    /// `hspeed`.
    pub hspeed: Real,
    /// `vspeed`.
    pub vspeed: Real,
    /// `image_xscale` and `image_yscale` (LR object698: 9).
    pub scale: Option<Real>,
    /// `visible` override.
    pub visible: Option<bool>,
    /// `depth` override.
    pub depth: Option<Real>,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Outside Room when `destroy_outside_room`.
pub fn event_set(kind: &KindDef) -> EventSet {
    EventSet {
        outside_room: matches!(
            kind.params.0.get("destroy_outside_room"),
            Some(ParamValue::Bool(true))
        ),
        ..EventSet::default()
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    param_bool(params, "destroy_outside_room")?;
    Ok(BuiltinState::Killer(KillerState {
        image_speed: param_real(params, "image_speed")?,
        hspeed: param_real(params, "hspeed")?.unwrap_or(Real::ZERO),
        vspeed: param_real(params, "vspeed")?.unwrap_or(Real::ZERO),
        scale: param_real(params, "scale")?,
        visible: param_bool(params, "visible")?,
        depth: param_real(params, "depth")?,
    }))
}

/// Runs one event on a killer.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    match ev {
        EventKind::Create => {
            let Some(BuiltinState::Killer(s)) = state_of(ctx, me) else {
                return Ok(Flow::Continue);
            };
            let s = s.clone();
            let Some(inst) = ctx.w.get_mut(me) else {
                return Ok(Flow::Continue);
            };
            if let Some(v) = s.image_speed {
                inst.img.set_speed(v)?;
            }
            if let Some(v) = s.scale {
                inst.img.set_xscale(v)?;
                inst.img.set_yscale(v)?;
            }
            if let Some(v) = s.visible {
                inst.visible = v;
            }
            if let Some(v) = s.depth {
                inst.depth = v;
            }
            if s.hspeed.get().abs() > 0.0 {
                inst.m.set_hspeed(s.hspeed)?;
            }
            if s.vspeed.get().abs() > 0.0 {
                inst.m.set_vspeed(s.vspeed)?;
            }
            Ok(Flow::Continue)
        }
        EventKind::OutsideRoom => Ok(ctx.destroy(me)),
        _ => Ok(Flow::Continue),
    }
}
