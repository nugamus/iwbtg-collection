//! Rounding, integer conversion and the small GML helpers built on exact operations.
//!
//! Everything here is exact IEEE (no transcendental math), so hardware f64 is safe.

use super::{EPSILON, NumError, Real};

/// Smallest f64 that is outside the i64 range: 2^63.
const I64_LIMIT: f64 = 9_223_372_036_854_775_808.0;

/// GM8 `round`: half to even ("banker's rounding"), as Delphi's `Round` does.
/// `round(0.5) = 0`, `round(1.5) = 2`, `round(2.5) = 2`, `round(-0.5) = 0`.
///
/// A zero result is always `+0.0`: Delphi's `Round` returns an integer, so GM8 never gives a
/// negative zero here, and the trace recorder (OpenGMK) gives `+0.0` for `-0.5 <= x < 0`.
/// OPEN: OpenGMK gives `-0.0` for `round(-0.0)`; GM8's integer result cannot. Confirm in
/// CLONE_SPEC.
///
/// It also places bounding boxes. `f64::round` (half away from zero) is banned in the sim.
pub fn round(x: Real) -> Real {
    positive_zero(x.get().round_ties_even())
}

/// Turns `-0.0` into `+0.0` and leaves every other value alone.
fn positive_zero(v: f64) -> Real {
    if v == 0.0 { Real::ZERO } else { Real::new(v) }
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

/// GML `floor`: largest integer not above `x`. `floor(-0.0) = -0.0`.
///
/// OPEN: this keeps IEEE signed zeros, as the trace recorder (OpenGMK) does. If GM8's `floor`
/// goes through Delphi's integer `Floor`, it never returns `-0.0`. Confirm in CLONE_SPEC.
pub fn floor(x: Real) -> Real {
    Real::new(x.get().floor())
}

/// GML `ceil`: smallest integer not below `x`. `ceil(-0.5) = -0.0`.
///
/// OPEN: this keeps IEEE signed zeros, as the trace recorder (OpenGMK) does. If GM8's `ceil`
/// goes through Delphi's integer `Ceil`, it gives `+0.0` for `-1 < x <= 0`. Confirm in
/// CLONE_SPEC.
pub fn ceil(x: Real) -> Real {
    Real::new(x.get().ceil())
}

/// GML `frac`: `x` minus its integer part toward zero, as Delphi's `Frac`.
/// Keeps the sign of `x`: `frac(-1.25) = -0.25`. The subtraction is exact.
pub fn frac(x: Real) -> Real {
    let v = x.get();
    Real::new(v - v.trunc())
}

/// GML `sign` with the comparison tolerance: `1` when `x >= EPSILON`, `-1` when
/// `x <= -EPSILON`, else `0` (`+0.0`). So the gravity drift `-7.347880794884119e-17` has
/// sign `0`, as in the trace recorder (OpenGMK).
///
/// OPEN: confirm in CLONE_SPEC that the real GM8 runner uses the tolerance here, and what it
/// gives for NaN. This returns `0` for NaN, like the trace recorder.
pub fn sign(x: Real) -> Real {
    let v = x.get();
    if v >= EPSILON {
        Real::ONE
    } else if v <= -EPSILON {
        Real::new(-1.0)
    } else {
        Real::ZERO
    }
}

/// GML `abs`.
pub fn abs(x: Real) -> Real {
    Real::new(x.get().abs())
}

/// GML `min` of two values: `b` when `a > b`, else `a`. Ties and a NaN `b` keep `a`; a NaN
/// `a` stays. Same as the trace recorder (OpenGMK).
///
/// OPEN: confirm in CLONE_SPEC whether GM8 compares exactly or with [`super::EPSILON`].
pub fn min(a: Real, b: Real) -> Real {
    if a.get() > b.get() { b } else { a }
}

/// GML `max` of two values: `a` when `a > b`, else `b`. Ties and a NaN on either side give
/// `b` (`max(-0.0, 0.0) = 0.0`, `max(NaN, 1) = 1`). Same as the trace recorder (OpenGMK).
///
/// OPEN: confirm in CLONE_SPEC whether GM8 compares exactly or with [`super::EPSILON`].
pub fn max(a: Real, b: Real) -> Real {
    if a.get() > b.get() { a } else { b }
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
