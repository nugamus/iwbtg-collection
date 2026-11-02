//! Gravity flip: the `[flip]` models (docs/plans/sim-core.md 5.3). WP5c owns it. Family B's
//! `player_reverse` is in `b.rs`; the group A object swap (`flip_swap`) is WP5c.4 (#131).

use serde::Deserialize;

use super::common::state_mut;
use crate::error::SimError;
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlipAction {
    /// `scrFlipGrav` (family C, `flip.model = "grav_sign"`): `grav` negates, `djump` 1, `vspeed` 0,
    /// gravity takes the new sign, `image_yscale` follows (the mask mirrors with it) and `y` moves
    /// 4 times the new `grav`. Also `kid::api::flip_grav`, for the gravity arrows.
    FlipGrav,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &FlipAction) -> Result<Flow, SimError> {
    match action {
        FlipAction::FlipGrav => flip_grav(ctx, me)?,
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
