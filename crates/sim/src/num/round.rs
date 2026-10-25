//! Rounding, integer conversion and the small GML helpers built on exact operations.
//!
//! Everything here is exact IEEE (no transcendental math), so hardware f64 is safe. Functions
//! whose result depends on an open rule take the run's [`NumRules`] (docs/plans/sim-core.md 3.2).

use super::compare::epsilon;
use super::{NumError, Real};
use crate::rules::NumRules;
use crate::rules::num::{MinMaxTies, NegZero, SignMode};

/// Smallest f64 that is outside the i64 range: 2^63.
const I64_LIMIT: f64 = 9_223_372_036_854_775_808.0;

/// GM8 `round`: half to even ("banker's rounding", Inferred), as Delphi's `Round` does.
/// `round(0.5) = 0`, `round(1.5) = 2`, `round(2.5) = 2`, `round(-0.5) = 0`.
///
/// A zero result's sign follows N4 (`NumRules::round_neg_zero`): `+0.0` under
/// `NegZero::Positive` (engine_rev 0 default), IEEE `-0.0` for `-0.5 <= x <= -0.0` under
/// `NegZero::Keep`.
///
/// `f64::round` (half away from zero) is banned in the sim. Bounding boxes round through
/// `collide::bbox::place_origin`, which returns an integer and so has no signed zero.
pub fn round(x: Real, r: &NumRules) -> Real {
    zero_sign(x.get().round_ties_even(), r.round_neg_zero)
}

/// Applies a [`NegZero`] rule to an integral result.
fn zero_sign(v: f64, rule: NegZero) -> Real {
    match rule {
        NegZero::Positive if v == 0.0 => Real::ZERO,
        NegZero::Positive | NegZero::Keep => Real::new(v),
    }
}

/// Truncates toward zero and converts to an integer, for places where GM8 truncates.
///
/// Errors on NaN, infinity or a value outside i64 instead of saturating silently.
pub fn to_int_trunc(x: Real) -> Result<i64, NumError> {
    real_to_i64(x, x.get().trunc())
}

/// Rounds half to even (see [`round`]) and converts to an integer, for places where GM8 rounds.
///
/// Errors on NaN, infinity or a value outside i64.
pub fn to_int_round(x: Real) -> Result<i64, NumError> {
    real_to_i64(x, x.get().round_ties_even())
}

/// Converts an already integral f64 to i64. `original` is only for the error message.
fn real_to_i64(original: Real, integral: f64) -> Result<i64, NumError> {
    if !integral.is_finite() {
        return Err(NumError::NotFinite(original));
    }
    if !(-I64_LIMIT..I64_LIMIT).contains(&integral) {
        return Err(NumError::OutOfRange(original));
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "checked above: integral and inside the i64 range, so the cast is exact"
    )]
    Ok(integral as i64)
}

/// GML `floor`: largest integer not above `x`. A zero result's sign follows N13
/// (`NumRules::floor_ceil_zero`): under `NegZero::Keep` (engine_rev 0 default)
/// `floor(-0.0) = -0.0`; under `NegZero::Positive` it is `+0.0`.
pub fn floor(x: Real, r: &NumRules) -> Real {
    zero_sign(x.get().floor(), r.floor_ceil_zero)
}

/// GML `ceil`: smallest integer not below `x`. A zero result's sign follows N13: under
/// `NegZero::Keep` (engine_rev 0 default) `ceil(-0.5) = -0.0`; under `NegZero::Positive` it is
/// `+0.0`.
pub fn ceil(x: Real, r: &NumRules) -> Real {
    zero_sign(x.get().ceil(), r.floor_ceil_zero)
}

/// GML `frac`: `x` minus its integer part toward zero, as Delphi's `Frac`.
/// Keeps the sign of `x`: `frac(-1.25) = -0.25`. The subtraction is exact.
pub fn frac(x: Real) -> Real {
    let v = x.get();
    Real::new(v - v.trunc())
}

/// GML `sign` (N11, `NumRules::sign`).
///
/// Under `SignMode::Epsilon` (engine_rev 0 default) it is `1` when `x >= e`, `-1` when
/// `x <= -e`, else `+0.0`, with `e` the compare epsilon of N1; so the gravity drift
/// `-7.347880794884119e-17` has sign `0`. Under `SignMode::Exact`, or when N1 is exact, any value
/// above 0 gives `1` and any value below 0 gives `-1`. Zero and NaN give `+0.0` in both modes
/// (NaN: OPEN).
///
/// Slip blocks subtract `slip * sign(hspeed)`, so N11 must follow N1 (plan 3.2).
pub fn sign(x: Real, r: &NumRules) -> Real {
    let v = x.get();
    let e = match r.sign {
        SignMode::Epsilon => epsilon(r),
        SignMode::Exact => None,
    };
    let (pos, neg) = match e {
        Some(e) => (v >= e, v <= -e),
        None => (v > 0.0, v < 0.0),
    };
    if pos {
        Real::ONE
    } else if neg {
        Real::new(-1.0)
    } else {
        Real::ZERO
    }
}

