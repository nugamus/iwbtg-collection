//! Masks under scale, mirror and rotation (docs/plans/sim-core.md 3.4, C3, C4).
//!
//! This is the transform of the **collision** mask, which is not the same as the reported
//! `bbox_*` box (`bbox::reported_box`): facing left, the kid collides on `x-5..x+5` while
//! `bbox_left` reads `x-7` (CLONE_SPEC collision, Confirmed (recorder), t-col-01-a, t-col-02,
//! mask-mirror).
//!
//! Default (engine_rev 0):
//!
//! - **Mirror** (C4): a pixel at offset `d` from the origin goes to `-d` (Confirmed (recorder)
//!   for the kid's rectangle mask; the precise mask under mirror is still UNKNOWN).
//! - **Rotation** (C3): multiples of 90 degrees rotate pixel offsets exactly, counter-clockwise
//!   on screen, `(u, v) -> (u cos a + v sin a, -u sin a + v cos a)`. This is the corner rule the
//!   spec Confirms (recorder) for rotated bboxes (wanna mask-rotate.csv). OpenGMK's rotated
//!   precise spike masks are not exact rotations (544, 539, 544 and 538 pixels at 0, 180, 90 and
//!   270 degrees, kaenbyou famb-spike-mask-a), so precise pixels under rotation stay UNKNOWN.
//!   Any other angle is `SimError::Unsupported` in engine_rev 0.
//! - **Scale** other than ±1 (C3, Design): a room pixel at offset `u` samples the sprite pixel
//!   `floor(u / |scale|)`, and the box is exactly the set of offsets that sample inside the
//!   mask. Scale 0 has no pixels. Order: scale and mirror first, then rotation.
//!
//! WP3 owns.

use crate::def::IRect;
use crate::error::SimError;
use crate::num::{Real, to_int_trunc};

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

/// An inclusive offset range on one axis.
type Span = (i64, i64);

/// Counter-clockwise quarter turns for an angle that is an exact multiple of 90 degrees.
/// Any other angle is `Unsupported` (C3).
pub fn quarter_turns(angle: Real) -> Result<u8, SimError> {
    if !angle.is_finite() {
        return Err(SimError::non_finite("image_angle"));
    }
    // `rem_euclid` is exact for finite values; `+ 0.0` turns -0.0 into +0.0.
    let a = angle.get().rem_euclid(360.0) + 0.0;
    match Real::new(a) {
        r if r == Real::ZERO => Ok(0),
        r if r == Real::new(90.0) => Ok(1),
        r if r == Real::new(180.0) => Ok(2),
        r if r == Real::new(270.0) => Ok(3),
        _ => Err(SimError::unsupported(format!(
            "collision mask at image_angle {angle} (only multiples of 90 degrees; C3, MASK-ROTATE)"
        ))),
    }
}

/// An integral f64 as i64, or `Overflow`.
pub(crate) fn int_of(v: f64, what: &str) -> Result<i64, SimError> {
    to_int_trunc(Real::new(v)).map_err(|_| SimError::overflow(what))
}

/// Checked `a + b`.
pub(crate) fn add(a: i64, b: i64) -> Result<i64, SimError> {
    a.checked_add(b).ok_or_else(|| SimError::overflow("collision coordinate"))
}

/// Checked `a - b`.
pub(crate) fn sub(a: i64, b: i64) -> Result<i64, SimError> {
    a.checked_sub(b).ok_or_else(|| SimError::overflow("collision coordinate"))
}

/// Checked `-a`.
fn neg(a: i64) -> Result<i64, SimError> {
    a.checked_neg().ok_or_else(|| SimError::overflow("collision coordinate"))
}

/// True for a scale of exactly 1 or -1.
fn unit(s: Real) -> bool {
    s == Real::ONE || s == -Real::ONE
}

