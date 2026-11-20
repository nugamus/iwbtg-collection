//! Built-in `block` (mechanics `block`; docs/architecture/sim/builtins.md; docs/format/builtins/block.md).
//! Tier 1. WP0 creates the file; WP6a owns it.
//!
//! A block has no behavior: the kid's collision handlers do the work. Create applies the
//! per-instance overrides (`solid`, `visible`, `depth`) and, with `visual_spawns_collider`
//! (rely objBLOCK, CR1), creates the plain collider block at its own position. The mask
//! variants (`mask`) are data the loader resolves to the kind's mask; the sim does not read
//! them. `kid_collides` is a class choice the loader makes (the block drops `ClassMask::BLOCK`
//! when it is false); the sim does not read it either.
//!
//! `proximity_frame` (LEM block02; research `specs/mechanics.md` builtin.block) is read from the
//! kind's params, as its event set is: Create sets `image_speed` 0, and each Step shows frame 1
//! while the kid's origin is inside the box -8 < dx < 40, -16 < dy < 40 around the block's
//! origin (all strict), frame 0 otherwise or with no kid. Visual only: the mask stays.

use serde::{Deserialize, Serialize};

use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_kind, param_real, state_of,
};
use crate::def::{KindDef, KindId, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::{self, Real};
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
    ParamSpec {
        name: "proximity_frame",
        ty: ParamType::Bool,
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

fn proximity_frame(params: &Params) -> bool {
    matches!(params.0.get("proximity_frame"), Some(ParamValue::Bool(true)))
}

/// What a kind with this base handles: Step with `proximity_frame`, nothing else (Create
/// always runs).
pub fn event_set(kind: &KindDef) -> EventSet {
    EventSet {
        step: proximity_frame(&kind.params),
        ..EventSet::default()
    }
}

/// Whether this instance's kind sets `proximity_frame` (the kind's params, as its event set).
fn kind_proximity_frame(ctx: &Ctx, me: InstId) -> bool {
    ctx.w
        .get(me)
        .is_some_and(|i| kind_proximity_frame_of(ctx.def, i.kind))
}

/// A new instance's state from its params. `visual_spawns_collider` needs `collider`.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    param_enum(params, "mask", MASKS)?;
    param_bool(params, "kid_collides")?;
    param_bool(params, "proximity_frame")?;
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

/// Runs one event on a block: Create, and Step with `proximity_frame`.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    if *ev == EventKind::Step {
        if kind_proximity_frame(ctx, me) {
            step_proximity_frame(ctx, me)?;
        }
        return Ok(Flow::Continue);
    }
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
    if kind_proximity_frame_of(ctx.def, inst.kind) {
        inst.img.set_speed(Real::ZERO)?;
    }
    let (x, y) = (inst.m.x(), inst.m.y());
    if let Some(kind) = s.collider {
        ctx.spawn(kind, x, y, SpawnInit::default())?;
    }
    Ok(Flow::Continue)
}

fn kind_proximity_frame_of(def: &crate::def::GameDef, kind: KindId) -> bool {
    kind.index()
        .and_then(|k| def.kinds.get(k))
        .is_some_and(|k| proximity_frame(&k.params))
}

/// block02's Step (research `specs/mechanics.md` builtin.block): frame 1 while the kid's
/// origin is in the box around the block's origin, frame 0 otherwise or with no kid.
fn step_proximity_frame(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let Some(inst) = ctx.w.get(me) else {
        return Ok(());
    };
    let (bx, by) = (inst.m.x(), inst.m.y());
    let near = ctx
        .w
        .kid(&ctx.rules.tick)
        .and_then(|k| ctx.w.get(k))
        .is_some_and(|kid| {
            let (dx, dy) = (kid.m.x() - bx, kid.m.y() - by);
            let r = &ctx.rules.num;
            num::gm_lt(Real::new(-8.0), dx, r)
                && num::gm_lt(dx, Real::new(40.0), r)
                && num::gm_lt(Real::new(-16.0), dy, r)
                && num::gm_lt(dy, Real::new(40.0), r)
        });
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.img.set_index(if near { Real::ONE } else { Real::ZERO })?;
    }
    Ok(())
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
