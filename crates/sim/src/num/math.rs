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

/// GML `arctan2(y, x)`: angle of the vector `(x, y)` in radians, in `[-pi, pi]`, correctly rounded.
///
/// `libm::atan2` is one or two ulps off for ordinary vectors (`atan2(8.1, 3.0)` is `...706` where
/// the recorder has `...707` in kaenbyou land-moving frame 10; vines frame 22), and one ulp high
/// when `|x|` is tiny next to `|y|` (`traces/rely-the-needle-4-2/djump.csv` frame 10). The recorder
/// matches the correctly rounded value, so this takes the `libm` result `a` as a starting point,
/// computes `sin a` and `cos a` in double-double, and adds the residual
/// `(y cos a - x sin a) / (x cos a + y sin a)` (the tangent of the angle left over). Zero, non-finite
/// and extreme-magnitude inputs fall back to `libm`.
///
/// Near an axis the recorder is not correctly rounded: with `|x| / |y|` up to at least `3.5e-16`
/// (`kaenbyou/famb-vines-a.csv` frames 32 to 35, hspeed `-7.3e-17`) its direction is exactly 90.0,
/// which is the double below pi/2 (`rely-the-needle-4-2/djump.csv` frame 10 and 24 agree, ratio
/// `2.9e-17`), not the correctly rounded angle. So below a ratio of `1e-15` this returns that double;
/// the cutoff is a guess between `3.5e-16` and the first ordinary vector (OPEN, research).
pub fn atan2(y: Real, x: Real) -> Real {
    let (yv, xv) = (y.get(), x.get());
    let a = libm::atan2(yv, xv);
    let (ya, xa) = (yv.abs(), xv.abs());
    // gm-compare: ok (internal range guard on raw f64)
    if !(ya > 1e-100 && ya < 1e100 && xa < 1e100 && (xa == 0.0 || xa > 1e-100)) {
        return Real::new(a);
    }
    if xa < ya * 1e-15 {
        return Real::new(core::f64::consts::FRAC_PI_2.copysign(yv));
    }
    let (s, c) = dd_sin_cos(a);
    let num = dd_add(dd_mul_f(c, yv), dd_neg(dd_mul_f(s, xv)));
    let den = dd_add(dd_mul_f(c, xv), dd_mul_f(s, yv));
    if den.0 <= 0.0 {
        return Real::new(a);
    }
    Real::new(a + num.0 / den.0)
}

/// Double-double: an unevaluated sum `hi + lo` with about 106 bits.
type Dd = (f64, f64);

fn two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

fn quick_two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    (s, b - (s - a))
}

/// Dekker split; no FMA, so the bits are the same on every target.
fn split(a: f64) -> Dd {
    let t = 134_217_729.0 * a;
    let hi = t - (t - a);
    (hi, a - hi)
}

fn two_prod(a: f64, b: f64) -> Dd {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    (p, ((ah * bh - p) + ah * bl + al * bh) + al * bl)
}

fn dd_neg(a: Dd) -> Dd {
    (-a.0, -a.1)
}

fn dd_add(a: Dd, b: Dd) -> Dd {
    let (s, e) = two_sum(a.0, b.0);
    let (t, f) = two_sum(a.1, b.1);
    let (s, e) = quick_two_sum(s, e + t);
    quick_two_sum(s, e + f)
}

fn dd_mul_f(a: Dd, b: f64) -> Dd {
    let (p, e) = two_prod(a.0, b);
    quick_two_sum(p, e + a.1 * b)
}

fn dd_mul(a: Dd, b: Dd) -> Dd {
    let (p, e) = two_prod(a.0, b.0);
    quick_two_sum(p, e + (a.0 * b.1 + a.1 * b.0))
}

/// `(sin a, cos a)` in double-double for `|a| <= pi`: reduce by multiples of pi/2, then Taylor.
fn dd_sin_cos(a: f64) -> (Dd, Dd) {
    const HALF_PI: Dd = (core::f64::consts::FRAC_PI_2, 6.123_233_995_736_766e-17);
    // Quadrant: nearest multiple of pi/2 (-2..=2).
    let k = if a >= 0.0 {
        (a * core::f64::consts::FRAC_2_PI + 0.5).floor()
    } else {
        -(-a * core::f64::consts::FRAC_2_PI + 0.5).floor()
    };
    let r = dd_add((a, 0.0), dd_neg(dd_mul_f(HALF_PI, k)));
    let r2 = dd_mul(r, r);
    // sin r = r - r^3/3! + ..., cos r = 1 - r^2/2! + ...; term_n = term_{n-1} * (-r2) / divisor.
    let (mut sin, mut cos) = (r, (1.0, 0.0));
    let (mut ts, mut tc) = (r, (1.0, 0.0));
    let mut n = 1.0;
    for _ in 0..16 {
        tc = dd_div_f(dd_neg(dd_mul(tc, r2)), n * (n + 1.0));
        ts = dd_div_f(dd_neg(dd_mul(ts, r2)), (n + 1.0) * (n + 2.0));
        cos = dd_add(cos, tc);
        sin = dd_add(sin, ts);
        n += 2.0;
    }
    #[allow(clippy::cast_possible_truncation)] // k is -2..=2
    let quadrant = k as i32;
    match quadrant {
        0 => (sin, cos),
        1 => (cos, dd_neg(sin)),
        -1 => (dd_neg(cos), sin),
        _ => (dd_neg(sin), dd_neg(cos)),
    }
}

/// `a / b` for a small exact integer `b`.
fn dd_div_f(a: Dd, b: f64) -> Dd {
    let q1 = a.0 / b;
    let (p, e) = two_prod(q1, b);
    let rem = dd_add(a, dd_neg((p, e)));
    let q2 = rem.0 / b;
    quick_two_sum(q1, q2)
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
