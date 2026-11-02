//! Software transcendental math on `libm` (no default features, so no arch-specific paths).
//!
//! These give the same bits on every platform. Std's `sin`, `exp`, `powf` and friends call the
//! platform libm and are banned in the sim (crates/sim/clippy.toml). Results may be NaN or
//! infinite for out-of-domain input; host setters reject non-finite values.

use super::Real;

/// Sine of an angle in radians.
pub fn sin(x: Real) -> Real {
    Real::new(libm::sin(x.get()))
}

/// Cosine of an angle in radians.
pub fn cos(x: Real) -> Real {
    Real::new(libm::cos(x.get()))
}

/// Tangent of an angle in radians.
pub fn tan(x: Real) -> Real {
    Real::new(libm::tan(x.get()))
}

/// GML `arctan2(y, x)`: angle of the vector `(x, y)` in radians, in `[-pi, pi]`.
///
/// `libm::atan2` rounds one ulp high when `|x|` is tiny next to `|y|`: `atan2(8.5, -7.3e-17)` is
/// `1.5707963267948968` there and `1.5707963267948966` (the correctly rounded value, what the
/// recorder's libm gives) in `traces/rely-the-needle-4-2/djump.csv` frame 10 (direction 90.0). When
/// `|x| / |y|` is below 4.9e-17 the true angle is within `1.1e-16` of pi/2 on the side of the double
/// below it, so that double is the nearest one (half an ulp is `1.11e-16`, the double is `6.1e-17`
/// below pi/2) and is returned directly. Frame 24 of the same trace has `|x| / |y|` = 2.9e-17.
pub fn atan2(y: Real, x: Real) -> Real {
    let (yv, xv) = (y.get(), x.get());
    if yv != 0.0 && yv.is_finite() && xv.is_finite() && xv.abs() < yv.abs() * 4.9e-17 {
        return Real::new(core::f64::consts::FRAC_PI_2.copysign(yv));
    }
    Real::new(libm::atan2(yv, xv))
}

/// Square root. IEEE `sqrt` is correctly rounded on every target, so this is hardware f64.
/// Negative input gives NaN.
pub fn sqrt(x: Real) -> Real {
    Real::new(x.get().sqrt())
}

/// GML `power(x, y)`: `x` raised to `y`.
///
/// OPEN: confirm in CLONE_SPEC how GM8's `power` treats a negative base and integer exponents
/// (Delphi's `Power` has special cases); this is plain `libm::pow`.
pub fn pow(x: Real, y: Real) -> Real {
    Real::new(libm::pow(x.get(), y.get()))
}

/// GML `exp(x)`: e raised to `x`.
pub fn exp(x: Real) -> Real {
    Real::new(libm::exp(x.get()))
}

/// GML `ln(x)`: natural logarithm. Zero gives negative infinity, negative input gives NaN.
pub fn ln(x: Real) -> Real {
    Real::new(libm::log(x.get()))
}
