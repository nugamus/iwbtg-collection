//! Collision masks from RGBA pixels (docs/plans/sim-core.md 3.4, 8.1, C5, C15).
//!
//! Rules (CLONE_SPEC collision, "How the manual defines masks", Confirmed manual 202_00):
//! only pixels inside the bounding box count; the shape is precise (per pixel), the box, a disk
//! or a diamond inside the box; without separate masks one mask combines all subimages; a manual
//! box is one box for all subimages. A box with `left > right` or `top > bottom` is empty
//! (Inferred, C15).
//!
//! Open items, isolated here:
//! - C5 (UNKNOWN): whether a precise pixel counts when `alpha > tolerance` or `>= tolerance`
//!   ([`ALPHA_IN_MASK`]).
//! - How GM rasterizes disk and diamond shapes is not in the spec. [`disk_contains`] and
//!   [`diamond_contains`] are our own rule (Design): a pixel is in when its centre is inside the
//!   ellipse or diamond inscribed in the box. Integer math only, so every target agrees.
//! - The automatic box (`bbox` absent) is the tightest box around the pixels that pass the alpha
//!   rule, over the frames the mask covers (Inferred from the manual's "automatic" mode).

use iwbtg_sim::def::{BitMask, IRect, Mask, MaskShape};

/// How a pixel's alpha is compared to the tolerance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlphaRule {
    /// In the mask when `alpha > tolerance`.
    Greater,
    /// In the mask when `alpha >= tolerance`.
    GreaterOrEqual,
}

impl AlphaRule {
    /// True when a pixel of this alpha is in a precise mask.
    pub const fn passes(self, alpha: u8, tolerance: u8) -> bool {
        match self {
            AlphaRule::Greater => alpha > tolerance,
            AlphaRule::GreaterOrEqual => alpha >= tolerance,
        }
    }
}

/// C5 (UNKNOWN): the alpha rule. Read only at load; a change bumps `gameplay_hash`.
pub const ALPHA_IN_MASK: AlphaRule = AlphaRule::Greater;

/// A sprite's collision shape (`collision` in the sidecar).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CollisionShape {
    /// Precise, per pixel.
    Precise,
    /// The bounding box.
    Rect,
    /// A disk inside the box, rasterized (Design, see the module docs).
    Disk,
    /// A diamond inside the box, rasterized (Design, see the module docs).
    Diamond,
}

/// The empty mask: no box, nothing collides.
pub fn empty_mask() -> Mask {
    Mask {
        bbox: IRect {
            left: 0,
            top: 0,
            right: -1,
            bottom: -1,
        },
        shape: MaskShape::Empty,
    }
}

/// Builds one mask from a frame's RGBA pixels. A manual `bbox` is shared by all frames
/// (Confirmed); `left > right` gives an empty mask (C15).
///
/// `rgba` is `w * h` pixels, row-major; missing bytes count as transparent. Bits are relative to
/// the box. A box larger than [`iwbtg_sim::def::MAX_MASK_WORDS`] allows cannot be stored as bits;
/// `sprites::load_sprite` refuses such sprites before it gets here, and this function then falls
/// back to the plain box.
pub fn build_mask(
    rgba: &[u8],
    w: u32,
    h: u32,
    shape: CollisionShape,
    bbox: Option<IRect>,
    tolerance: u8,
) -> Mask {
    build_union_mask(&[rgba], w, h, shape, bbox, tolerance)
}

