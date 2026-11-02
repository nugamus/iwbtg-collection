//! Built-in `block` (mechanics `block`; docs/plans/sim-core.md 6.1; docs/format/builtins/block.md).
//! Tier 1. WP0 creates the file; WP6a owns it.
//!
//! A block has no behavior: the kid's collision handlers do the work. Create applies the
//! per-instance overrides (`solid`, `visible`, `depth`) and, with `visual_spawns_collider`
//! (rely objBLOCK, CR1), creates the plain collider block at its own position. The mask
//! variants (`mask`) are data the loader resolves to the kind's mask; the sim does not read
//! them. `kid_collides` is a class choice the loader makes (the block drops `ClassMask::BLOCK`
//! when it is false); the sim does not read it either.

use serde::{Deserialize, Serialize};

use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_kind, param_real, state_of,
};
use crate::def::{KindDef, KindId, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId, SpawnInit, World};

const MASKS: &[&str] = &[
    "full32",
    "mini16",
    "large64",
    "line_h",
    "line_v",
    "thin32x3",
    "precise_sprite",
];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "mask",
        ty: ParamType::Enum(MASKS),
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
        name: "solid",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "kid_collides",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "blood_sticks",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "slip",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "visual_spawns_collider",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "collider",
        ty: ParamType::Kind,
        required: false,
    },
];

/// A block's per-instance settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BlockState {
    /// `solid` override (the kind's default otherwise).
    pub solid: Option<bool>,
    /// `visible` override.
    pub visible: Option<bool>,
    /// `depth` override.
    pub depth: Option<Real>,
    /// Slip value: 0 is not ice (LT, RBF 0.2, KOT 0.4). The kid's ice action reads it through
    /// [`slip_of`].
    pub slip: Real,
    /// Blood sticks to this block (default true); `blood` reads it through [`blood_sticks`].
    pub blood_sticks: bool,
    /// The kind Create spawns at the block's position, when `visual_spawns_collider`.
    pub collider: Option<KindId>,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: nothing (Create always runs).
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet::default()
}

/// A new instance's state from its params. `visual_spawns_collider` needs `collider`.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    param_enum(params, "mask", MASKS)?;
    param_bool(params, "kid_collides")?;
    let spawns = param_bool(params, "visual_spawns_collider")?.unwrap_or(false);
    let collider = param_kind(params, "collider")?;
    if spawns && collider.is_none() {
        return Err(super::param_error(
            "collider",
            "visual_spawns_collider needs `collider`, the kind of the plain block to create",
        ));
    }
    Ok(BuiltinState::Block(BlockState {
        solid: param_bool(params, "solid")?,
        visible: param_bool(params, "visible")?,
        depth: param_real(params, "depth")?,
        slip: param_real(params, "slip")?.unwrap_or(Real::ZERO),
        blood_sticks: param_bool(params, "blood_sticks")?.unwrap_or(true),
        collider: if spawns { collider } else { None },
    }))
}

/// Runs one event on a block: only Create does anything.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    if *ev != EventKind::Create {
        return Ok(Flow::Continue);
    }
    let Some(BuiltinState::Block(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    let Some(inst) = ctx.w.get_mut(me) else {
        return Ok(Flow::Continue);
    };
    if let Some(v) = s.solid {
        inst.solid = v;
    }
    if let Some(v) = s.visible {
        inst.visible = v;
    }
    if let Some(v) = s.depth {
        inst.depth = v;
    }
    let (x, y) = (inst.m.x(), inst.m.y());
    if let Some(kind) = s.collider {
        ctx.spawn(kind, x, y, SpawnInit::default())?;
    }
    Ok(Flow::Continue)
}

/// The slip value of a block (0 for anything else): what the kid's ice action reads.
pub fn slip_of(w: &World, id: InstId) -> Real {
    match w.get(id).map(|i| &i.state) {
        Some(crate::world::KindState::Builtin(BuiltinState::Block(s))) => s.slip,
        _ => Real::ZERO,
    }
}

/// Whether blood sticks to this instance (true for anything but a block that says no).
pub fn blood_sticks(w: &World, id: InstId) -> bool {
    match w.get(id).map(|i| &i.state) {
        Some(crate::world::KindState::Builtin(BuiltinState::Block(s))) => s.blood_sticks,
        _ => true,
    }
}
