//! Slope collision runs: `block_slope_step`, `slope_por_rise`, `slope_yane` (docs/plans/sim-core.md
//! 5.3). WP5c owns it. Spec: kid/A1.md (poronkusema, get-the-rainbow-feather, rori, fairy-tale),
//! kid/A4.md (eat-the-new-year).

use serde::Deserialize;

use super::block;
use crate::collide::query::{Target, place_free, place_meeting};
use crate::def::ClassMask;
use crate::error::SimError;
use crate::num::{Real, gm_ge, gm_gt, gm_le, gm_lt};
use crate::world::{Ctx, Flow, InstId};

/// The collision runs this file implements, by their `physics.toml` names (snake case). Part
/// of `kid::KidCollisionRun`.
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
    /// Eat-the-new-year, Collision with yaneH (`slope`) or yaneM (`slope_mirror`): when
    /// `place_free(x + hspeed, y)` is false and `hspeed >= 0` (H; `<= 0` for M, OPEN: mirrored) and
    /// `place_free(x, y + vspeed)` is false, the kid moves `step` up and sideways; `hspeed` 0.
    /// Then the vertical and diagonal block parts.
    SlopeYane,
}

/// Runs one collision run for the kid `me` against `other`. `kid::run_collision` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, other: InstId, run: &SlopeRun) -> Result<Flow, SimError> {
    match run {
        SlopeRun::BlockSlopeStep => block::block_slope_step(ctx, me)?,
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
    block::vertical(ctx, me)?;
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
    let i = ctx.w.get(me).ok_or_else(missing)?;
    let (x, y, hs, vs) = (i.m.x(), i.m.y(), i.m.hspeed(), i.m.vspeed());
    if !place_free(ctx, me, x + hs, y)? {
        let toward = if mirror {
            gm_le(hs, Real::ZERO, n)
        } else {
            gm_ge(hs, Real::ZERO, n)
        };
        if toward && !place_free(ctx, me, x, y + vs)? {
            let dx = if mirror { -step } else { step };
            let i = ctx.w.get_mut(me).ok_or_else(missing)?;
            i.m.set_x(x + dx)?;
            i.m.set_y(y - step)?;
        }
        ctx.w.get_mut(me).ok_or_else(missing)?.m.set_hspeed(Real::ZERO)?;
    }
    block::vertical(ctx, me)?;
    block::diagonal(ctx, me)
}