/// Builds one mask that combines several frames of the same size (GM's default "one mask for
/// all subimages", Confirmed manual 202_00): a pixel is in when it is in for any frame.
pub fn build_union_mask(
    frames: &[&[u8]],
    w: u32,
    h: u32,
    shape: CollisionShape,
    bbox: Option<IRect>,
    tolerance: u8,
) -> Mask {
    let solid = |x: i64, y: i64| frames.iter().any(|f| pixel_in(f, w, h, x, y, tolerance));
    let bbox = match bbox {
        Some(b) => b,
        None => match automatic_box(w, h, &solid) {
            Some(b) => b,
            None => return empty_mask(),
        },
    };
    if bbox.left > bbox.right || bbox.top > bbox.bottom {
        return Mask {
            bbox,
            shape: MaskShape::Empty,
        };
    }
    let rect = Mask {
        bbox,
        shape: MaskShape::Rect,
    };
    if shape == CollisionShape::Rect {
        return rect;
    }
    let (Ok(bw), Ok(bh)) = (
        u32::try_from(bbox.right - bbox.left + 1),
        u32::try_from(bbox.bottom - bbox.top + 1),
    ) else {
        return rect;
    };
    let Ok(mut bits) = BitMask::new(bw, bh) else {
        return rect;
    };
    for j in 0..i64::from(bh) {
        for i in 0..i64::from(bw) {
            let on = match shape {
                CollisionShape::Precise => solid(bbox.left + i, bbox.top + j),
                CollisionShape::Disk => disk_contains(bw, bh, i, j),
                CollisionShape::Diamond => diamond_contains(bw, bh, i, j),
                CollisionShape::Rect => true,
            };
            if on {
                bits.set(i, j, true);
            }
        }
    }
    Mask {
        bbox,
        shape: MaskShape::Bits(bits),
    }
}

/// True when pixel `(x, y)` of a `w` x `h` RGBA frame passes [`ALPHA_IN_MASK`]. Outside the
/// frame, or past the end of `rgba`, it does not.
fn pixel_in(rgba: &[u8], w: u32, h: u32, x: i64, y: i64, tolerance: u8) -> bool {
    let (Ok(x), Ok(y)) = (u64::try_from(x), u64::try_from(y)) else {
        return false;
    };
    if x >= u64::from(w) || y >= u64::from(h) {
        return false;
    }
    let alpha_at = (y * u64::from(w) + x) * 4 + 3;
    usize::try_from(alpha_at)
        .ok()
        .and_then(|i| rgba.get(i))
        .is_some_and(|&a| ALPHA_IN_MASK.passes(a, tolerance))
}

/// The tightest box around the pixels for which `solid` holds, or `None` when there are none.
fn automatic_box(w: u32, h: u32, solid: &impl Fn(i64, i64) -> bool) -> Option<IRect> {
    let mut found: Option<IRect> = None;
    for y in 0..i64::from(h) {
        for x in 0..i64::from(w) {
            if solid(x, y) {
                let b = found.get_or_insert(IRect {
                    left: x,
                    top: y,
                    right: x,
                    bottom: y,
                });
                b.left = b.left.min(x);
                b.right = b.right.max(x);
                b.top = b.top.min(y);
                b.bottom = b.bottom.max(y);
            }
        }
    }
    found
}

/// Offsets from the box centre in half pixels: pixel `i` of `n` has its centre at `2i + 1 - n`.
fn doubled_offset(i: i64, n: u32) -> i128 {
    2 * i128::from(i) + 1 - i128::from(n)
}

/// Design: pixel `(i, j)` of a `w` x `h` box is in the disk when its centre lies inside the
/// inscribed ellipse: `(X/w)^2 + (Y/h)^2 <= 1` with half-pixel offsets `X`, `Y`.
pub fn disk_contains(w: u32, h: u32, i: i64, j: i64) -> bool {
    let (x, y) = (doubled_offset(i, w), doubled_offset(j, h));
    let (w, h) = (i128::from(w), i128::from(h));
    x * x * h * h + y * y * w * w <= w * w * h * h
}

