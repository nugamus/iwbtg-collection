//! Built-in `bullet_blocker` (mechanics `bullet_blocker`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/bullet_blocker.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! Collision with a kid bullet destroys the bullet. With `except_when_overlapping_kid` (WAN
//! bulletBlocker) it does nothing while it overlaps a kid.

use super::warp::KIDS;
use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, state_of};
use crate::collide::Target;
use crate::collide::query::place_meeting;
use crate::def::{ClassMask, KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

const SPECS: &[ParamSpec] = &[ParamSpec {
    name: "except_when_overlapping_kid",
    ty: ParamType::Bool,
    required: false,
}];

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: a kid bullet touching it.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    set.collisions.push(ClassMask::KID_BULLET);
    set
}

/// A new instance's state: whether it spares bullets while it overlaps the kid.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    Ok(BuiltinState::BulletBlocker {
        except_overlapping: param_bool(params, "except_when_overlapping_kid")?.unwrap_or(false),
    })
}

/// Runs one event on a bullet blocker.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let EventKind::Collision { with, other } = ev else {
        return Ok(Flow::Continue);
    };
    if !with.intersects(ClassMask::KID_BULLET) {
        return Ok(Flow::Continue);
    }
    let Some(BuiltinState::BulletBlocker { except_overlapping }) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    if *except_overlapping {
        let Some(inst) = ctx.w.get(me) else {
            return Ok(Flow::Continue);
        };
        let (x, y) = (inst.m.x(), inst.m.y());
        if place_meeting(ctx, me, x, y, Target::Class(KIDS))? {
            return Ok(Flow::Continue);
        }
    }
    ctx.destroy(*other);
    Ok(Flow::Continue)
}
