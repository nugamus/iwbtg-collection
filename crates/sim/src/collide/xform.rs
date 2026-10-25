//! Masks under scale, mirror and rotation (docs/plans/sim-core.md 3.4, C3, C4).
//!
//! Default: offsets from the origin mirror `d -> -d` and rotate by multiples of 90 degrees
//! exactly; fractional scale scales offsets and floors; other angles are
//! `SimError::Unsupported` in engine_rev 0. WP0 declares; WP3 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP3 fills the bodies")]

use crate::def::IRect;
use crate::error::SimError;
use crate::num::Real;

/// Placement transform of one mask: integer origin in room space, sprite origin, scale, angle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Xform {
    /// Rounded room position of the instance origin.
    pub at: (i64, i64),
    /// Sprite origin.
    pub origin: (i32, i32),
    /// `image_xscale`.
    pub xscale: Real,
    /// `image_yscale`.
    pub yscale: Real,
    /// `image_angle`.
    pub angle: Real,
}

/// A sprite-space box mapped to room space.
pub fn map_box(b: IRect, xf: &Xform) -> Result<IRect, SimError> {
    unimplemented!("WP3: xform::map_box")
}

/// The sprite-space pixel that a room pixel samples, or `None` when it maps outside the mask.
pub fn map_pixel(x: i64, y: i64, xf: &Xform) -> Result<Option<(i64, i64)>, SimError> {
    unimplemented!("WP3: xform::map_pixel")
}
