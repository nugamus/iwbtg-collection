//! Kid actions specific to family C (kid/C.md: `vine_probe`, the vines, the suicide press;
//! docs/plans/sim-core.md 5.3). WP5c owns it. `slide_block` and `slope_c` are not built: no xia
//! room places a slide block or a slope (tier 3), so a profile listing them does not load.

use serde::Deserialize;

use super::common::{down, state, state_mut, vine_distances};
use crate::collide::query::place_free;
use crate::error::SimError;
use crate::input::Action;
use crate::kid::api::{KillCause, kill};
use crate::num::{Real, gm_le};
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CAction {
    /// Step 3-4: `onVineL` / `onVineR`: the distance to the nearest left or right vine is at most
    /// 1 and the kid is free 1 px in the gravity direction. Computed before the jump code; the
    /// `vines` action (step 12) reads them.
    VineProbe,
    /// Step 10: the Suicide key pressed (not held) runs `killPlayer`, when not frozen.
    SuicidePress,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &CAction) -> Result<Flow, SimError> {
    match action {
        CAction::VineProbe => {
            vine_probe(ctx, me)?;
            Ok(Flow::Continue)
        }
        CAction::SuicidePress => suicide_press(ctx, me),
    }
}

fn vine_probe(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let n = &ctx.rules.num;
    let s = state(ctx, me)?;
    let Some(i) = ctx.w.get(me) else { return Ok(()) };
    let (x, y) = (i.m.x(), i.m.y());
    let (dr, dl) = vine_distances(ctx, me)?;
    let free = place_free(ctx, me, x, y + Real::new(f64::from(down(&s))))?;
    let (left, right) = (free && gm_le(dl, Real::ONE, n), free && gm_le(dr, Real::ONE, n));
    let ext = &mut state_mut(ctx, me)?.ext;
    ext.on_vine_l = left;
    ext.on_vine_r = right;
    Ok(())
}

fn suicide_press(ctx: &mut Ctx, me: InstId) -> Result<Flow, SimError> {
    let s = state(ctx, me)?;
    if s.frozen || ctx.w.engine.kid.frozen || !ctx.w.input.pressed(Action::Suicide) {
        return Ok(Flow::Continue);
    }
    kill(ctx, me, KillCause::Suicide)
}

/// Step 12, vines (`vine.model = "distance_sym_early"`), from the flags `vine_probe` left:
/// `xScale` is -1 on a right vine (which wins when both are set), else 1; `vspeed = 2 * grav`
/// overrides the jump code; sliding sprite, `image_speed` 0.5. A Right press on a left vine or a
/// Left press on a right vine then kicks (Jump held, a direct read: `hspeed` +-15, `vspeed = -9 *
/// grav`, jump sprite) or lets go (`hspeed` +-3, fall sprite).
pub(crate) fn vines(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let p = &def.physics;
    let s = state(ctx, me)?;
    let (on_l, on_r) = (s.ext.on_vine_l, s.ext.on_vine_r);
    if !on_l && !on_r {
        return Ok(());
    }
    let sign = Real::new(f64::from(down(&s)));
    let input = &mut ctx.w.input;
    let kick = (on_l && input.pressed(Action::Right)) || (on_r && input.pressed(Action::Left));
    let jump_held = input.direct(Action::Jump);
    let facing: i8 = if on_r { -1 } else { 1 };
    state_mut(ctx, me)?.facing = facing;
    let (kick_h, kick_v) = (p.vine.kick[0], p.vine.kick[1]);
    let drop = p.vine.drop;
    let Some(i) = ctx.w.get_mut(me) else { return Ok(()) };
    i.img.set_xscale(Real::new(f64::from(facing)))?;
    i.m.set_vspeed(p.vine.slide * sign)?;
    super::common::set_sprite(def, &mut i.img, p.sprites.sliding);
    i.img.set_speed(p.numbers.image_speed_run)?;
    if kick {
        let dir = if on_r { -Real::ONE } else { Real::ONE };
        if jump_held {
            i.m.set_hspeed(dir * kick_h)?;
            // `kick[1]` is the unflipped -9; the original writes -9 * grav.
            i.m.set_vspeed(kick_v * sign)?;
            super::common::set_sprite(def, &mut i.img, p.sprites.jump);
        } else {
            i.m.set_hspeed(dir * drop)?;
            super::common::set_sprite(def, &mut i.img, p.sprites.fall);
        }
    }
    Ok(())
}
