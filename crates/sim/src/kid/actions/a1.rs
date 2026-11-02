//! Kid actions specific to family A1 (kid/A1.md): `platform_gravity_zero`, `fade_in`
//! (docs/plans/sim-core.md 5.3). WP5a owns it.

use serde::Deserialize;

use super::common::state;
use crate::error::SimError;
use crate::num::{Real, gm_lt};
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum A1Action {
    /// Step action 6 (rely): gravity 0 while `onPlatform`, overriding the Step's gravity reset.
    PlatformGravityZero,
    /// Step action 7 (rely): while `image_alpha < 1`, add `misc.fade_step`; no clamp.
    FadeIn,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &A1Action) -> Result<Flow, SimError> {
    let def = ctx.def;
    let n = &ctx.rules.num;
    match action {
        A1Action::PlatformGravityZero => {
            let on = state(ctx, me)?.on_platform;
            if let (true, Some(i)) = (on, ctx.w.get_mut(me)) {
                i.m.set_gravity(Real::ZERO)?;
            }
        }
        A1Action::FadeIn => {
            if let Some(i) = ctx.w.get_mut(me) {
                let a = i.img.alpha();
                if gm_lt(a, Real::ONE, n) {
                    i.img.set_alpha(a + def.physics.misc.fade_step)?;
                }
            }
        }
    }
    Ok(Flow::Continue)
}