/// Room offsets covered by sprite offsets `lo..=hi` under `scale`, or `None` when none are
/// (scale 0, or a fractional scale that maps the whole range below one pixel).
fn axis_range(lo: i64, hi: i64, scale: Real) -> Result<Option<(i64, i64)>, SimError> {
    if !scale.is_finite() {
        return Err(SimError::non_finite("image scale"));
    }
    let s = scale.get();
    if s == 0.0 {
        return Ok(None);
    }
    let (a, b) = if unit(scale) {
        (lo, hi)
    } else {
        let m = s.abs();
        let hi1 = add(hi, 1)?;
        // u samples floor(u / m); the u with floor(u / m) in [lo, hi] are
        // ceil(lo * m) ..= ceil((hi + 1) * m) - 1.
        let a = int_of((lo as f64 * m).ceil(), "scaled mask box")?;
        let b = sub(int_of((hi1 as f64 * m).ceil(), "scaled mask box")?, 1)?;
        if b < a {
            return Ok(None);
        }
        (a, b)
    };
    if s < 0.0 { Ok(Some((neg(b)?, neg(a)?))) } else { Ok(Some((a, b))) }
}

/// The sprite offset a room offset `u` samples on one axis (inverse of [`axis_range`]), or
/// `None` for scale 0.
fn axis_sample(u: i64, scale: Real) -> Result<Option<i64>, SimError> {
    if !scale.is_finite() {
        return Err(SimError::non_finite("image scale"));
    }
    let s = scale.get();
    if s == 0.0 {
        return Ok(None);
    }
    let u = if s < 0.0 { neg(u)? } else { u };
    if unit(scale) {
        return Ok(Some(u));
    }
    let v = (u as f64 / s.abs()).floor();
    Ok(Some(int_of(v, "scaled mask sample")?))
}

/// Rotates an offset box by `turns` quarter turns counter-clockwise on screen.
fn rotate_box(x: Span, y: Span, turns: u8) -> Result<(Span, Span), SimError> {
    Ok(match turns {
        // (u, v) -> (v, -u)
        1 => (y, (neg(x.1)?, neg(x.0)?)),
        // (u, v) -> (-u, -v)
        2 => ((neg(x.1)?, neg(x.0)?), (neg(y.1)?, neg(y.0)?)),
        // (u, v) -> (-v, u)
        3 => ((neg(y.1)?, neg(y.0)?), x),
        _ => (x, y),
    })
}

/// The offset box (relative to the placed origin) that a sprite-space box covers, or `None`
/// when it covers no pixel.
pub fn offset_box(b: IRect, xf: &Xform) -> Result<Option<IRect>, SimError> {
    let turns = quarter_turns(xf.angle)?;
    let ox = i64::from(xf.origin.0);
    let oy = i64::from(xf.origin.1);
    let Some(x) = axis_range(sub(b.left, ox)?, sub(b.right, ox)?, xf.xscale)? else {
        return Ok(None);
    };
    let Some(y) = axis_range(sub(b.top, oy)?, sub(b.bottom, oy)?, xf.yscale)? else {
        return Ok(None);
    };
    let (x, y) = rotate_box(x, y, turns)?;
    Ok(Some(IRect {
        left: x.0,
        top: y.0,
        right: x.1,
        bottom: y.1,
    }))
}

/// A sprite-space box mapped to room space. A box that covers no pixel (scale 0) comes back
/// with `left > right`.
pub fn map_box(b: IRect, xf: &Xform) -> Result<IRect, SimError> {
    match offset_box(b, xf)? {
        Some(o) => Ok(IRect {
            left: add(o.left, xf.at.0)?,
            top: add(o.top, xf.at.1)?,
            right: add(o.right, xf.at.0)?,
            bottom: add(o.bottom, xf.at.1)?,
        }),
        None => Ok(IRect {
            left: 0,
            top: 0,
            right: -1,
            bottom: -1,
        }),
    }
}

