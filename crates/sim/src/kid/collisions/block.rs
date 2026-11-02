//! The `block_standard` collision run: horizontal contact, vertical contact with the landing
//! refresh, diagonal case (docs/plans/sim-core.md 5.3). WP5a owns it.
//!
//! Spec: kid/A1.md "Collision with block (block is solid)". The pipeline has already put the kid
//! back at its previous position (the solid put-back, `events::collision_phase`) and moves it by
//! the remaining speeds afterwards. The three steps run in order and each reads the position and
//! speeds the one before left:
//!
//! 1. `place_free(x + hspeed, y)` false: `move_contact_solid` left (`hspeed <= 0`) or right by
//!    at most `|hspeed|`; `hspeed = 0`.
//! 2. `place_free(x, y + vspeed)` false: `move_contact_solid` up (`vspeed <= 0`) or down
//!    (`vspeed > 0`, which also refreshes the double jump) by at most `|vspeed|`; `vspeed = 0`
//!    and, with `gravity.block_landing_zeroes`, `gravity = 0`.
//! 3. `place_free(x + hspeed, y + vspeed)` false: `hspeed = 0`.
//!
//! The reversed kid of family B (`flip.model = "yflag"`) refreshes the jump on the ceiling side
//! instead (`vspeed <= 0`); xia's flipped kid is WP5c.

use serde::Deserialize;

use crate::collide::move_contact_solid;
use crate::collide::query::{Target, place_free, place_meeting};
use crate::def::ClassMask;
use crate::def::physics::{AirJump, BlockSlopeStep, FlipKind};
use crate::error::SimError;
use crate::kid::actions::common::{state, yflag};
use crate::kid::api::refresh_air_jump;
use crate::num::{Real, gm_ge, gm_gt, gm_le, gm_lt};
use crate::world::{Ctx, Flow, InstId};

/// The collision runs this file implements, by their `physics.toml` names (snake case). Part
/// of `kid::KidCollisionRun`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockRun {
    /// Kid A1 block contact, steps 1-3 above.
    BlockStandard,
}

/// Runs one collision run for the kid `me` against `other`. `kid::run_collision` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, _other: InstId, run: &BlockRun) -> Result<Flow, SimError> {
    match run {
        BlockRun::BlockStandard => block_standard(ctx, me)?,
    }
    Ok(Flow::Continue)
}

fn block_standard(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    check_flip(ctx, me)?;
    horizontal(ctx, me, false)?;
    vertical(ctx, me)?;
    diagonal(ctx, me)
}

/// `block_standard` with the slope step of `[slope] block_step` in the horizontal part
/// (`slope::SlopeRun::BlockSlopeStep`).
pub(crate) fn block_slope_step(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    check_flip(ctx, me)?;
    horizontal(ctx, me, true)?;
    vertical(ctx, me)?;
    diagonal(ctx, me)
}

fn check_flip(ctx: &Ctx, me: InstId) -> Result<(), SimError> {
    // The `yflag` kid (family B) is the same object, mirrored; other flipped kids are WP5c's.
    let rev = yflag(ctx, &state(ctx, me)?) < 0;
    if state(ctx, me)?.flipped && !rev {
        return Err(SimError::unsupported(
            "kid collision block_standard: the flipped kid (5.3 flip rows, WP5b/WP5c)",
        ));
    }
    Ok(())
}

fn pos(ctx: &Ctx, me: InstId) -> Option<(Real, Real, Real, Real)> {
    ctx.w
        .get(me)
        .map(|i| (i.m.x(), i.m.y(), i.m.hspeed(), i.m.vspeed()))
}

fn missing() -> SimError {
    SimError::invariant("block_standard: kid vanished")
}

