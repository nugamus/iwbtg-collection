//! GM8 comparisons and truthiness.
//!
//! GML compares reals with a tolerance on the difference `d = a - b`: two reals are equal when
//! `|d| < EPSILON`. Built-ins use these functions wherever the original compared reals in GML,
//! and scripts call them by name, because plain `==` in scripts stays exact.
//!
//! The bounds follow the trace recorder (OpenGMK), so a difference of exactly [`EPSILON`] is
//! not equal:
//!
//! | GML      | Here      | True when            |
//! |----------|-----------|----------------------|
//! | `a == b` | [`gm_eq`] | `\|d\| < EPSILON`    |
//! | `a != b` | [`gm_ne`] | `\|d\| >= EPSILON`   |
//! | `a < b`  | [`gm_lt`] | `d <= -EPSILON`      |
//! | `a <= b` | [`gm_le`] | `d < EPSILON`        |
//! | `a > b`  | [`gm_gt`] | `d >= EPSILON`       |
//! | `a >= b` | [`gm_ge`] | `d > -EPSILON`       |
//!
//! Every comparison is false when `d` is NaN (a NaN operand, or `inf - inf`), `!=` included.

use super::Real;

/// Tolerance on the difference for GM8 real comparisons. Value and exclusive bound from
/// OpenGMK, which the traces were recorded with; docs/architecture.md, "Comparisons".
///
/// OPEN: confirm in CLONE_SPEC the value and that the bound is exclusive (`|d| < EPSILON` is
/// equal) in the real GM8 runner.
pub const EPSILON: f64 = 1e-13;

/// GML truthiness boundary: a real is true from this value on (`x >= 0.5`).
///
/// Decision 0001 says "from 0.5", and the trace recorder (OpenGMK) treats 0.5 as true.
/// OPEN: confirm in CLONE_SPEC that the real GM8 runner counts 0.5 itself as true.
pub const TRUTH_THRESHOLD: f64 = 0.5;

/// The difference the comparisons test.
fn diff(a: Real, b: Real) -> f64 {
    a.get() - b.get()
}

/// GML `a == b`: `|a - b| < EPSILON`. False if the difference is NaN.
pub fn gm_eq(a: Real, b: Real) -> bool {
    diff(a, b).abs() < EPSILON
}

/// GML `a != b`: `|a - b| >= EPSILON`. Also false if the difference is NaN, like the trace
/// recorder, so it is not quite `!gm_eq`.
///
/// OPEN: confirm in CLONE_SPEC how the real GM8 runner compares NaN.
pub fn gm_ne(a: Real, b: Real) -> bool {
    diff(a, b).abs() >= EPSILON
}

/// GML `a < b`: `a - b <= -EPSILON`.
pub fn gm_lt(a: Real, b: Real) -> bool {
    diff(a, b) <= -EPSILON
}

/// GML `a <= b`: `a - b < EPSILON`.
pub fn gm_le(a: Real, b: Real) -> bool {
    diff(a, b) < EPSILON
}

/// GML `a > b`: `a - b >= EPSILON`.
pub fn gm_gt(a: Real, b: Real) -> bool {
    diff(a, b) >= EPSILON
}

/// GML `a >= b`: `a - b > -EPSILON`.
pub fn gm_ge(a: Real, b: Real) -> bool {
    diff(a, b) > -EPSILON
}

/// GML truthiness of a real, for `if x` in ported code: true when `x >= TRUTH_THRESHOLD`.
/// NaN is false.
pub fn gm_bool(x: Real) -> bool {
    x.get() >= TRUTH_THRESHOLD
}