/// The sprite-space pixel that a room pixel samples, or `None` when no sprite pixel maps there
/// (scale 0). The caller checks the result against the mask's box.
pub fn map_pixel(x: i64, y: i64, xf: &Xform) -> Result<Option<(i64, i64)>, SimError> {
    let turns = quarter_turns(xf.angle)?;
    let dx = sub(x, xf.at.0)?;
    let dy = sub(y, xf.at.1)?;
    // Inverse of rotate_box.
    let (u, v) = match turns {
        1 => (neg(dy)?, dx),
        2 => (neg(dx)?, neg(dy)?),
        3 => (dy, neg(dx)?),
        _ => (dx, dy),
    };
    let (Some(su), Some(sv)) = (axis_sample(u, xf.xscale)?, axis_sample(v, xf.yscale)?) else {
        return Ok(None);
    };
    Ok(Some((
        add(su, i64::from(xf.origin.0))?,
        add(sv, i64::from(xf.origin.1))?,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xf(scale: (f64, f64), angle: f64) -> Xform {
        Xform {
            at: (100, 200),
            origin: (17, 23),
            xscale: Real::new(scale.0),
            yscale: Real::new(scale.1),
            angle: Real::new(angle),
        }
    }

    const KID: IRect = IRect {
        left: 12,
        top: 11,
        right: 22,
        bottom: 31,
    };

    fn rect(l: i64, t: i64, r: i64, b: i64) -> IRect {
        IRect {
            left: l,
            top: t,
            right: r,
            bottom: b,
        }
    }

    #[test]
    fn mirror_keeps_the_kid_box() {
        assert_eq!(map_box(KID, &xf((1.0, 1.0), 0.0)).unwrap(), rect(95, 188, 105, 208));
        assert_eq!(map_box(KID, &xf((-1.0, 1.0), 0.0)).unwrap(), rect(95, 188, 105, 208));
        assert_eq!(map_box(KID, &xf((1.0, -1.0), 0.0)).unwrap(), rect(95, 192, 105, 212));
    }

    #[test]
    fn right_angles_rotate_offsets() {
        // offsets x -5..5, y -12..8
        assert_eq!(map_box(KID, &xf((1.0, 1.0), 90.0)).unwrap(), rect(88, 195, 108, 205));
        assert_eq!(map_box(KID, &xf((1.0, 1.0), 180.0)).unwrap(), rect(95, 192, 105, 212));
        assert_eq!(map_box(KID, &xf((1.0, 1.0), 270.0)).unwrap(), rect(92, 195, 112, 205));
        assert_eq!(map_box(KID, &xf((1.0, 1.0), -90.0)).unwrap(), rect(92, 195, 112, 205));
        assert_eq!(map_box(KID, &xf((1.0, 1.0), 450.0)).unwrap(), rect(88, 195, 108, 205));
    }

    #[test]
    fn other_angles_are_unsupported() {
        for a in [45.0, 89.999, 0.5] {
            assert!(matches!(
                map_box(KID, &xf((1.0, 1.0), a)),
                Err(SimError::Unsupported { .. })
            ));
        }
        assert!(matches!(
            quarter_turns(Real::new(f64::NAN)),
            Err(SimError::NonFinite { .. })
        ));
    }

    #[test]
    fn map_pixel_inverts_map_box_corners() {
        for scale in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0), (2.0, 0.5), (-0.5, 3.0)] {
            for angle in [0.0, 90.0, 180.0, 270.0] {
                let t = xf(scale, angle);
                let b = map_box(KID, &t).unwrap();
                for y in b.top - 2..=b.bottom + 2 {
                    for x in b.left - 2..=b.right + 2 {
                        let inside = (b.left..=b.right).contains(&x) && (b.top..=b.bottom).contains(&y);
                        let (sx, sy) = map_pixel(x, y, &t).unwrap().unwrap();
                        let hits = (KID.left..=KID.right).contains(&sx) && (KID.top..=KID.bottom).contains(&sy);
                        assert_eq!(inside, hits, "scale {scale:?} angle {angle} at ({x}, {y})");
                    }
                }
            }
        }
    }

    #[test]
    fn fractional_scale_and_zero_scale() {
        let spike = rect(0, 0, 31, 31);
        let t = Xform {
            at: (0, 0),
            origin: (0, 0),
            xscale: Real::new(0.5),
            yscale: Real::new(0.5),
            angle: Real::ZERO,
        };
        assert_eq!(map_box(spike, &t).unwrap(), rect(0, 0, 15, 15));
        assert_eq!(map_pixel(15, 15, &t).unwrap(), Some((30, 30)));
        let t2 = Xform {
            xscale: Real::new(2.0),
            ..t
        };
        assert_eq!(map_box(spike, &t2).unwrap(), rect(0, 0, 63, 15));
        let zero = Xform {
            xscale: Real::ZERO,
            ..t
        };
        let b = map_box(spike, &zero).unwrap();
        assert!(b.left > b.right);
        assert_eq!(map_pixel(0, 0, &zero).unwrap(), None);
    }
}
