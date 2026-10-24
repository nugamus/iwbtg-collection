//! Rounding, integer conversion and the small GML helpers built on exact operations.
//!
//! Everything here is exact IEEE (no transcendental math), so hardware f64 is safe.

use super::{NumError, Real};

/// Smallest f64 that is outside the i64 range: 2^63.
const I64_LIMIT: f64 = 9_223_372_036_854_775_808.0;

/// GM8 `round`: half to even ("banker's rounding"), as Delphi's `Round` does.
/// `round(0.5) = 0`, `round(1.5) = 2`, `round(2.5) = 2`, `round(-0.5) = -0`.
///
/// It also places bounding boxes. `f64::round` (half away from zero) is banned in the sim.
pub fn round(x: Real) -> Real {
    Real::new(x.get().round_ties_even())
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
    // In range and integral, so the cast is exact.
    Ok(integral as i64)
}

/// GML `floor`: largest integer not above `x`.
pub fn floor(x: Real) -> Real {
    Real::new(x.get().floor())
}

/// GML `ceil`: smallest integer not below `x`.
pub fn ceil(x: Real) -> Real {
    Real::new(x.get().ceil())
}

/// GML `frac`: `x` minus its integer part toward zero, as Delphi's `Frac`.
/// Keeps the sign of `x`: `frac(-1.25) = -0.25`. The subtraction is exact.
pub fn frac(x: Real) -> Real {
    let v = x.get();
    Real::new(v - v.trunc())
}

/// GML `sign`: `1` for positive, `-1` for negative, `0` for both zeros. NaN stays NaN.
pub fn sign(x: Real) -> Real {
    let v = x.get();
    if v > 0.0 {
        Real::ONE
    } else if v < 0.0 {
        Real::new(-1.0)
    } else if v.is_nan() {
        x
    } else {
        Real::ZERO
    }
}

/// GML `abs`.
pub fn abs(x: Real) -> Real {
    Real::new(x.get().abs())
}

/// GML `min` of two values: `b` only when it is strictly smaller, so ties keep `a`.
///
/// OPEN: confirm in CLONE_SPEC whether GM8 compares exactly or with [`super::EPSILON`].
pub fn min(a: Real, b: Real) -> Real {
    if b.get() < a.get() { b } else { a }
}

/// GML `max` of two values: `b` only when it is strictly larger, so ties keep `a`.
///
/// OPEN: confirm in CLONE_SPEC whether GM8 compares exactly or with [`super::EPSILON`].
pub fn max(a: Real, b: Real) -> Real {
    if b.get() > a.get() { b } else { a }
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

/// GML `a mod b` on reals: the remainder of `a / b` truncated toward zero, so the result keeps
/// the sign of the dividend (`-7 mod 3 = -1`, `7 mod -3 = 1`, `5.5 mod 2 = 1.5`). Computed by
/// `libm::fmod`, which is exact.
///
/// OPEN: confirm in CLONE_SPEC that GM8's `mod` is fmod on reals (not on truncated integers),
/// and that a divisor of exactly zero is the only error case.
pub fn gm_mod(a: Real, b: Real) -> Result<Real, NumError> {
    if b.get() == 0.0 {
        return Err(NumError::DivisionByZero);
    }
    Ok(Real::new(libm::fmod(a.get(), b.get())))
}

/// GML `a div b` on reals: `a / b` truncated toward zero (`7 div 2 = 3`, `-7 div 2 = -3`,
/// `7.5 div 2 = 3`).
///
/// OPEN: confirm in CLONE_SPEC that GM8 divides the reals before truncating (not the truncated
/// operands), and that a divisor of exactly zero is the only error case.
pub fn gm_div(a: Real, b: Real) -> Result<Real, NumError> {
    if b.get() == 0.0 {
        return Err(NumError::DivisionByZero);
    }
    Ok(Real::new((a.get() / b.get()).trunc()))
}
