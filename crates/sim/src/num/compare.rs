//! GM8 comparisons and truthiness, under the run's [`NumRules`].
//!
//! Built-ins use these functions wherever the original compared reals in GML, and scripts call
//! them by name, because plain `==` in scripts stays exact. Every function takes the rules, so a
//! rule settled later becomes a new `engine_rev`, not a new signature (docs/plans/sim-core.md
//! 3.2).
//!
//! Under `CompareMode::Epsilon(e)` (N1, the engine_rev 0 default with `e = 1e-13`), the
//! comparisons test the difference `d = a - b`, and a difference of exactly `e` is not equal:
//!
//! | GML      | Here      | True when      |
//! |----------|-----------|----------------|
//! | `a == b` | [`gm_eq`] | `\|d\| < e`    |
//! | `a != b` | [`gm_ne`] | `\|d\| >= e`   |
//! | `a < b`  | [`gm_lt`] | `d <= -e`      |
//! | `a <= b` | [`gm_le`] | `d < e`        |
//! | `a > b`  | [`gm_gt`] | `d >= e`       |
//! | `a >= b` | [`gm_ge`] | `d > -e`       |
//!
//! The default fits every recorded trace (CLONE_SPEC numbers, Confirmed (recorder)): a tolerance
//! `t` with `9e-14 < t <= 1e-13` (`traces/little-trap/num-eps-slip-b.csv` rows 50-51,
//! `num-eps-slip-c.csv` rows 50-100), and "g equals 0.8" passes for `0.8000000000000004`
//! (`traces/go-the-jun-g-world/num-eps-alpha.csv` rows 44-45). The real runner uses a tolerance too
//! (rely's kid alpha stays 0.9999999999999999, CLONE_SPEC, Number handling, Confirmed (real
//! runner)); the exact `t` on either and whether its edge is inclusive stay OPEN (N1).
//!
//! Under `CompareMode::Exact` they are the IEEE comparisons. In both modes every comparison is
//! false when an operand is NaN (or `d` is NaN, as for `inf - inf` in epsilon mode), `!=`
//! included, so `gm_ne` is not quite `!gm_eq`. How the real runner compares NaN is OPEN.

use super::Real;
use crate::rules::NumRules;
use crate::rules::num::{CompareMode, Truth};

/// The truthiness boundary both [`Truth`] modes test against.
const HALF: f64 = 0.5;

/// The compare epsilon, or `None` for exact comparison. A zero or negative epsilon is exact.
pub(super) fn epsilon(r: &NumRules) -> Option<f64> {
    match r.compare {
        CompareMode::Epsilon(e) if e.get() > 0.0 => Some(e.get()),
        CompareMode::Epsilon(_) | CompareMode::Exact => None,
    }
}

/// True when either operand is NaN.
fn any_nan(a: Real, b: Real) -> bool {
    a.is_nan() || b.is_nan()
}

/// GML `a == b` (N1).
#[allow(
    clippy::float_cmp,
    reason = "the exact mode is the IEEE comparison by definition"
)]
pub fn gm_eq(a: Real, b: Real, r: &NumRules) -> bool {
    match epsilon(r) {
        Some(e) => (a.get() - b.get()).abs() < e,
        None => a.get() == b.get(),
    }
}

/// GML `a != b` (N1). False when an operand or the difference is NaN.
#[allow(
    clippy::float_cmp,
    reason = "the exact mode is the IEEE comparison by definition"
)]
pub fn gm_ne(a: Real, b: Real, r: &NumRules) -> bool {
    match epsilon(r) {
        Some(e) => (a.get() - b.get()).abs() >= e,
        None => !any_nan(a, b) && a.get() != b.get(),
    }
}

/// GML `a < b` (N1).
pub fn gm_lt(a: Real, b: Real, r: &NumRules) -> bool {
    match epsilon(r) {
        Some(e) => a.get() - b.get() <= -e,
        None => a.get() < b.get(),
    }
}

/// GML `a <= b` (N1).
pub fn gm_le(a: Real, b: Real, r: &NumRules) -> bool {
    match epsilon(r) {
        Some(e) => a.get() - b.get() < e,
        None => a.get() <= b.get(),
    }
}

/// GML `a > b` (N1).
pub fn gm_gt(a: Real, b: Real, r: &NumRules) -> bool {
    match epsilon(r) {
        Some(e) => a.get() - b.get() >= e,
        None => a.get() > b.get(),
    }
}

/// GML `a >= b` (N1).
pub fn gm_ge(a: Real, b: Real, r: &NumRules) -> bool {
    match epsilon(r) {
        Some(e) => a.get() - b.get() > -e,
        None => a.get() >= b.get(),
    }
}

/// GML truthiness of a real, for `if x` in ported code (N2): `x > 0.5` under `Truth::GtHalf`,
/// `x >= 0.5` under `Truth::GeHalf` (the engine_rev 0 default). NaN is false.
///
/// The spec infers `> 0.5` from the manual's "rounded value 0 or less is false" with
/// ties-to-even rounding; no trace has a fractional condition yet, so the engine_rev 0 default
/// keeps the earlier `>= 0.5` until one does (N2).
pub fn gm_bool(x: Real, r: &NumRules) -> bool {
    match r.truth {
        Truth::GtHalf => x.get() > HALF,
        Truth::GeHalf => x.get() >= HALF,
    }
}
