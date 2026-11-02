//! The solid collision put-back (docs/plans/sim-core.md 2.3 step 4, C7, C9, M7).
//! WP0 declares; WP3 owns.
//!
//! CLONE_SPEC collision, Confirmed (manual 111_00) for the four steps, and Inferred "What
//! previous place and new position mean" (corrected, Confirmed (recorder)) for their meaning:
//!
//! 1. "Previous place" is `(xprevious, yprevious)` (showdown t-mot-07-a..e.csv).
//! 2. The event runs there.
//! 3. "Moved to its new position" is `x += hspeed; y += vspeed` with the speeds as the event
//!    left them, from wherever the event left the instance (kaenbyou famb-land-moving-b.csv
//!    row 39).
//! 4. If it still collides (C9 `SolidRecheck`, default: with the other instance), it is kept at
//!    its previous place, `(xprevious, yprevious)`, not where the event left it (M7
//!    `SolidKeep`, default; showdown t-mot-07-e.csv rows 2-8).
//!
//! The per-pair re-check before the event (C7) is the caller's (`events::collision_phase`).

use super::overlap::overlaps;
use super::query::{Q, Target};
use crate::error::SimError;
use crate::num::Real;
use crate::rules::collide::{SolidKeep, SolidRecheck};
use crate::world::{Ctx, Flow, InstId};

/// For a solid pair `(a, b)`: put `a` at its previous place, run the event (`run`), then
/// `x += hspeed; y += vspeed` from wherever the event left it; if `a` overlaps again (C9),
/// keep it at its previous place (M7, module docs).
///
/// A `Flow::Halt` from the event returns at once. When the event destroyed `a`, nothing moves.
pub fn solid_resolve(
    ctx: &mut Ctx,
    a: InstId,
    b: InstId,
    run: impl FnOnce(&mut Ctx) -> Flow,
) -> Result<Flow, SimError> {
    let prev = {
        let inst = Q::of(ctx).inst(a)?;
        (inst.m.xprevious(), inst.m.yprevious())
    };
    move_to(ctx, a, prev)?;
    let flow = run(ctx);
    if flow.is_halt() {
        return Ok(flow);
    }
    let (event_pos, speeds, alive) = match ctx.w.get(a) {
        Some(i) => ((i.m.x(), i.m.y()), (i.m.hspeed(), i.m.vspeed()), i.alive),
        None => return Ok(flow),
    };
    if !alive {
        return Ok(flow);
    }
    move_to(ctx, a, (event_pos.0 + speeds.0, event_pos.1 + speeds.1))?;
    if still_colliding(ctx, a, b)? {
        let keep = match ctx.rules.collide.solid_keep {
            SolidKeep::Previous => {
                let inst = Q::of(ctx).inst(a)?;
                (inst.m.xprevious(), inst.m.yprevious())
            }
            SolidKeep::EventPosition => event_pos,
        };
        move_to(ctx, a, keep)?;
    }
    Ok(flow)
}

/// The final check (C9).
fn still_colliding(ctx: &Ctx, a: InstId, b: InstId) -> Result<bool, SimError> {
    let q = Q::of(ctx);
    let inst = q.inst(a)?;
    let (x, y) = (inst.m.x(), inst.m.y());
    match ctx.rules.collide.solid_recheck {
        SolidRecheck::AnySolid => Ok(!q.place_hits(a, x, y, Target::Any, true)?.is_empty()),
        SolidRecheck::VsOther => {
            let Some(other) = ctx.w.get(b) else {
                return Ok(false);
            };
            if !other.active || !(other.alive || ctx.rules.tick.dead_in_queries) {
                return Ok(false);
            }
            let rules = &ctx.rules.collide;
            let pa = super::bbox::instance_box(ctx.def, inst, (x, y), rules)?;
            let pb = super::bbox::instance_box(ctx.def, other, (other.m.x(), other.m.y()), rules)?;
            match (pa, pb) {
                (Some(pa), Some(pb)) => overlaps(&pa, &pb),
                _ => Ok(false),
            }
        }
    }
}

/// Sets `a`'s position, touching only the coordinates that change.
fn move_to(ctx: &mut Ctx, a: InstId, to: (Real, Real)) -> Result<(), SimError> {
    let (x, y) = {
        let inst = Q::of(ctx).inst(a)?;
        (inst.m.x(), inst.m.y())
    };
    if (x, y) == to {
        return Ok(());
    }
    let inst = ctx
        .w
        .get_mut(a)
        .ok_or_else(|| SimError::invariant(format!("instance {} vanished during a solid collision", a.0)))?;
    if x != to.0 {
        inst.m.set_x(to.0)?;
    }
    if y != to.1 {
        inst.m.set_y(to.1)?;
    }
    Ok(())
}
