//! `move_bounce_solid`, `move_bounce_all` and the Bounce action (register C14, U-MECH-5).
//!
//! Rule: research `specs/engines/gm8/CLONE_SPEC.md`, GM8 runner motion, Confirmed (recorder),
//! "Bounce" (traces `animus/tr-mech-04.md`, research#190). A bounce changes only `hspeed` and
//! `vspeed` (so `speed` and `direction`); it never moves the instance. The tick's motion then
//! moves it by the new speed, after friction and gravity. "Blocked" is `place_free` false
//! (against solids) or `place_empty` false (against all), tested with the instance's own mask.
//!
//! Both rules are pure functions of the speeds and a "blocked here" closure
//! ([`plain_bounce`], [`precise_direction`]), so tests can check them against the trace tables
//! without a world.
//!
//! A precise bounce writes the new direction with `Motion::set_direction` (speed kept), so
//! `hspeed` and `vspeed` follow from the same conversion as any direction write: libm
//! `speed * cos` and `-speed * sin` of the new direction in radians, each snapped to the nearest
//! integer when within 1e-4 of it (`num::snap_to_int`; research#193, Confirmed (recorder): the
//! rule fits all 3716 changed precise-bounce rows, zero sign included). The sim's libm gives those bits on every
//! row but direction 220, where its `cos` is one ulp from the recorder's (the M6 gap in
//! `num/trig.rs`). Still OPEN (research#8): the real runner, speed 0
//! and negative speeds (the rule is applied as written).

use super::query::{place_empty, place_free};
use crate::error::SimError;
use crate::num::{self, Real, lengthdir_x, lengthdir_y};
use crate::rules::NumRules;
use crate::world::{Ctx, InstId};

/// What a bounce tests against.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Against {
    /// Solid instances (`move_bounce_solid`, the Bounce action's "solid objects").
    Solid,
    /// Every instance (`move_bounce_all`, "all objects").
    All,
}

/// GML `move_bounce_solid(precise)`: the Bounce action against solid objects.
pub fn move_bounce_solid(ctx: &mut Ctx, me: InstId, precise: bool) -> Result<(), SimError> {
    move_bounce(ctx, me, precise, Against::Solid)
}

/// GML `move_bounce_all(precise)`: the Bounce action against all objects.
pub fn move_bounce_all(ctx: &mut Ctx, me: InstId, precise: bool) -> Result<(), SimError> {
    move_bounce(ctx, me, precise, Against::All)
}

/// The Bounce action (precise, against): changes `me`'s `hspeed` and `vspeed` only.
pub fn move_bounce(ctx: &mut Ctx, me: InstId, precise: bool, against: Against) -> Result<(), SimError> {
    let inst = ctx
        .w
        .get(me)
        .ok_or_else(|| SimError::invariant(format!("bounce: instance {} is gone", me.0)))?;
    let (x, y) = (inst.m.x(), inst.m.y());
    let (h, v) = (inst.m.hspeed(), inst.m.vspeed());
    let (speed, direction) = (inst.m.speed(), inst.m.direction());
    let blocked = |px: Real, py: Real| -> Result<bool, SimError> {
        let c: &Ctx = ctx;
        Ok(!match against {
            Against::Solid => place_free(c, me, px, py)?,
            Against::All => place_empty(c, me, px, py)?,
        })
    };
    if precise {
        let Some(dir) = precise_direction((x, y), speed, direction, &ctx.rules.num, blocked)? else {
            return Ok(());
        };
        let inst = live_mut(ctx, me)?;
        inst.m.set_direction(dir)
    } else {
        let (flip_h, flip_v) = plain_bounce((x, y), (h, v), blocked)?;
        let inst = live_mut(ctx, me)?;
        if flip_h {
            inst.m.set_hspeed(-h)?;
        }
        if flip_v {
            inst.m.set_vspeed(-v)?;
        }
        Ok(())
    }
}

fn live_mut<'c>(ctx: &'c mut Ctx, me: InstId) -> Result<&'c mut crate::world::Instance, SimError> {
    ctx.w
        .get_mut(me)
        .ok_or_else(|| SimError::invariant(format!("bounce: instance {} is gone", me.0)))
}

