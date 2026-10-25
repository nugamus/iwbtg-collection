//! Overlap tests: inclusive boxes, then a pixel test over the intersection when either shape is
//! precise (Inferred; docs/plans/sim-core.md 3.4). WP0 declares; WP3 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP3 fills the bodies")]

use super::bbox::PlacedMask;
use crate::error::SimError;

/// True when the two placed masks share a solid pixel.
pub fn overlaps(a: &PlacedMask, b: &PlacedMask) -> Result<bool, SimError> {
    unimplemented!("WP3: overlap::overlaps")
}

/// True when the placed mask is solid at room pixel `(x, y)`.
pub fn point_in(m: &PlacedMask, x: i64, y: i64) -> Result<bool, SimError> {
    unimplemented!("WP3: overlap::point_in")
}

/// True when the placed mask touches the inclusive room rectangle.
pub fn rect_in(m: &PlacedMask, x1: i64, y1: i64, x2: i64, y2: i64) -> Result<bool, SimError> {
    unimplemented!("WP3: overlap::rect_in")
}
