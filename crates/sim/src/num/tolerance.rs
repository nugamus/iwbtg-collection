//! The `standard` ruleset's one tolerance mechanism (docs/architecture/sim/numbers.md, N1).
//!
//! `gm8` compares through [`crate::rules::num::CompareMode`], whose epsilon lives in
//! [`crate::rules::NumRules`]. The `standard` ruleset compares exactly and carries no comparison
//! mode at all. Where a feel contract needs a forgiving boundary — a collision contact, a trigger
//! boundary, a save alignment — the call site declares the bound as a [`Tolerance`] and passes it
//! to one of the checks below. The value is visible at the use that needs it, cannot come from a
//! package, and is not a mode in shared code (decision 0005).
//!
//! The checks use the same edge rule as `gm8`'s `CompareMode::Epsilon`, because that is the rule
//! the number traces prove: a difference of exactly the bound is not equal and does not satisfy
//! `<=`/`>=` (CLONE_SPEC numbers, Confirmed (recorder); `traces/little-trap/num-eps-slip-b.csv`
//! rows 50-51, `-c.csv` rows 50-100). Declaring the value here changes where it applies, not what
//! the boundary means. As in `gm8`, every check is false when either operand is NaN (including
//! [`Tolerance::ne`]).
//!
//! A tolerance that decides a contact, a trigger or a save alignment is F1: it needs a feel
//! contract and the trace that justifies the value (decision 0005,
//! `docs/architecture/feel-contracts.md`). A tolerance that no contract can distinguish is
//! removed rather than carried.

use super::Real;

/// A declared absolute tolerance on a difference, in the units of the values it compares.
///
/// Construct it at the use that needs it, with the feel contract or trace that justifies the
/// value beside it; do not store it in `NumRules` and do not let a package set it. `bound` must be
/// finite and positive for the checks to be meaningful; use [`Tolerance::is_usable`] to assert
/// that in a test that declares one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tolerance(Real);

impl Tolerance {
    /// A tolerance of `bound` in the units of the values it compares.
    pub const fn new(bound: Real) -> Tolerance {
        Tolerance(bound)
    }

    /// The declared bound.
    pub const fn bound(self) -> Real {
        self.0
    }

    /// True when the bound is finite and positive, i.e. usable as a tolerance.
    pub fn is_usable(self) -> bool {
        let b = self.0.get();
        b.is_finite() && b > 0.0
    }

    /// `a == b` within the tolerance: `|a - b| < bound`. False when either operand is NaN.
    pub fn eq(self, a: Real, b: Real) -> bool {
        (a.get() - b.get()).abs() < self.0.get()
    }

    /// `a != b` beyond the tolerance: `|a - b| >= bound`. False when either operand is NaN, so
    /// this is not `!eq`.
    pub fn ne(self, a: Real, b: Real) -> bool {
        (a.get() - b.get()).abs() >= self.0.get()
    }

    /// `a < b` beyond the tolerance: `a - b <= -bound`.
    pub fn lt(self, a: Real, b: Real) -> bool {
        a.get() - b.get() <= -self.0.get()
    }

    /// `a <= b` within the tolerance: `a - b < bound`.
    pub fn le(self, a: Real, b: Real) -> bool {
        a.get() - b.get() < self.0.get()
    }

    /// `a > b` beyond the tolerance: `a - b >= bound`.
    pub fn gt(self, a: Real, b: Real) -> bool {
        a.get() - b.get() >= self.0.get()
    }

    /// `a >= b` within the tolerance: `a - b > -bound`.
    pub fn ge(self, a: Real, b: Real) -> bool {
        a.get() - b.get() > -self.0.get()
    }
}
