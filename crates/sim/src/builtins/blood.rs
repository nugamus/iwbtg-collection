//! Built-in `blood` (a blood drop; CLONE_SPEC numbers "Hidden RNG consumers", kid/A1.md Death,
//! kid/A3.md blood; docs/plans/sim-core.md 6.1; docs/format/builtins/blood.md). Tier 1. WP0
//! creates the file; WP6a owns it.
//!
//! Create draws from the gameplay RNG, in this order (Confirmed (recorder) rely `num-rng-b.csv`,
//! all 40 blood of frame 15 bit for bit): the frame, `choose(0, 1, 2)`, then the gravity,
//! `0.1 + random(0.2)` (`-0.1 + random(-0.2)` when `flipped`). `image_speed` is 0, so blood never
//! animates. The emitter sets direction and speed after Create (`blood_emitter`).
//!
//! Outside Room destroys it (A1, A3). With `sticks` (default true; A2, A3 and rely: blood is a
//! child of nothing, the block is the one that sticks), a Collision with a block whose
//! `blood_sticks` is on moves the drop to contact and sets its speed and gravity to 0 (kid/A3.md
//! blood). OPEN: the contact rule at fractional positions; this steps back to the previous
//! position and then forward pixel by pixel along the motion while the next pixel is free.

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, state_of};
use crate::collide::Target;
use crate::collide::query::place_meeting;
use crate::def::{ClassMask, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "flipped",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "sticks",
        ty: ParamType::Bool,
        required: false,
    },
];

/// A blood drop's settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BloodState {
    /// The kid was flipped: gravity is negative.
    pub flipped: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: Outside Room, and the block collision with `sticks`.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        outside_room: true,
        ..EventSet::default()
    };
    if !matches!(kind.params.0.get("sticks"), Some(ParamValue::Bool(false))) {
        set.collisions.push(ClassMask::BLOCK);
    }
    set
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    param_bool(params, "sticks")?;
    Ok(BuiltinState::Blood(BloodState {
        flipped: param_bool(params, "flipped")?.unwrap_or(false),
    }))
}

/// Runs one event on a blood drop.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Blood(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let flipped = s.flipped;
    match ev {
        EventKind::Create => {
            // The order is the recorded one: the frame first, then the gravity.
            let frame = ctx.w.rng.choose(3)?;
            let (base, spread) = if flipped { (-0.1, -0.2) } else { (0.1, 0.2) };
            let g = Real::new(base) + ctx.w.rng.random(Real::new(spread));
            if let Some(inst) = ctx.w.get_mut(me) {
                inst.img.set_index(Real::new(f64::from(frame)))?;
                inst.img.set_speed(Real::ZERO)?;
                inst.m.set_gravity(g)?;
            }
            Ok(Flow::Continue)
        }
        EventKind::OutsideRoom => Ok(ctx.destroy(me)),
        EventKind::Collision { with, other } if with.intersects(ClassMask::BLOCK) => {
            if super::block::blood_sticks(ctx.w, *other) {
                stick(ctx, me)?;
            }
            Ok(Flow::Continue)
        }
        _ => Ok(Flow::Continue),
    }
}

/// Moves to contact with the block it hit and stops (speed and gravity 0).
fn stick(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let Some(inst) = ctx.w.get(me) else {
        return Ok(());
    };
    let (px, py) = (inst.m.xprevious(), inst.m.yprevious());
    let (h, v) = (inst.m.hspeed(), inst.m.vspeed());
    let (mut x, mut y) = (px, py);
    let (dx, dy) = (h.get(), v.get());
    let len = (dx * dx + dy * dy).sqrt();
    if len > 0.0 {
        let (sx, sy) = (Real::new(dx / len), Real::new(dy / len));
        let mut left = len;
        while left >= 1.0 && !place_meeting(ctx, me, x + sx, y + sy, Target::Class(ClassMask::BLOCK))? {
            x += sx;
            y += sy;
            left -= 1.0;
        }
    }
    if let Some(inst) = ctx.w.get_mut(me) {
        inst.m.set_x(x)?;
        inst.m.set_y(y)?;
        inst.m.set_hspeed(Real::ZERO)?;
        inst.m.set_vspeed(Real::ZERO)?;
        inst.m.set_gravity(Real::ZERO)?;
    }
    Ok(())
}
