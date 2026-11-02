//! Overlap tests: inclusive boxes, then a pixel test over the intersection when either shape is
//! precise (Inferred; docs/plans/sim-core.md 3.4). WP0 declares; WP3 owns.
//!
//! A `Rect` mask is solid on its whole placed box. A `Bits` mask (precise, and the rasterized
//! disk and diamond shapes) is solid where the room pixel samples a set bit inside the mask's
//! box (`xform::map_pixel`). An `Empty` mask is never solid.

use super::bbox::PlacedMask;
use super::xform::map_pixel;
use crate::def::{IRect, MaskShape};
use crate::error::SimError;

/// The intersection of two inclusive boxes, or `None`.
pub(crate) fn intersect(a: &IRect, b: &IRect) -> Option<IRect> {
    let r = IRect {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    (r.left <= r.right && r.top <= r.bottom).then_some(r)
}

/// True when `m` is solid at room pixel `(x, y)`, which the caller knows is inside `m.bbox`.
fn solid_inside(m: &PlacedMask, x: i64, y: i64) -> Result<bool, SimError> {
    match &m.mask.shape {
        MaskShape::Rect => Ok(true),
        MaskShape::Empty => Ok(false),
        MaskShape::Bits(bits) => {
            let Some((sx, sy)) = map_pixel(x, y, &m.xf)? else {
                return Ok(false);
            };
            let b = m.mask.bbox;
            if sx < b.left || sx > b.right || sy < b.top || sy > b.bottom {
                return Ok(false);
            }
            Ok(bits.get(sx - b.left, sy - b.top))
        }
    }
}

/// True when any pixel of `area` (inside both boxes) is solid in both masks.
fn any_shared(a: &PlacedMask, b: Option<&PlacedMask>, area: IRect) -> Result<bool, SimError> {
    for y in area.top..=area.bottom {
        for x in area.left..=area.right {
            if solid_inside(a, x, y)? && b.map_or(Ok(true), |b| solid_inside(b, x, y))? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// True when the two placed masks share a solid pixel.
pub fn overlaps(a: &PlacedMask, b: &PlacedMask) -> Result<bool, SimError> {
    let Some(area) = intersect(&a.bbox, &b.bbox) else {
        return Ok(false);
    };
    match (&a.mask.shape, &b.mask.shape) {
        (MaskShape::Empty, _) | (_, MaskShape::Empty) => Ok(false),
        (MaskShape::Rect, MaskShape::Rect) => Ok(true),
        _ => any_shared(a, Some(b), area),
    }
}

/// True when the placed mask is solid at room pixel `(x, y)`.
pub fn point_in(m: &PlacedMask, x: i64, y: i64) -> Result<bool, SimError> {
    let b = m.bbox;
    if x < b.left || x > b.right || y < b.top || y > b.bottom {
        return Ok(false);
    }
    solid_inside(m, x, y)
}

/// True when the placed mask touches the inclusive room rectangle. The corners may come in any
/// order.
pub fn rect_in(m: &PlacedMask, x1: i64, y1: i64, x2: i64, y2: i64) -> Result<bool, SimError> {
    let r = IRect {
        left: x1.min(x2),
        top: y1.min(y2),
        right: x1.max(x2),
        bottom: y1.max(y2),
    };
    let Some(area) = intersect(&m.bbox, &r) else {
        return Ok(false);
    };
    match &m.mask.shape {
        MaskShape::Rect => Ok(true),
        MaskShape::Empty => Ok(false),
        MaskShape::Bits(_) => any_shared(m, None, area),
    }
}