/// Design: pixel `(i, j)` of a `w` x `h` box is in the diamond when its centre lies inside the
/// inscribed diamond: `|X|/w + |Y|/h <= 1` with half-pixel offsets `X`, `Y`.
pub fn diamond_contains(w: u32, h: u32, i: i64, j: i64) -> bool {
    let (x, y) = (doubled_offset(i, w), doubled_offset(j, h));
    let (w, h) = (i128::from(w), i128::from(h));
    x.abs() * h + y.abs() * w <= w * h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w` x `h` RGBA frame from rows of `#` (alpha 255), `+` (alpha 100) and `.` (alpha 0).
    fn frame(rows: &[&str]) -> (Vec<u8>, u32, u32) {
        let h = rows.len() as u32;
        let w = rows[0].len() as u32;
        let mut px = Vec::new();
        for r in rows {
            for c in r.chars() {
                let a = match c {
                    '#' => 255,
                    '+' => 100,
                    _ => 0,
                };
                px.extend_from_slice(&[0, 0, 0, a]);
            }
        }
        (px, w, h)
    }

    fn bits(m: &Mask) -> &BitMask {
        match &m.shape {
            MaskShape::Bits(b) => b,
            other => panic!("not bits: {other:?}"),
        }
    }

    #[test]
    fn precise_automatic_box_and_tolerance() {
        let (px, w, h) = frame(&["....", ".#+.", "..#.", "...."]);
        let m = build_mask(&px, w, h, CollisionShape::Precise, None, 0);
        assert_eq!(
            m.bbox,
            IRect {
                left: 1,
                top: 1,
                right: 2,
                bottom: 2
            }
        );
        assert_eq!(bits(&m).count(), 3);
        // Tolerance 100 with `>` drops the alpha-100 pixel (C5 default) and shrinks the box.
        let m = build_mask(&px, w, h, CollisionShape::Precise, None, 100);
        assert_eq!(bits(&m).count(), 2);
        assert!(bits(&m).get(0, 0) && bits(&m).get(1, 1) && !bits(&m).get(1, 0));
    }

    #[test]
    fn manual_box_clips_and_may_exceed_the_frame() {
        let (px, w, h) = frame(&["##", "##"]);
        let b = IRect {
            left: 1,
            top: 0,
            right: 2,
            bottom: 0,
        };
        let m = build_mask(&px, w, h, CollisionShape::Precise, Some(b), 0);
        assert_eq!(m.bbox, b);
        // Column 1 is inside the frame, column 2 is not.
        assert!(bits(&m).get(0, 0));
        assert!(!bits(&m).get(1, 0));
        assert_eq!(
            build_mask(&px, w, h, CollisionShape::Rect, Some(b), 0).shape,
            MaskShape::Rect
        );
    }

    #[test]
    fn inverted_box_is_empty_and_transparent_sprite_is_empty() {
        let (px, w, h) = frame(&["##", "##"]);
        let b = IRect {
            left: 31,
            top: 31,
            right: 0,
            bottom: 0,
        };
        assert_eq!(
            build_mask(&px, w, h, CollisionShape::Rect, Some(b), 0).shape,
            MaskShape::Empty
        );
        let (px, w, h) = frame(&["..", ".."]);
        assert_eq!(
            build_mask(&px, w, h, CollisionShape::Precise, None, 0).shape,
            MaskShape::Empty
        );
    }

    #[test]
    fn union_combines_frames() {
        let (a, w, h) = frame(&["#.", ".."]);
        let (b, _, _) = frame(&["..", ".#"]);
        let m = build_union_mask(&[&a, &b], w, h, CollisionShape::Precise, None, 0);
        assert_eq!(bits(&m).count(), 2);
        assert_eq!(m.bbox.right, 1);
        assert_eq!(m.bbox.bottom, 1);
    }

    #[test]
    fn disk_and_diamond_shapes() {
        let (px, w, h) = frame(&["#####"; 5]);
        let disk = build_mask(&px, w, h, CollisionShape::Disk, None, 0);
        // Only the four corners are out.
        assert_eq!(bits(&disk).count(), 21);
        assert!(!bits(&disk).get(0, 0) && bits(&disk).get(1, 0) && bits(&disk).get(1, 1));
        let diamond = build_mask(&px, w, h, CollisionShape::Diamond, None, 0);
        assert_eq!(bits(&diamond).count(), 13);
        assert!(!bits(&diamond).get(1, 0) && bits(&diamond).get(2, 0) && bits(&diamond).get(1, 1));
        // A 1 x 1 disk is one pixel; a 4 x 4 blood-sized disk keeps its centre.
        assert!(disk_contains(1, 1, 0, 0));
        assert!(diamond_contains(1, 1, 0, 0));
    }

    #[test]
    fn short_pixel_data_is_transparent() {
        let m = build_mask(&[0, 0, 0, 255], 2, 2, CollisionShape::Precise, None, 0);
        assert_eq!(bits(&m).count(), 1);
    }
}
