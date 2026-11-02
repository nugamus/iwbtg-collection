//! Contact moves (docs/plans/sim-core.md 3.4, C10, C11, C14). WP0 declares; WP3 owns.
//!
//! Both moves are computed on positions only ([`contact_steps`], [`outside_steps`]), with a
//! closure that answers "does `me` placed here overlap a solid", and write `x` and `y` once at
//! the end. Steps add the unit vector `(dcos d, -dsin d)` to the real position, so whole-pixel
//! axis moves keep the sub-pixel (CLONE_SPEC collision, Confirmed (recorder): x 49.5 rests at
//! 58.5, trick-wall-align.md run d; 39 to 37, t-col-02.csv row 57).
//!
//! **Unbounded loops** (Design). A negative `maxdist` means "any distance" (manual 403_01).
//! Once `me` has moved farther (in x plus y) than the extent of every solid and its own box
//! together, no later step can reach a solid, so a contact move that has not stopped by then
//! never would: that is `SimError::Unsupported`, never an endless loop. A positive `maxdist`
//! past that bound keeps stepping without tests, up to [`MAX_FREE_STEPS`] more.

use super::bbox::{instance_box, reported_box};
use super::query::{Q, Target};
use crate::def::IRect;
use crate::error::SimError;
use crate::num::{Real, dcos, dsin};
use crate::rules::collide::McsStart;
use crate::world::{Ctx, InstId};

/// Most untested steps a positive `maxdist` may take beyond the reach of every solid.
pub const MAX_FREE_STEPS: u64 = 1 << 20;

/// GML `move_contact_solid` (Inferred): unit steps `(dcos d, -dsin d)` until one more step would
/// hit a solid, at most `maxdist` steps (C11), no limit when negative (bounded by the reach of
/// the solids, then `Unsupported`). Overlapping start per C10 (default: no move, Confirmed
/// (recorder) little-runner t-col-07-d.csv rows 4-5).
pub fn move_contact_solid(ctx: &mut Ctx, me: InstId, dir: Real, maxdist: Real) -> Result<(), SimError> {
    let limit = mcs_step_limit(maxdist)?;
    let (start, end) = {
        let q = Q::of(ctx);
        let inst = q.inst(me)?;
        let start = (inst.m.x(), inst.m.y());
        let reach = || solid_reach(&q, me);
        let hits = |x: Real, y: Real| Ok(!q.place_hits(me, x, y, Target::Any, true)?.is_empty());
        (start, contact_steps(start, dir, limit, ctx.rules.collide.mcs_start, hits, reach)?)
    };
    set_position(ctx, me, start, end)
}

/// GML `move_outside_solid` (Inferred): when `me` overlaps a solid, step 1 px in `dir` until it
/// no longer does, at most `maxdist` steps (C11 count rule), no limit when negative.
pub fn move_outside_solid(ctx: &mut Ctx, me: InstId, dir: Real, maxdist: Real) -> Result<(), SimError> {
    let limit = mcs_step_limit(maxdist)?;
    let (start, end) = {
        let q = Q::of(ctx);
        let inst = q.inst(me)?;
        let start = (inst.m.x(), inst.m.y());
        let reach = || solid_reach(&q, me);
        let hits = |x: Real, y: Real| Ok(!q.place_hits(me, x, y, Target::Any, true)?.is_empty());
        (start, outside_steps(start, dir, limit, hits, reach)?)
    };
    set_position(ctx, me, start, end)
}

/// GML `move_bounce_solid` (C14, UNKNOWN U-MECH-5): `Unsupported` until specified. Tier 3.
pub fn move_bounce_solid(_ctx: &mut Ctx, _me: InstId, precise: bool) -> Result<(), SimError> {
    Err(SimError::unsupported(format!(
        "move_bounce_solid (precise {precise}): the algorithm is UNKNOWN (C14, U-MECH-5, TR-MECH-04)"
    )))
}

/// C11: how many unit steps `move_contact_solid` may take for `maxdist`; `None` for no limit.
///
/// Default (Inferred): `floor(maxdist)` steps, so `maxdist` 0 does not move and a fractional
/// part never adds a step (the spec shows the fraction cannot change the kid's results from a
/// free start); negative means no limit (manual 403_01).
pub fn mcs_step_limit(maxdist: Real) -> Result<Option<u64>, SimError> {
    let m = maxdist.get();
    if !m.is_finite() {
        return Err(SimError::non_finite("move_contact maxdist"));
    }
    if m < 0.0 {
        return Ok(None);
    }
    let f = m.floor();
    // u64::MAX as f64 rounds up to 2^64; anything at or past it is out of range.
    if f >= 18_446_744_073_709_551_616.0 {
        return Err(SimError::overflow("move_contact maxdist"));
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked above: integral, non-negative and below 2^64"
    )]
    Ok(Some(f as u64))
}

/// The unit step for a direction in degrees: `(dcos d, -dsin d)`.
fn unit_step(dir: Real) -> Result<(Real, Real), SimError> {
    if !dir.is_finite() {
        return Err(SimError::non_finite("move_contact direction"));
    }
    Ok((dcos(dir), -dsin(dir)))
}

