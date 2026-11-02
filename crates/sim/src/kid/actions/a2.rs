//! Kid actions specific to family A2 (kid/A2.md; docs/plans/sim-core.md 5.3). WP5c owns it.

use serde::Deserialize;

use super::common::{frozen2, state};
use crate::collide::query::{Target, place_meeting};
use crate::def::ClassMask;
use crate::error::SimError;
use crate::input::Action;
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum A2Action {
    /// Animus and easy-miku-2, the end of the movement action: with a `block` or `solidblock` 1 px
    /// below, a press of A (`extra1`) subtracts 1 from `hspeed` and a press of D (`extra2`) adds 1.
    AdNudge,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &A2Action) -> Result<Flow, SimError> {
    match action {
        A2Action::AdNudge => ad_nudge(ctx, me)?,
    }
    Ok(Flow::Continue)
}

fn ad_nudge(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let s = state(ctx, me)?;
    // OPEN: whether `frozen` stops the nudge (A2 "Measured by trace" has no frozen case).
    if frozen2(ctx, &s) || s.frozen || ctx.w.engine.kid.frozen {
        return Ok(());
    }
    let step =
        i32::from(ctx.w.input.pressed(Action::Extra2)) - i32::from(ctx.w.input.pressed(Action::Extra1));
    if step == 0 {
        return Ok(());
    }
    let Some(i) = ctx.w.get(me) else { return Ok(()) };
    let (x, y) = (i.m.x(), i.m.y());
    let sign = Real::new(f64::from(super::common::down(&s)));
    let ground = ClassMask::BLOCK | ClassMask::SOLIDBLOCK;
    if !place_meeting(ctx, me, x, y + sign, Target::Class(ground))? {
        return Ok(());
    }
    if let Some(i) = ctx.w.get_mut(me) {
        i.m.set_hspeed(i.m.hspeed() + Real::new(f64::from(step)))?;
    }
    Ok(())
}
