//! Gravity flip: the `[flip]` models (docs/plans/sim-core.md 5.3). WP5c owns it. Family B's
//! `player_reverse` is in `b.rs`; the group A object swap (`flip_swap`) is WP5c.4 (#131).

use serde::Deserialize;

use super::common::state_mut;
use crate::def::EngineKind;
use crate::error::SimError;
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId, SpawnInit};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlipAction {
    /// `scrFlipGrav` (family C, `flip.model = "grav_sign"`): `grav` negates, `djump` 1, `vspeed` 0,
    /// gravity takes the new sign, `image_yscale` follows (the mask mirrors with it) and `y` moves
    /// 4 times the new `grav`. Also `kid::api::flip_grav`, for the gravity arrows.
    FlipGrav,
    /// Group A Step (`flip.model = "swap_object"`, A1 Step action 5): when `global.grav` no longer
    /// matches this kid object, the facing goes to `global.image_xscale`, the other kid object
    /// is created at `y - 4` (to the flipped object) or `y + 4` (back to the normal one), and this
    /// instance is destroyed. The new kid starts from its Create: speeds 0, `djump` true.
    FlipSwap,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &FlipAction) -> Result<Flow, SimError> {
    match action {
        FlipAction::FlipGrav => flip_grav(ctx, me)?,
        FlipAction::FlipSwap => return flip_swap(ctx, me),
    }
    Ok(Flow::Continue)
}

/// `scrFlipGrav` on the kid `me`.
pub(crate) fn flip_grav(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let g = ctx.def.physics.numbers.gravity;
    ctx.w.engine.kid.grav = !ctx.w.engine.kid.grav;
    let new = {
        let s = state_mut(ctx, me)?;
        s.yflag = -s.yflag;
        s.flipped = s.yflag < 0;
        s.djump = true;
        s.yflag
    };
    let sign = Real::new(f64::from(new));
    if let Some(i) = ctx.w.get_mut(me) {
        i.m.set_vspeed(Real::ZERO)?;
        i.m.set_gravity(g * sign)?;
        i.img.set_yscale(sign)?;
        i.m.set_y(i.m.y() + Real::new(4.0) * sign)?;
    }
    Ok(())
}

/// `flip_swap` on the kid `me`.
fn flip_swap(ctx: &mut Ctx, me: InstId) -> Result<Flow, SimError> {
    let s = super::common::state(ctx, me)?;
    let want = ctx.w.engine.kid.grav;
    if want == s.flipped {
        return Ok(Flow::Continue);
    }
    let Some(i) = ctx.w.get(me) else {
        return Ok(Flow::Continue);
    };
    let (x, y) = (i.m.x(), i.m.y());
    ctx.w.engine.kid.image_xscale = s.facing;
    let (role, dy) = if want {
        (EngineKind::KidFlipped, Real::new(-4.0))
    } else {
        (EngineKind::Kid, Real::new(4.0))
    };
    if ctx.def.engine_kind(role).is_none() {
        return Err(SimError::unsupported(
            "kid action flip_swap: the game has no flipped kid object (engine role kid_flipped)",
        ));
    }
    ctx.spawn_engine(role, x, y + dy, SpawnInit::default())?;
    let flow = ctx.destroy(me);
    Ok(if flow.is_halt() { flow } else { Flow::Continue })
}
