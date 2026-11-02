//! Slip blocks and ice: `[ice] model = "slip"` (in `walk`) and `ice_temp_hspeed` (docs/plans/
//! sim-core.md 5.3, N11). WP5c owns it.

use serde::Deserialize;

use super::common::{down, state, state_mut};
use crate::builtins::block::slip_of;
use crate::collide::query::{Target, instance_position, place_meeting};
use crate::def::ClassMask;
use crate::error::SimError;
use crate::num::{Real, gm_eq, gm_lt, gm_ne, sign};
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IceAction {
    /// Showdown Step action 3 (kid/A3.md): with a slip block within `ice.temp_probe` px below,
    /// `tempHspeed` stores `hspeed` when it is 0 or when the two have the same sign and `hspeed`
    /// is not 0; otherwise (opposite sign, or `hspeed` 0) `hspeed` gains `tempHspeed`, which is
    /// multiplied by `ice.temp_decay` and becomes 0 below `ice.temp_zero_below`. Off ice it is 0.
    IceTempHspeed,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &IceAction) -> Result<Flow, SimError> {
    match action {
        IceAction::IceTempHspeed => ice_temp_hspeed(ctx, me)?,
    }
    Ok(Flow::Continue)
}

/// Whether a slip block lies within `px` pixels in the gravity direction.
pub(crate) fn on_ice(ctx: &mut Ctx, me: InstId, px: Real) -> Result<bool, SimError> {
    let s = state(ctx, me)?;
    let Some(i) = ctx.w.get(me) else { return Ok(false) };
    let (x, y) = (i.m.x(), i.m.y());
    let dy = px * Real::new(f64::from(down(&s)));
    place_meeting(ctx, me, x, y + dy, Target::Class(ClassMask::SLIP))
}

/// The slip value of the slip block at the point `ice.read` px in the gravity direction (0 when
/// there is none).
pub(crate) fn slip_below(ctx: &mut Ctx, me: InstId) -> Result<Real, SimError> {
    let s = state(ctx, me)?;
    let Some(i) = ctx.w.get(me) else {
        return Ok(Real::ZERO);
    };
    let (x, y) = (i.m.x(), i.m.y());
    let dy = ctx.def.physics.ice.read * Real::new(f64::from(down(&s)));
    let found = instance_position(ctx, x, y + dy, Target::Class(ClassMask::SLIP))?;
    Ok(found.map_or(Real::ZERO, |b| slip_of(ctx.w, b)))
}

fn ice_temp_hspeed(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let n = &ctx.rules.num;
    let rules = &def.physics.ice;
    let ice = on_ice(ctx, me, rules.temp_probe)?;
    let temp = state(ctx, me)?.temp_hspeed;
    let hs = ctx.w.get(me).map_or(Real::ZERO, |i| i.m.hspeed());
    if !ice {
        state_mut(ctx, me)?.temp_hspeed = Real::ZERO;
        return Ok(());
    }
    let same_sign = gm_eq(sign(temp, n), sign(hs, n), n);
    if gm_eq(temp, Real::ZERO, n) || (same_sign && gm_ne(hs, Real::ZERO, n)) {
        state_mut(ctx, me)?.temp_hspeed = hs;
        return Ok(());
    }
    if let Some(i) = ctx.w.get_mut(me) {
        i.m.set_hspeed(hs + temp)?;
    }
    let mut t = temp * rules.temp_decay;
    if gm_lt(Real::new(t.get().abs()), rules.temp_zero_below, n) {
        t = Real::ZERO;
    }
    state_mut(ctx, me)?.temp_hspeed = t;
    Ok(())
}
