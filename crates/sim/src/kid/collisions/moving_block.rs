//! The poronkusema `moving_block_por` collision run (docs/plans/sim-core.md 5.3, tier 3). WP5c
//! owns it. Spec: kid/A1.md poronkusema, "Collision with movingBlock" (64 x 64, a non-solid child
//! of block; the game places none, so every reading below is from the event code only).

use serde::Deserialize;

use crate::error::SimError;
use crate::kid::api::{refresh_air_jump, set_on_platform};
use crate::num::{Real, gm_ge, gm_gt, gm_le, gm_lt};
use crate::world::{Ctx, Flow, InstId};

/// The collision runs this file implements, by their `physics.toml` names (snake case). Part
/// of `kid::KidCollisionRun`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovingBlockRun {
    /// Poronkusema, Collision with movingBlock, in this order:
    /// - ride from above: `y <= other.y + vspeed + 2`, the kid moving down relative to the block
    ///   (`vspeed > other.vspeed`) and `other.y - yprevious >= 8`: `y = other.y - 9`, `vspeed` the
    ///   block's, `onPlatform` and `djump` set;
    /// - bump from below: `yprevious - other.y >= height + 10`: `y = other.y + height + 12` and
    ///   `vspeed` the block's when that is 0 or more, else 0;
    /// - else pushed out sideways: `x = other.x - 6` or `other.x + width + 5`, whichever side of
    ///   the block's middle the kid is on.
    ///
    /// OPEN (the spec gives the numbers, not every test): the "within `vspeed + 2`" reading, the
    /// relative-motion test and the choice of side (#314).
    MovingBlockPor,
}

/// Runs one collision run for the kid `me` against `other`. `kid::run_collision` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, other: InstId, run: &MovingBlockRun) -> Result<Flow, SimError> {
    match run {
        MovingBlockRun::MovingBlockPor => moving_block_por(ctx, me, other)?,
    }
    Ok(Flow::Continue)
}

fn moving_block_por(ctx: &mut Ctx, me: InstId, other: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let n = &ctx.rules.num;
    let missing = || SimError::invariant("moving_block_por: kid or block vanished");
    let k = ctx.w.get(me).ok_or_else(missing)?;
    let (kx, ky, kvs, kyp) = (k.m.x(), k.m.y(), k.m.vspeed(), k.m.yprevious());
    let o = ctx.w.get(other).ok_or_else(missing)?;
    let (ox, oy, ovs) = (o.m.x(), o.m.y(), o.m.vspeed());
    let (w, h) = def
        .kind(o.kind)
        .and_then(|k| k.sprite)
        .and_then(|s| s.index())
        .and_then(|i| def.sprites.get(i))
        .map_or((64, 64), |s| (s.size.0, s.size.1));
    let (w, h) = (Real::new(f64::from(w)), Real::new(f64::from(h)));
    if gm_le(ky, oy + kvs + Real::new(2.0), n) && gm_gt(kvs, ovs, n) && gm_ge(oy - kyp, Real::new(8.0), n) {
        let i = ctx.w.get_mut(me).ok_or_else(missing)?;
        i.m.set_y(oy - Real::new(9.0))?;
        i.m.set_vspeed(ovs)?;
        set_on_platform(ctx, me, true);
        refresh_air_jump(ctx, me);
    } else if gm_ge(kyp - oy, h + Real::new(10.0), n) {
        let i = ctx.w.get_mut(me).ok_or_else(missing)?;
        i.m.set_y(oy + h + Real::new(12.0))?;
        i.m.set_vspeed(if gm_ge(ovs, Real::ZERO, n) {
            ovs
        } else {
            Real::ZERO
        })?;
    } else {
        let i = ctx.w.get_mut(me).ok_or_else(missing)?;
        let left = gm_lt(kx, ox + w * Real::new(0.5), n);
        i.m.set_x(if left {
            ox - Real::new(6.0)
        } else {
            ox + w + Real::new(5.0)
        })?;
    }
    Ok(())
}
