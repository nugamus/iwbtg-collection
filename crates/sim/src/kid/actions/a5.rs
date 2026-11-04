//! Kid actions specific to family A5 (kid/A5.md: little-runner's End Step; docs/plans/sim-core.md
//! 5.3). WP5c owns it.

use serde::Deserialize;

use crate::collide::move_contact_solid;
use crate::collide::query::place_free;
use crate::error::SimError;
use crate::num::{Real, point_direction};
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum A5Action {
    /// little-runner's End Step: gravity is `numbers.gravity` every tick (frozen or not); then,
    /// when the kid overlaps a solid at its position, `y` goes back to `yprevious` and the kid
    /// moves to contact toward where it was (`move_contact_solid` with no limit). `vspeed` is
    /// kept. (The original's `obj_wall` slope branches never run: no such object is placed.)
    GravityAndUnstuck,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &A5Action) -> Result<Flow, SimError> {
    match action {
        A5Action::GravityAndUnstuck => gravity_and_unstuck(ctx, me)?,
    }
    Ok(Flow::Continue)
}

fn gravity_and_unstuck(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let gravity = ctx.def.physics.numbers.gravity;
    let missing = || SimError::invariant("gravity_and_unstuck: kid vanished");
    let (x, wy, yprev, wspeed) = {
        let i = ctx.w.get_mut(me).ok_or_else(missing)?;
        i.m.set_gravity(gravity)?;
        (i.m.x(), i.m.y(), i.m.yprevious(), i.m.vspeed())
    };
    if !place_free(ctx, me, x, wy)? {
        ctx.w.get_mut(me).ok_or_else(missing)?.m.set_y(yprev)?;
        move_contact_solid(ctx, me, point_direction(x, yprev, x, wy), Real::new(-1.0))?;
        ctx.w.get_mut(me).ok_or_else(missing)?.m.set_vspeed(wspeed)?;
    }
    Ok(())
}
