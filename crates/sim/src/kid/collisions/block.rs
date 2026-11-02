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
//! The flipped kid's mirrored contact (ceiling refresh, xia's hspeed 0) is WP5b and WP5c.

use serde::Deserialize;

use crate::collide::{move_contact_solid, query::place_free};
use crate::error::SimError;
use crate::kid::actions::common::state;
use crate::kid::api::refresh_air_jump;
use crate::num::{Real, gm_gt, gm_le};
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
    let def = ctx.def;
    let n = &ctx.rules.num;
    if state(ctx, me)?.flipped {
        return Err(SimError::unsupported(
            "kid collision block_standard: the flipped kid (5.3 flip rows, WP5b/WP5c)",
        ));
    }
    let pos = |ctx: &Ctx| {
        ctx.w
            .get(me)
            .map(|i| (i.m.x(), i.m.y(), i.m.hspeed(), i.m.vspeed()))
    };
    let missing = || SimError::invariant("block_standard: kid vanished");

    // 1. Horizontal contact.
    let (x, y, hs, _) = pos(ctx).ok_or_else(missing)?;
    if !place_free(ctx, me, x + hs, y)? {
        let right = gm_gt(hs, Real::ZERO, n);
        move_contact_solid(
            ctx,
            me,
            Real::new(if right { 0.0 } else { 180.0 }),
            Real::new(hs.get().abs()),
        )?;
        ctx.w.get_mut(me).ok_or_else(missing)?.m.set_hspeed(Real::ZERO)?;
    }
    // 2. Vertical contact.
    let (x, y, _, vs) = pos(ctx).ok_or_else(missing)?;
    if !place_free(ctx, me, x, y + vs)? {
        if gm_le(vs, Real::ZERO, n) {
            move_contact_solid(ctx, me, Real::new(90.0), Real::new(vs.get().abs()))?;
        } else {
            move_contact_solid(ctx, me, Real::new(270.0), vs)?;
            refresh_air_jump(ctx, me);
        }
        let i = ctx.w.get_mut(me).ok_or_else(missing)?;
        i.m.set_vspeed(Real::ZERO)?;
        if def.physics.gravity.block_landing_zeroes {
            i.m.set_gravity(Real::ZERO)?;
        }
    }
    // 3. Diagonal.
    let (x, y, hs, vs) = pos(ctx).ok_or_else(missing)?;
    if !place_free(ctx, me, x + hs, y + vs)? {
        ctx.w.get_mut(me).ok_or_else(missing)?.m.set_hspeed(Real::ZERO)?;
    }
    Ok(())
}