/// GML `abs`.
pub fn abs(x: Real) -> Real {
    Real::new(x.get().abs())
}

/// GML `min` of two values; ties and signed zeros per N12 (`NumRules::min_max_ties`).
///
/// `MinMaxTies::StrictGreater` (engine_rev 0 default): `b` when `a > b`, else `a`. Ties and a
/// NaN `b` keep `a`; a NaN `a` stays. `MinMaxTies::GreaterOrEqual`: `b` when `a >= b`, else `a`.
/// The test is exact in both; whether GM8 uses the compare epsilon here is part of N12.
pub fn gm_min(a: Real, b: Real, r: &NumRules) -> Real {
    let take_b = match r.min_max_ties {
        MinMaxTies::StrictGreater => a.get() > b.get(),
        MinMaxTies::GreaterOrEqual => a.get() >= b.get(),
    };
    if take_b { b } else { a }
}

/// GML `max` of two values; ties and signed zeros per N12 (`NumRules::min_max_ties`).
///
/// `MinMaxTies::StrictGreater` (engine_rev 0 default): `a` when `a > b`, else `b`. Ties and a
/// NaN on either side give `b` (`max(-0.0, 0.0) = 0.0`, `max(NaN, 1) = 1`).
/// `MinMaxTies::GreaterOrEqual`: `a` when `a >= b`, else `b`.
pub fn gm_max(a: Real, b: Real, r: &NumRules) -> Real {
    let take_a = match r.min_max_ties {
        MinMaxTies::StrictGreater => a.get() > b.get(),
        MinMaxTies::GreaterOrEqual => a.get() >= b.get(),
    };
    if take_a { a } else { b }
}

/// How many times GML `repeat (x)` runs its body: the rounded value of `x` (Confirmed, manual),
/// rounding half to even (Inferred), and 0 when that is 0 or less. It takes the rules so a later
/// `engine_rev` can change the rounding without a new signature (plan 11).
///
/// Errors on NaN or infinity, and on a value outside i64 (`OutOfRange`). The script step budget
/// bounds what a large count can cost.
pub fn gm_repeat_count(x: Real, r: &NumRules) -> Result<u64, NumError> {
    let _ = r;
    let n = to_int_round(x)?;
    Ok(u64::try_from(n).unwrap_or(0))
}

/// Limits `x` to `[lo, hi]`. GM8 has no `clamp`; this follows GameMaker Studio's.
/// If `lo > hi`, the result is `lo` for small `x` and `hi` for large `x`, and never panics.
pub fn clamp(x: Real, lo: Real, hi: Real) -> Real {
    if x.get() < lo.get() {
        lo
    } else if x.get() > hi.get() {
        hi
    } else {
        x
    }
}

/// Linear interpolation `a + (b - a) * t`. GM8 has no `lerp`; this is GameMaker Studio's
/// formula, kept in this order because a different order gives different bits.
pub fn lerp(a: Real, b: Real, t: Real) -> Real {
    a + (b - a) * t
}

/// GML `a mod b` on reals: `a - trunc(a / b) * b`, each step rounded to f64, as the trace
/// recorder (OpenGMK) computes it. The result has the sign of the dividend (`-7 mod 3 = -1`,
/// `7 mod -3 = 1`, `5.5 mod 2 = 1.5`), but an exact multiple gives `+0.0` (`-6 mod 3 = 0`).
///
/// This is not `fmod`, which is exact: `1 mod 0.1` is `0` here and `0.09999999999999995` with
/// `fmod`, and `-6 mod 3` is `-0.0` with `fmod`. `trunc` gives the same bits as OpenGMK's
/// `abs().floor() * signum()` for every input, signed zeros and NaN included.
///
/// OPEN: confirm in CLONE_SPEC that GM8's `mod` uses this formula on reals (not fmod, not on
/// truncated integers), and that a divisor of exactly zero is the only error case.
pub fn gm_mod(a: Real, b: Real) -> Result<Real, NumError> {
    let (a, b) = (a.get(), b.get());
    if b == 0.0 {
        return Err(NumError::DivisionByZero);
    }
    Ok(Real::new(a - (a / b).trunc() * b))
}

/// GML `a div b` on reals: `floor(a / b)`, as the trace recorder (OpenGMK) computes it
/// (`7 div 2 = 3`, `-7 div 2 = -4`, `7 div -2 = -4`, `7.5 div 2 = 3`).
///
/// OPEN: confirm in CLONE_SPEC whether GM8 floors the quotient (OpenGMK) or truncates it
/// toward zero (Delphi's integer `div`, which gives `-7 div 2 = -3`), whether it divides the
/// reals or the truncated operands, and that a divisor of exactly zero is the only error case.
pub fn gm_div(a: Real, b: Real) -> Result<Real, NumError> {
    if b.get() == 0.0 {
        return Err(NumError::DivisionByZero);
    }
    Ok(Real::new((a.get() / b.get()).floor()))
}