/// The pure `move_contact_solid` loop: the final position from `start`. `hits(x, y)` tells
/// whether `me` placed there overlaps a solid; `reach()` is the step count past which no solid
/// can be hit (computed only when needed).
pub fn contact_steps(
    start: (Real, Real),
    dir: Real,
    limit: Option<u64>,
    start_rule: McsStart,
    mut hits: impl FnMut(Real, Real) -> Result<bool, SimError>,
    reach: impl FnOnce() -> Result<u64, SimError>,
) -> Result<(Real, Real), SimError> {
    if start_rule == McsStart::ManualNoMove && hits(start.0, start.1)? {
        return Ok(start);
    }
    let (dx, dy) = unit_step(dir)?;
    let mut pos = start;
    let mut reach = Some(reach);
    let mut bound: Option<u64> = None;
    let mut taken: u64 = 0;
    loop {
        if limit.is_some_and(|l| taken >= l) {
            return Ok(pos);
        }
        // Past the reach of every solid: no step can stop any more.
        let b = match bound {
            Some(b) => b,
            None if limit.is_some_and(|l| l <= QUICK_STEPS) => u64::MAX,
            None => {
                let f = reach.take().ok_or_else(|| SimError::invariant("contact reach computed twice"))?;
                let b = f()?;
                bound = Some(b);
                b
            }
        };
        if taken >= b {
            let Some(l) = limit else {
                return Err(SimError::unsupported(
                    "move_contact_solid with a negative maxdist and no solid in its path (C11)",
                ));
            };
            let rest = l - taken;
            if rest > MAX_FREE_STEPS {
                return Err(SimError::unsupported(format!(
                    "move_contact_solid maxdist {l}: {rest} steps past every solid (limit {MAX_FREE_STEPS})"
                )));
            }
            for _ in 0..rest {
                pos = (pos.0 + dx, pos.1 + dy);
            }
            return Ok(pos);
        }
        let next = (pos.0 + dx, pos.1 + dy);
        if hits(next.0, next.1)? {
            return Ok(pos);
        }
        pos = next;
        taken += 1;
    }
}

/// Limits up to this many steps run without computing the reach bound.
const QUICK_STEPS: u64 = 64;

/// The pure `move_outside_solid` loop: from an overlapping `start`, step until free.
pub fn outside_steps(
    start: (Real, Real),
    dir: Real,
    limit: Option<u64>,
    mut hits: impl FnMut(Real, Real) -> Result<bool, SimError>,
    reach: impl FnOnce() -> Result<u64, SimError>,
) -> Result<(Real, Real), SimError> {
    if !hits(start.0, start.1)? {
        return Ok(start);
    }
    let (dx, dy) = unit_step(dir)?;
    let bound = match limit {
        Some(l) if l <= QUICK_STEPS => u64::MAX,
        _ => reach()?,
    };
    let mut pos = start;
    let mut taken: u64 = 0;
    loop {
        if limit.is_some_and(|l| taken >= l) {
            return Ok(pos);
        }
        if taken > bound {
            return Err(SimError::invariant("move_outside_solid still overlaps past every solid"));
        }
        pos = (pos.0 + dx, pos.1 + dy);
        taken += 1;
        if !hits(pos.0, pos.1)? {
            return Ok(pos);
        }
    }
}

/// Steps after which `me` cannot touch any solid: the x plus y extent of the union of every
/// solid's box and `me`'s own box, plus 2.
fn solid_reach(q: &Q, me: InstId) -> Result<u64, SimError> {
    let inst = q.inst(me)?;
    let rules = &q.rules.collide;
    let Some(own) = instance_box(q.def, inst, (inst.m.x(), inst.m.y()), rules)? else {
        return Ok(0);
    };
    let mut u: IRect = own.bbox;
    // Every solid, whatever its state (a superset is safe). The reported box covers the
    // collision box to within a pixel and exists for every angle.
    for c in q.w.instances() {
        if c.id == me || !c.solid {
            continue;
        }
        if let Some(b) = reported_box(q.def, c, (c.m.x(), c.m.y()), rules)? {
            u = IRect {
                left: u.left.min(b.left),
                top: u.top.min(b.top),
                right: u.right.max(b.right),
                bottom: u.bottom.max(b.bottom),
            };
        }
    }
    let w = i128::from(u.right) - i128::from(u.left) + 1;
    let h = i128::from(u.bottom) - i128::from(u.top) + 1;
    // A unit step moves |cos| + |sin| >= 1 in x plus y, so after more than w + h steps the box
    // has moved past the union on one axis; +8 covers the pixel margins and rounding.
    u64::try_from(w + h + 8).map_err(|_| SimError::overflow("move_contact reach"))
}

/// Writes the final position when it differs from the start.
fn set_position(ctx: &mut Ctx, me: InstId, start: (Real, Real), end: (Real, Real)) -> Result<(), SimError> {
    if end == start {
        return Ok(());
    }
    let inst = ctx
        .w
        .get_mut(me)
        .ok_or_else(|| SimError::invariant(format!("instance {} vanished during a contact move", me.0)))?;
    if end.0 != start.0 {
        inst.m.set_x(end.0)?;
    }
    if end.1 != start.1 {
        inst.m.set_y(end.1)?;
    }
    Ok(())
}
