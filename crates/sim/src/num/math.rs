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
pub fn atan2(y: Real, x: Real) -> Real {
    Real::new(libm::atan2(y.get(), x.get()))
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
