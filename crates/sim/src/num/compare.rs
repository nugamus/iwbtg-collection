//! GM8 comparisons and truthiness.
//!
//! GML compares reals with a tolerance: two reals are equal when their difference is at most
//! [`EPSILON`]. Built-ins use these functions wherever the original compared reals in GML, and
//! scripts call them by name, because plain `==` in scripts stays exact.

use super::Real;

/// Tolerance on the difference for GM8 real comparisons. Value from OpenGMK, which the traces
/// were recorded with; docs/architecture.md, "Comparisons".
///
/// OPEN: confirm in CLONE_SPEC (value, and whether the bound is `<=` or `<`; we use `<=`).
pub const EPSILON: f64 = 1e-13;

/// GML truthiness boundary: a real is true when it is greater than this.
///
/// OPEN: confirm in CLONE_SPEC whether the boundary itself (0.5) counts as true
/// (decision 0001 says "from 0.5"; this task's spec says "> 0.5").
pub const TRUTH_THRESHOLD: f64 = 0.5;

/// GML `a == b`: `|a - b| <= EPSILON`. False if either side is NaN.
pub fn gm_eq(a: Real, b: Real) -> bool {
    (a.get() - b.get()).abs() <= EPSILON
}

/// GML `a != b`: not [`gm_eq`].
pub fn gm_ne(a: Real, b: Real) -> bool {
    !gm_eq(a, b)
}

/// GML `a < b`: smaller and not [`gm_eq`].
pub fn gm_lt(a: Real, b: Real) -> bool {
    a.get() < b.get() && !gm_eq(a, b)
}

/// GML `a <= b`: smaller or [`gm_eq`].
pub fn gm_le(a: Real, b: Real) -> bool {
    a.get() < b.get() || gm_eq(a, b)
}

/// GML `a > b`: larger and not [`gm_eq`].
pub fn gm_gt(a: Real, b: Real) -> bool {
    a.get() > b.get() && !gm_eq(a, b)
}

/// GML `a >= b`: larger or [`gm_eq`].
pub fn gm_ge(a: Real, b: Real) -> bool {
    a.get() > b.get() || gm_eq(a, b)
}

/// GML truthiness of a real, for `if x` in ported code: true when `x > TRUTH_THRESHOLD`.
/// NaN is false.
pub fn gm_bool(x: Real) -> bool {
    x.get() > TRUTH_THRESHOLD
}