/// Step 1, horizontal contact. With `slope_step` the `[slope] block_step` rule applies: `free_spot`
/// (rainbow) adds the step-up after the contact when the matching slope is `step` px ahead and the
/// spot `step` up is free; `replace` (rori, fairy-tale) steps up instead of the contact when the
/// shifted kid overlaps the matching slope.
pub(crate) fn horizontal(ctx: &mut Ctx, me: InstId, slope_step: bool) -> Result<(), SimError> {
    let def = ctx.def;
    let n = &ctx.rules.num;
    let rev = yflag(ctx, &state(ctx, me)?) < 0;
    let (x, y, hs, _) = pos(ctx, me).ok_or_else(missing)?;
    if place_free(ctx, me, x + hs, y)? {
        return Ok(());
    }
    let rules = &def.physics.slope;
    let step = rules.step;
    if slope_step && rules.block_step == BlockSlopeStep::Replace {
        let up = if gm_lt(hs, Real::ZERO, n) {
            Some((-step, ClassMask::SLOPE_MIRROR))
        } else if gm_gt(hs, Real::ZERO, n) {
            Some((step, ClassMask::SLOPE))
        } else {
            None
        };
        if let Some((dx, class)) = up
            && place_meeting(ctx, me, x + hs, y, Target::Class(class))?
        {
            let i = ctx.w.get_mut(me).ok_or_else(missing)?;
            i.m.set_x(x + dx)?;
            i.m.set_y(y - step)?;
            i.m.set_hspeed(Real::ZERO)?;
            return Ok(());
        }
    }
    // Family C's flipped kid reads `hspeed` 0 as right (`hspeed >= 0`), everyone else as left.
    let right = if matches!(def.physics.flip.model, FlipKind::GravSign | FlipKind::SwapObject) && rev {
        gm_ge(hs, Real::ZERO, n)
    } else {
        gm_gt(hs, Real::ZERO, n)
    };
    move_contact_solid(
        ctx,
        me,
        Real::new(if right { 0.0 } else { 180.0 }),
        Real::new(hs.get().abs()),
    )?;
    ctx.w.get_mut(me).ok_or_else(missing)?.m.set_hspeed(Real::ZERO)?;
    if slope_step && rules.block_step == BlockSlopeStep::FreeSpot {
        let (x, y, _, _) = pos(ctx, me).ok_or_else(missing)?;
        let (dx, class) = if right {
            (step, ClassMask::SLOPE)
        } else {
            (-step, ClassMask::SLOPE_MIRROR)
        };
        if place_meeting(ctx, me, x + dx, y, Target::Class(class))? && place_free(ctx, me, x + dx, y - step)?
        {
            let i = ctx.w.get_mut(me).ok_or_else(missing)?;
            i.m.set_x(x + dx)?;
            i.m.set_y(y - step)?;
        }
    }
    Ok(())
}

/// Step 2, vertical contact.
pub(crate) fn vertical(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let n = &ctx.rules.num;
    let rev = yflag(ctx, &state(ctx, me)?) < 0;
    let (x, y, _, vs) = pos(ctx, me).ok_or_else(missing)?;
    if !place_free(ctx, me, x, y + vs)? {
        // Landing is the contact in the gravity direction: down normally, up when reversed.
        let up = gm_le(vs, Real::ZERO, n);
        if up {
            move_contact_solid(ctx, me, Real::new(90.0), Real::new(vs.get().abs()))?;
        } else {
            move_contact_solid(ctx, me, Real::new(270.0), vs)?;
        }
        if up == rev {
            refresh_air_jump(ctx, me);
            // Showdown: a landing also spends the triple jump the Create granted.
            if def.physics.jump.air == AirJump::DjumpThenTjump {
                crate::kid::actions::common::state_mut(ctx, me)?.tjump = false;
            }
        }
        let i = ctx.w.get_mut(me).ok_or_else(missing)?;
        i.m.set_vspeed(Real::ZERO)?;
        if def.physics.gravity.block_landing_zeroes {
            i.m.set_gravity(Real::ZERO)?;
        }
    }
    Ok(())
}

/// Step 3, diagonal.
pub(crate) fn diagonal(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let (x, y, hs, vs) = pos(ctx, me).ok_or_else(missing)?;
    if !place_free(ctx, me, x + hs, y + vs)? {
        ctx.w.get_mut(me).ok_or_else(missing)?.m.set_hspeed(Real::ZERO)?;
    }
    Ok(())
}
