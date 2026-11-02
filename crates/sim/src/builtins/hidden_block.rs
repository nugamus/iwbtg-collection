//! Built-in `hidden_block` (mechanics `hidden_block`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/hidden_block.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! A solid block that is invisible until a kid comes near (KOT blockNotVis, WAN blockInvisible,
//! XIA objBlockInvis). Create makes it invisible. While invisible, each Step: when the distance
//! to either kid is below `reveal_distance` (2; XIA 1) it becomes visible (the sound is the
//! client's); when no kid exists and `destroy_when_no_kid` (default true) it destroys itself.
//! `stops_bullets` destroys a kid bullet that touches it (WAN).
//!
//! Solidity and the mask are the loader's (kind data). OPEN, not done: XIA objBlockInvis is also
//! destroyed when shot, or when touched by blood while invisible (KID-C); TR-MECH-09 (the
//! reveal tick and whether the kid stops in the same tick).

use super::warp::KIDS;
use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_real};
use crate::collide::Target;
use crate::collide::query::distance_to_object;
use crate::def::{ClassMask, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::{self, Real};
use crate::world::{Ctx, Flow, InstId};

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "reveal_distance",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "sound",
        ty: ParamType::Sound,
        required: false,
    },
    ParamSpec {
        name: "stops_bullets",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "destroy_when_no_kid",
        ty: ParamType::Bool,
        required: false,
    },
];

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Step, and kid bullets with `stops_bullets`.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        step: true,
        ..EventSet::default()
    };
    if matches!(kind.params.0.get("stops_bullets"), Some(ParamValue::Bool(true))) {
        set.collisions.push(ClassMask::KID_BULLET);
    }
    set
}

/// A new instance's state: none, the params are checked.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    param_real(params, "reveal_distance")?;
    param_bool(params, "stops_bullets")?;
    param_bool(params, "destroy_when_no_kid")?;
    Ok(BuiltinState::Stateless)
}

fn params_of<'a>(ctx: &'a Ctx, me: InstId) -> Option<&'a Params> {
    let kind = ctx.w.get(me)?.kind;
    ctx.def.kinds.get(kind.index()?).map(|k| &k.params)
}

/// Runs one event on a hidden block.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    match ev {
        EventKind::Create => {
            if let Some(inst) = ctx.w.get_mut(me) {
                inst.visible = false;
            }
        }
        EventKind::Step => step(ctx, me)?,
        EventKind::Collision { with, other } if with.intersects(ClassMask::KID_BULLET) => {
            ctx.destroy(*other);
        }
        _ => {}
    }
    Ok(Flow::Continue)
}

fn step(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    if ctx.w.get(me).is_none_or(|i| i.visible) {
        return Ok(());
    }
    let (reveal, destroy_when_none) = match params_of(ctx, me) {
        Some(p) => (
            param_real(p, "reveal_distance")
                .ok()
                .flatten()
                .unwrap_or(Real::new(2.0)),
            param_bool(p, "destroy_when_no_kid")
                .ok()
                .flatten()
                .unwrap_or(true),
        ),
        None => (Real::new(2.0), true),
    };
    match distance_to_object(ctx, me, Target::Class(KIDS))? {
        Some(d) => {
            if num::gm_lt(d, reveal, &ctx.rules.num)
                && let Some(inst) = ctx.w.get_mut(me)
            {
                inst.visible = true;
            }
        }
        None => {
            if destroy_when_none {
                ctx.destroy(me);
            }
        }
    }
    Ok(())
}
