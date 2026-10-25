//! Paths (docs/plans/sim-core.md 3.1, M8). WP0 declares; WP2 owns.

use crate::num::Real;

/// One path.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PathDef {
    /// Package path id.
    pub name: String,
    /// Points as (x, y, speed factor).
    pub points: Vec<(Real, Real, Real)>,
    /// Closed path: the last point connects to the first.
    pub closed: bool,
    /// Smooth (curved) path.
    pub smooth: bool,
    /// Smoothing precision.
    pub precision: u32,
}
