//! Paths (docs/plans/sim-core.md 3.1, M8). WP2 owns; `crates/format` (WP7b) fills it.

use crate::num::Real;

/// One path. `path::advance` is the only code that reads it.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PathDef {
    /// Package path id.
    pub name: String,
    /// Points as (x, y, speed factor). The speed factor is kept for a later rule; engine_rev 0
    /// ignores it (M8, OPEN).
    pub points: Vec<(Real, Real, Real)>,
    /// Closed path: the last point connects to the first.
    pub closed: bool,
    /// Smooth (curved) path. Not implemented: `path::advance` returns `SimError::Unsupported`
    /// (M8).
    pub smooth: bool,
    /// Smoothing precision.
    pub precision: u32,
}
