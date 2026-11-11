//! Slope collision runs: `block_slope_step`, `slope_por_rise`, `slope_yane` (docs/plans/sim-core.md
//! 5.3). WP5c owns it. Spec: kid/A1.md (poronkusema, get-the-rainbow-feather, rori, fairy-tale),
//! kid/A4.md (eat-the-new-year).

use serde::Deserialize;

use super::block;
use crate::collide::move_contact_solid;
use crate::collide::query::{Target, place_free, place_meeting};
use crate::def::ClassMask;
use crate::error::SimError;
use crate::kid::actions::common::state;
use crate::num::{Real, gm_ge, gm_gt, gm_le, gm_lt};
use crate::world::{Ctx, Flow, InstId};

/// The collision runs this file implements, by their `physics.toml` names (snake case). Part
/// of `kid::KidCollisionRun`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlopeRun {
    /// The block contact with the climb of `[slope] block_step` in its horizontal part. Used by the
    /// `block`, `slope` and `slope_mirror` rows of rainbow, rori and fairy-tale (kid/A1.md).
    BlockSlopeStep,
    /// Poronkusema, Collision with nanameblock: the vertical block part; a rise found by testing
    /// 0 to 9 px up with the kid shifted by `hspeed` (0 when still inside a slope at 9, or when the
    /// found spot overlaps a block); then `x` moves `step` in the direction of `hspeed` (even with
    /// rise 0), `y` goes up by the rise, `hspeed` 0 and the diagonal check.
    SlopePorRise,
    /// Eat-the-new-year, Collision with yaneH (`slope`) or yaneM (`slope_mirror`), kid/A4.md:
    /// 1. when `place_free(x + hspeed, y)` is false: uphill (H `hspeed >= 0`, M `<= 0`) with
    ///    `place_free(x, y + vspeed)` false, the kid moves `step` up and sideways; H downhill
    ///    (`hspeed < 0`) does nothing (the original's contact is commented out); M downhill
    ///    (`hspeed > 0`) contacts toward 180 (left). Then `hspeed` 0.
    /// 2. and 3. the vertical and diagonal block parts.
    /// 4. with a slope of the event's own class 1 px below and the Step's `h` pointing downhill
    ///    (H `h < 0`, M `h > 0`), the kid moves `step` down and sideways; `hspeed` 0.
    SlopeYane,
}

/// Runs one collision run for the kid `me` against `other`. `kid::run_collision` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, other: InstId, run: &SlopeRun) -> Result<Flow, SimError> {
    match run {
        SlopeRun::BlockSlopeStep => block::block_slope_step(ctx, me, other)?,
        SlopeRun::SlopePorRise => por_rise(ctx, me)?,
        SlopeRun::SlopeYane => yane(ctx, me, other)?,
    }
    Ok(Flow::Continue)
}

fn missing() -> SimError {
    SimError::invariant("slope run: kid vanished")
}

fn por_rise(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let step = ctx.def.physics.slope.step;
    let n = &ctx.rules.num;
    block::vertical(ctx, me, true)?;
    let i = ctx.w.get(me).ok_or_else(missing)?;
    let (x, y, hs) = (i.m.x(), i.m.y(), i.m.hspeed());
    let slope = Target::Class(ClassMask::SLOPE);
    let mut rise = None;
    for off in 0..=9 {
        let dy = Real::new(f64::from(off));
        if !place_meeting(ctx, me, x + hs, y - dy, slope)? {
            rise = Some(dy);
            break;
        }
    }
    let mut rise = rise.unwrap_or(Real::ZERO);
    if place_meeting(ctx, me, x + hs, y - rise, Target::Class(ClassMask::BLOCK))? {
        rise = Real::ZERO;
    }
    let dx = if gm_gt(hs, Real::ZERO, n) {
        step
    } else if gm_lt(hs, Real::ZERO, n) {
        -step
    } else {
        Real::ZERO
    };
    let i = ctx.w.get_mut(me).ok_or_else(missing)?;
    i.m.set_x(x + dx)?;
    i.m.set_y(y - rise)?;
    i.m.set_hspeed(Real::ZERO)?;
    block::diagonal(ctx, me)
}

fn yane(ctx: &mut Ctx, me: InstId, other: InstId) -> Result<(), SimError> {
    let step = ctx.def.physics.slope.step;
    let n = &ctx.rules.num;
    let mirror = ctx
        .w
        .get(other)
        .is_some_and(|o| o.classes.intersects(ClassMask::SLOPE_MIRROR));
    // H rises to the right, M to the left: `side` is the uphill direction.
    let (side, own) = if mirror {
        (-step, ClassMask::SLOPE_MIRROR)
    } else {
        (step, ClassMask::SLOPE)
    };
    let i = ctx.w.get(me).ok_or_else(missing)?;
    let (x, y, hs, vs) = (i.m.x(), i.m.y(), i.m.hspeed(), i.m.vspeed());
    if !place_free(ctx, me, x + hs, y)? {
        let uphill = if mirror {
            gm_le(hs, Real::ZERO, n)
        } else {
            gm_ge(hs, Real::ZERO, n)
        };
        if uphill {
            if !place_free(ctx, me, x, y + vs)? {
                let i = ctx.w.get_mut(me).ok_or_else(missing)?;
                i.m.set_x(x + side)?;
                i.m.set_y(y - step)?;
            }
        } else if mirror {
            // yaneM's downhill hit contacts toward 180 although the kid moves right (kid/A4.md;
            // yaneH's matching contact is commented out, so H does nothing here).
            move_contact_solid(ctx, me, Real::new(180.0), Real::new(hs.get().abs()))?;
        }
        ctx.w.get_mut(me).ok_or_else(missing)?.m.set_hspeed(Real::ZERO)?;
    }
    block::vertical(ctx, me, true)?;
    block::diagonal(ctx, me)?;
    let h = state(ctx, me)?.ext.walk_h;
    let downhill = if mirror { h > 0 } else { h < 0 };
    let i = ctx.w.get(me).ok_or_else(missing)?;
    let (x, y) = (i.m.x(), i.m.y());
    if downhill && place_meeting(ctx, me, x, y + Real::ONE, Target::Class(own))? {
        let i = ctx.w.get_mut(me).ok_or_else(missing)?;
        i.m.set_x(x - side)?;
        i.m.set_y(y + step)?;
        i.m.set_hspeed(Real::ZERO)?;
    }
    Ok(())
}