/// The not-precise rule: which of `hspeed` and `vspeed` to negate. `h` flips when `(x + h, y)`
/// is blocked, `v` when `(x, y + v)` is; when neither is but `(x + h, y + v)` is, both flip.
/// Every test starts from the current position.
pub fn plain_bounce(
    (x, y): (Real, Real),
    (h, v): (Real, Real),
    mut blocked: impl FnMut(Real, Real) -> Result<bool, SimError>,
) -> Result<(bool, bool), SimError> {
    let fh = blocked(x + h, y)?;
    let fv = blocked(x, y + v)?;
    if !fh && !fv && blocked(x + h, y + v)? {
        return Ok((true, true));
    }
    Ok((fh, fv))
}

/// The precise rule: the new direction, or `None` when nothing changes. `free[k]` is "the point
/// at radius `speed` in direction `10 k` is not blocked" (k = 0..35); `k0 = round(direction / 10)`
/// (ties to even) mod 36; `a` and `b` are the steps (>= 1) to the first free sample going up and
/// down from `k0`. None free: `10 k0 + 180`. `a = b = 1`: no change. Otherwise
/// `10 (k0 + a - b) + 180`, mod 360.
pub fn precise_direction(
    (x, y): (Real, Real),
    speed: Real,
    direction: Real,
    nr: &NumRules,
    mut blocked: impl FnMut(Real, Real) -> Result<bool, SimError>,
) -> Result<Option<Real>, SimError> {
    let mut free = [false; 36];
    for (k, f) in (0i32..).zip(free.iter_mut()) {
        let d = Real::new(f64::from(k * 10));
        *f = !blocked(x + lengthdir_x(speed, d), y + lengthdir_y(speed, d))?;
    }
    let k0 = num::to_int_round(num::round(direction / Real::new(10.0), nr))
        .map_err(|e| SimError::unsupported(format!("bounce direction: {e}")))?
        .rem_euclid(36);
    Ok(precise_pick(k0, &free).map(|d| Real::new(f64::from(d))))
}

/// The precise rule on the samples: the new direction in degrees (a multiple of 10 in
/// `[0, 360)`), or `None` when nothing changes. `k0` is in `0..36`.
pub fn precise_pick(k0: i64, free: &[bool; 36]) -> Option<i32> {
    let at = |k: i64| free[usize::try_from(k.rem_euclid(36)).unwrap_or(0)];
    let a = (1..36).find(|&j| at(k0 + j));
    let b = (1..36).find(|&j| at(k0 - j));
    let k = match (a, b) {
        (Some(1), Some(1)) => return None,
        (Some(a), Some(b)) => k0 + a - b,
        // One free sample is found both ways, so `a` and `b` are both set or both unset.
        _ => k0,
    };
    i32::try_from((k * 10 + 180).rem_euclid(360)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frees(blocked: &[usize]) -> [bool; 36] {
        let mut f = [true; 36];
        for &k in blocked {
            f[k] = false;
        }
        f
    }

    #[test]
    fn nearest_blocked_neighbours_free_changes_nothing() {
        assert_eq!(precise_pick(0, &frees(&[0])), None);
    }

    #[test]
    fn floor_under_a_fall_reflects() {
        // Falling at 270 onto a floor: samples 190..350 blocked (k 19..35).
        let blocked: Vec<usize> = (19..36).collect();
        // a: first free above 27 is 0 (9 steps); b: first free below is 18 (9 steps).
        assert_eq!(precise_pick(27, &frees(&blocked)), Some(90));
    }

    #[test]
    fn sliding_along_a_floor_turns_up_10() {
        // Direction 0 touching a floor: samples 190..350 blocked. Up from k0 = 0, sample 1 is
        // free (a = 1); down, 35..19 are blocked and 18 is free (b = 18): 10 (1 - 18) + 180 = 10.
        let blocked: Vec<usize> = (19..36).collect();
        assert_eq!(precise_pick(0, &frees(&blocked)), Some(10));
    }

    #[test]
    fn none_free_reverses() {
        assert_eq!(precise_pick(3, &[false; 36]), Some(210));
    }
}
