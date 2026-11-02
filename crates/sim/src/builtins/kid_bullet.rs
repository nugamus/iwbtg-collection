//! Built-in `kid_bullet` (the kid's bullet; kid/A1.md "Shooting" and the per-game bullet notes;
//! docs/plans/sim-core.md 6.1; docs/format/builtins/kid_bullet.md). Tier 1. WP0 creates the
//! file; WP6a owns it.
//!
//! - Create arms alarm 0 with `life` (40); Alarm 0 destroys the bullet.
//! - The shooter (the kid's shoot action) sets `hspeed` after Create (16 times the facing), so
//!   the bullet's own Step in its creation tick already sees it (Confirmed (recorder) rely
//!   `a1-bullet-b.csv` f20: a bullet created in the kid's Step runs its Step the same tick, then
//!   moves).
//! - `block_check` is how a block stops it (the games differ):
//!   - `before_move` (rely, A1): Step looks at `(x + hspeed, y)` before the move; a plain block
//!     there destroys the bullet, a platform (class `PLATFORM`) only when visible.
//!   - `collision_destroy` (RBF, FT, RORI, most A2/A3): a Collision with `BLOCK` destroys it
//!     (invisible blocks and platforms too).
//!   - `stop_and_die` (poronkusema): when a block lies at `(x + hspeed, y)`, the bullet moves up
//!     to contact, stops (speed 0) and is destroyed on its next Step. OPEN: the contact-move
//!     rule at fractional positions.

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_enum, param_real, state_mut, state_of};
use crate::alarm;
use crate::collide::Target;
use crate::collide::query::{place_hits_ids, place_meeting};
use crate::def::{ClassMask, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

const CHECKS: &[&str] = &["before_move", "collision_destroy", "stop_and_die"];

const SPECS: &[ParamSpec] = &[
    ParamSpec { name: "life", ty: ParamType::Real, required: false },
    ParamSpec { name: "block_check", ty: ParamType::Enum(CHECKS), required: false },
];

/// How a block stops the bullet.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BlockCheck {
    /// Step looks one move ahead.
    BeforeMove,
    /// A collision event destroys it.
    CollisionDestroy,
    /// It stops at the block and dies next Step.
    StopAndDie,
}

/// A bullet's settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KidBulletState {
    /// Alarm 0 at Create: the lifetime in ticks.
    pub life: Real,
    /// The block rule.
    pub block_check: BlockCheck,
    /// `stop_and_die`: stopped, destroyed on the next Step.
    pub dying: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

fn check_of(name: Option<&str>) -> BlockCheck {
    match name {
        Some("collision_destroy") => BlockCheck::CollisionDestroy,
        Some("stop_and_die") => BlockCheck::StopAndDie,
        _ => BlockCheck::BeforeMove,
    }
}

/// What a kind with this base handles: alarm 0, and the Step check or the block collision.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        alarms: 1,
        ..EventSet::default()
    };
    let name = match kind.params.0.get("block_check") {
        Some(ParamValue::Str(s)) => Some(s.as_str()),
        _ => None,
    };
    match check_of(name) {
        BlockCheck::CollisionDestroy => set.collisions.push(ClassMask::BLOCK),
        BlockCheck::BeforeMove | BlockCheck::StopAndDie => set.step = true,
    }
    set
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    Ok(BuiltinState::KidBullet(KidBulletState {
        life: param_real(params, "life")?.unwrap_or(Real::new(40.0)),
        block_check: check_of(param_enum(params, "block_check", CHECKS)?),
        dying: false,
    }))
}

/// A block (or visible platform) the bullet would hit at `(x, y)`: plain blocks always, a
/// platform only when visible (kid/A1.md Inferred, parent semantics).
fn blocked_at(ctx: &Ctx, me: InstId, x: Real, y: Real) -> Result<bool, SimError> {
    for id in place_hits_ids(ctx, me, x, y, Target::Class(ClassMask::BLOCK))? {
        if let Some(i) = ctx.w.get(id)
            && (!i.classes.intersects(ClassMask::PLATFORM) || i.visible)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Runs one event on a bullet.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::KidBullet(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            let rules = &ctx.rules.num;
            if let Some(inst) = ctx.w.get_mut(me) {
                alarm::set(inst, 0, s.life, rules)?;
            }
            Ok(Flow::Continue)
        }
        EventKind::Alarm(0) => Ok(ctx.destroy(me)),
        EventKind::Collision { with, .. }
            if s.block_check == BlockCheck::CollisionDestroy && with.intersects(ClassMask::BLOCK) =>
        {
            Ok(ctx.destroy(me))
        }
        EventKind::Step => step(ctx, me, &s),
        _ => Ok(Flow::Continue),
    }
}

fn step(ctx: &mut Ctx, me: InstId, s: &KidBulletState) -> Result<Flow, SimError> {
    let Some(inst) = ctx.w.get(me) else {
        return Ok(Flow::Continue);
    };
    let (x, y, h) = (inst.m.x(), inst.m.y(), inst.m.hspeed());
    match s.block_check {
        BlockCheck::BeforeMove => {
            if blocked_at(ctx, me, x + h, y)? {
                return Ok(ctx.destroy(me));
            }
        }
        BlockCheck::StopAndDie => {
            if s.dying {
                return Ok(ctx.destroy(me));
            }
            if blocked_at(ctx, me, x + h, y)? {
                move_to_contact(ctx, me)?;
                if let Some(inst) = ctx.w.get_mut(me) {
                    inst.m.set_hspeed(Real::ZERO)?;
                }
                if let Some(BuiltinState::KidBullet(st)) = state_mut(ctx, me) {
                    st.dying = true;
                }
            }
        }
        BlockCheck::CollisionDestroy => {}
    }
    Ok(Flow::Continue)
}

/// Moves one pixel at a time in the direction of `hspeed` until the next pixel is blocked, at
/// most `|hspeed|` pixels (GM `move_contact_solid`, simplified to the horizontal case).
fn move_to_contact(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let Some(inst) = ctx.w.get(me) else {
        return Ok(());
    };
    let (mut x, y, h) = (inst.m.x(), inst.m.y(), inst.m.hspeed());
    let dir = if h.get() < 0.0 { -1.0 } else { 1.0 };
    let mut left = h.get().abs();
    while left >= 1.0 && !place_meeting(ctx, me, x + Real::new(dir), y, Target::Class(ClassMask::BLOCK))? {
        x += Real::new(dir);
        left -= 1.0;
    }
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.m.set_x(x)?;
    }
    Ok(())
}
