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

use std::fmt;

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

/// A canonical collision shape baked from a sprite's art (`[collider]` in the sidecar).
///
/// The authoring `collision` value (`precise`, `rect`, `disk`, `diamond`) is resolved to one of
/// these forms at bake time. A degenerate mask (a transparent frame, `collision = "none"`, an
/// inverted manual box, or a bit grid with no set pixel) is [`Collider::None`]; the package never
/// spells it as an inverted rectangle. The bounding box is kept as the mask had it rather than
/// trimmed, so a baked collider reproduces the collision box and reported boxes of the mask it
/// was baked from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Collider {
    /// Nothing collides.
    None,
    /// The whole bounding box is solid.
    Rect {
        /// Bounding box in sprite space, inclusive.
        bbox: IRect,
    },
    /// The set pixels of a bit grid, relative to `bbox`.
    Bits {
        /// Bounding box in sprite space, inclusive.
        bbox: IRect,
        /// The solid pixels, `bbox`-relative.
        bits: BitMask,
    },
}

/// One collider for every frame, or one per frame ("separate collision masks").
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ColliderSet {
    /// Every frame uses this collider.
    Shared(Collider),
    /// Frame `i` uses entry `i`.
    PerFrame(Vec<Collider>),
}

/// Why a baked collider is not a canonical collider of its frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
pub enum ColliderError {
    /// An inverted box; a collider with nothing solid must be `none`.
    #[error("the bounding box is inverted; a collider with no solid pixel must be `none`")]
    Inverted,
    /// A box that is not inside the sprite frame.
    #[error("the bounding box is not inside the {width} x {height} frame")]
    Frame {
        /// Frame width in pixels.
        width: u32,
        /// Frame height in pixels.
        height: u32,
    },
    /// A bit grid whose size differs from its bounding box.
    #[error("the bit grid is {grid_w} x {grid_h}, but the bounding box is {bbox_w} x {bbox_h}")]
    Grid {
        /// Bounding-box width in pixels.
        bbox_w: u32,
        /// Bounding-box height in pixels.
        bbox_h: u32,
        /// Grid width in pixels.
        grid_w: u32,
        /// Grid height in pixels.
        grid_h: u32,
    },
    /// A bit grid with no set pixel; the canonical form is `none`.
    #[error("the bit grid has no solid pixel; a collider with nothing solid must be `none`")]
    EmptyBits,
    /// A per-frame set whose length differs from the sprite's frame count.
    #[error("the collider has {found} frame(s), but the sprite has {expected}")]
    FrameCount {
        /// The sprite's frame count.
        expected: u16,
        /// The collider set's length.
        found: usize,
    },
}

/// Why hex rows do not form a canonical bit grid.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum BitGridError {
    /// A row count that differs from the box height.
    #[error("the grid has {found} row(s), but the box is {expected} row(s) high")]
    RowCount {
        /// Rows supplied.
        found: usize,
        /// Rows the box needs.
        expected: usize,
    },
    /// A row whose digit count differs from the box width.
    #[error("row {row} has {found} hex digit(s), but the box is {width} pixels wide (needs {expected})")]
    RowLength {
        /// Zero-based row.
        row: usize,
        /// Digits supplied.
        found: usize,
        /// Digits the width needs.
        expected: usize,
        /// Width in pixels.
        width: u32,
    },
    /// A character that is not a hex digit.
    #[error("row {row} has a non-hex character `{ch}`")]
    NotHex {
        /// Zero-based row.
        row: usize,
        /// The offending character.
        ch: char,
    },
    /// A set bit past the box width (the last digit's padding).
    #[error("row {row} sets a padding bit past the {width}-pixel width")]
    Padding {
        /// Zero-based row.
        row: usize,
        /// Width in pixels.
        width: u32,
    },
    /// A grid past [`iwbtg_sim::def::MAX_MASK_WORDS`].
    #[error("a {width} x {height} bit grid is too large")]
    TooLarge {
        /// Grid width in pixels.
        width: u32,
        /// Grid height in pixels.
        height: u32,
    },
}

impl Collider {
    /// The canonical collider of a mask built from art. An empty mask or an empty bit grid is
    /// [`Collider::None`].
    pub fn from_mask(mask: &Mask) -> Collider {
        match &mask.shape {
            MaskShape::Empty => Collider::None,
            MaskShape::Rect => Collider::Rect { bbox: mask.bbox },
            MaskShape::Bits(bits) if bits.count() == 0 => Collider::None,
            MaskShape::Bits(bits) => Collider::Bits {
                bbox: mask.bbox,
                bits: bits.clone(),
            },
        }
    }

    /// The runtime mask this collider stands for. [`Collider::None`] becomes the sim's internal
    /// empty mask; the loader keeps using the mask it sampled from the art until #1182.
    pub fn to_mask(&self) -> Mask {
        match self {
            Collider::None => empty_mask(),
            Collider::Rect { bbox } => Mask {
                bbox: *bbox,
                shape: MaskShape::Rect,
            },
            Collider::Bits { bbox, bits } => Mask {
                bbox: *bbox,
                shape: MaskShape::Bits(bits.clone()),
            },
        }
    }

    /// True for the explicit no-collision form.
    pub const fn is_none(&self) -> bool {
        matches!(self, Collider::None)
    }

    /// Number of solid pixels: `Rect` covers its box, `Bits` its set bits, `None` zero.
    pub fn solid_pixels(&self) -> u64 {
        match self {
            Collider::None => 0,
            Collider::Rect { bbox } => {
                let w = (bbox.right - bbox.left + 1).max(0);
                let h = (bbox.bottom - bbox.top + 1).max(0);
                u64::try_from(w.saturating_mul(h)).unwrap_or(u64::MAX)
            }
            Collider::Bits { bits, .. } => u64::from(bits.count()),
        }
    }

    /// Checks the canonical rules for a `frame_size` frame: a box inside the frame, a bit grid
    /// matching its box, and at least one solid pixel.
    pub fn validate(&self, frame_size: (u32, u32)) -> Result<(), ColliderError> {
        let frame = ColliderError::Frame {
            width: frame_size.0,
            height: frame_size.1,
        };
        match self {
            Collider::None => Ok(()),
            Collider::Rect { bbox } => check_box(*bbox, frame_size, frame),
            Collider::Bits { bbox, bits } => {
                check_box(*bbox, frame_size, frame)?;
                let bbox_w = u32::try_from(bbox.right - bbox.left + 1).map_err(|_| frame)?;
                let bbox_h = u32::try_from(bbox.bottom - bbox.top + 1).map_err(|_| frame)?;
                if bits.width() != bbox_w || bits.height() != bbox_h {
                    return Err(ColliderError::Grid {
                        bbox_w,
                        bbox_h,
                        grid_w: bits.width(),
                        grid_h: bits.height(),
                    });
                }
                if bits.count() == 0 {
                    return Err(ColliderError::EmptyBits);
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for Collider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Collider::None => f.write_str("none"),
            Collider::Rect { bbox } => write!(
                f,
                "rect {} x {} at [{}, {}]",
                bbox.right - bbox.left + 1,
                bbox.bottom - bbox.top + 1,
                bbox.left,
                bbox.top
            ),
            Collider::Bits { bbox, bits } => write!(
                f,
                "bits {} x {} with {} solid pixel(s) at [{}, {}]",
                bits.width(),
                bits.height(),
                bits.count(),
                bbox.left,
                bbox.top
            ),
        }
    }
}

impl ColliderSet {
    /// Canonicalizes the masks the loader built for a sprite. `per_frame` is the loader's
    /// separate-mask decision.
    pub fn from_masks(per_frame: bool, masks: &[Mask]) -> ColliderSet {
        if per_frame {
            ColliderSet::PerFrame(masks.iter().map(Collider::from_mask).collect())
        } else {
            ColliderSet::Shared(masks.first().map_or(Collider::None, Collider::from_mask))
        }
    }

    /// The collider frame `frame` uses; a shared set answers for every frame.
    pub fn get(&self, frame: usize) -> Option<&Collider> {
        match self {
            ColliderSet::Shared(collider) => Some(collider),
            ColliderSet::PerFrame(colliders) => colliders.get(frame),
        }
    }

    /// Validates every entry and a per-frame set's length against the sprite's frames.
    pub fn validate(&self, frame_size: (u32, u32), frames: u16) -> Result<(), ColliderError> {
        match self {
            ColliderSet::Shared(collider) => collider.validate(frame_size),
            ColliderSet::PerFrame(colliders) => {
                if colliders.len() != usize::from(frames) {
                    return Err(ColliderError::FrameCount {
                        expected: frames,
                        found: colliders.len(),
                    });
                }
                colliders.iter().try_for_each(|c| c.validate(frame_size))
            }
        }
    }
}

/// Checks that a box is non-inverted and inside `frame_size`.
fn check_box(bbox: IRect, frame_size: (u32, u32), frame: ColliderError) -> Result<(), ColliderError> {
    if bbox.left > bbox.right || bbox.top > bbox.bottom {
        return Err(ColliderError::Inverted);
    }
    if bbox.left < 0
        || bbox.top < 0
        || bbox.right >= i64::from(frame_size.0)
        || bbox.bottom >= i64::from(frame_size.1)
    {
        return Err(frame);
    }
    Ok(())
}

/// The canonical hex rows of a bit grid: one row top to bottom, the most significant bit of each
/// digit first, `ceil(width / 4)` digits. The bake tool and the format example use this form.
pub fn bits_to_hex_rows(bits: &BitMask) -> Vec<String> {
    let digits = usize::try_from(bits.width().div_ceil(4)).unwrap_or(0);
    (0..bits.height())
        .map(|y| {
            let mut row = String::with_capacity(digits);
            for d in 0..digits {
                let mut digit = 0u8;
                for bit in 0..4 {
                    let x = u32::try_from(d).unwrap_or(u32::MAX).saturating_mul(4) + bit;
                    if x < bits.width() && bits.get(i64::from(x), i64::from(y)) {
                        digit |= 8 >> bit;
                    }
                }
                row.push(char::from_digit(u32::from(digit), 16).unwrap_or('0'));
            }
            row
        })
        .collect()
}

/// Parses canonical hex rows into a `width` x `height` bit grid. Each row needs `ceil(width / 4)`
/// digits; bits past `width` must be zero so a grid has one spelling.
pub fn hex_rows_to_bits(rows: &[String], width: u32, height: u32) -> Result<BitMask, BitGridError> {
    let too_large = BitGridError::TooLarge { width, height };
    let mut bits = BitMask::new(width, height).map_err(|_| too_large)?;
    let expected = usize::try_from(width.div_ceil(4)).unwrap_or(usize::MAX);
    if rows.len() != usize::try_from(height).unwrap_or(usize::MAX) {
        return Err(BitGridError::RowCount {
            found: rows.len(),
            expected: usize::try_from(height).unwrap_or(usize::MAX),
        });
    }
    for (y, row) in rows.iter().enumerate() {
        let found = row.chars().count();
        if found != expected {
            return Err(BitGridError::RowLength {
                row: y,
                found,
                expected,
                width,
            });
        }
        for (d, ch) in row.chars().enumerate() {
            let digit = ch.to_digit(16).ok_or(BitGridError::NotHex { row: y, ch })?;
            for bit in 0..4 {
                let x = u32::try_from(d).unwrap_or(u32::MAX).saturating_mul(4) + bit;
                let on = digit >> (3 - bit) & 1 == 1;
                if x < width {
                    bits.set(i64::from(x), i64::try_from(y).unwrap_or(i64::MAX), on);
                } else if on {
                    return Err(BitGridError::Padding { row: y, width });
                }
            }
        }
    }
    Ok(bits)
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

    #[test]
    fn degenerate_masks_canonicalize_to_none() {
        // A fully transparent automatic box, an inverted manual box and a manual box whose
        // precise pixels are all transparent are one explicit form: None.
        let (px, w, h) = frame(&["..", ".."]);
        let transparent = build_mask(&px, w, h, CollisionShape::Precise, None, 0);
        assert_eq!(Collider::from_mask(&transparent), Collider::None);

        let (px, w, h) = frame(&["##", "##"]);
        let inverted = build_mask(
            &px,
            w,
            h,
            CollisionShape::Rect,
            Some(IRect {
                left: 1,
                top: 1,
                right: 0,
                bottom: 0,
            }),
            0,
        );
        assert_eq!(inverted.shape, MaskShape::Empty);
        assert_eq!(Collider::from_mask(&inverted), Collider::None);

        let empty_bits = build_mask(
            &px,
            w,
            h,
            CollisionShape::Precise,
            Some(IRect {
                left: 0,
                top: 0,
                right: 1,
                bottom: 1,
            }),
            255,
        );
        assert!(matches!(empty_bits.shape, MaskShape::Bits(_)));
        assert_eq!(Collider::from_mask(&empty_bits), Collider::None);
        assert_eq!(Collider::None.to_mask(), empty_mask());
    }

    #[test]
    fn collider_round_trips_a_mask() {
        let (px, w, h) = frame(&["#.", ".#"]);
        let bits_mask = build_mask(&px, w, h, CollisionShape::Precise, None, 0);
        assert_eq!(Collider::from_mask(&bits_mask).to_mask(), bits_mask);
        let rect_mask = build_mask(&px, w, h, CollisionShape::Rect, None, 0);
        assert_eq!(Collider::from_mask(&rect_mask).to_mask(), rect_mask);
        let mut bit_grid = BitMask::new(2, 2).unwrap();
        bit_grid.set(1, 0, true);
        let collider = Collider::Bits {
            bbox: bits_mask.bbox,
            bits: bit_grid.clone(),
        };
        assert_eq!(
            collider.to_mask(),
            Mask {
                bbox: bits_mask.bbox,
                shape: MaskShape::Bits(bit_grid),
            }
        );
        assert_eq!(collider.solid_pixels(), 1);
        assert_eq!(rect_mask.bbox.left, bits_mask.bbox.left);
        assert_eq!(Collider::from_mask(&rect_mask).solid_pixels(), 4);
    }

    #[test]
    fn collider_validate_rejects_non_canonical_forms() {
        let frame_size = (4, 4);
        let outside = IRect {
            left: 0,
            top: 0,
            right: 4,
            bottom: 1,
        };
        assert_eq!(
            Collider::Rect { bbox: outside }.validate(frame_size),
            Err(ColliderError::Frame { width: 4, height: 4 })
        );
        let inverted = IRect {
            left: 2,
            top: 2,
            right: 1,
            bottom: 1,
        };
        assert_eq!(
            Collider::Rect { bbox: inverted }.validate(frame_size),
            Err(ColliderError::Inverted)
        );
        let bbox = IRect {
            left: 0,
            top: 0,
            right: 1,
            bottom: 1,
        };
        let wrong_grid = Collider::Bits {
            bbox,
            bits: BitMask::new(1, 1).unwrap(),
        };
        assert_eq!(
            wrong_grid.validate(frame_size),
            Err(ColliderError::Grid {
                bbox_w: 2,
                bbox_h: 2,
                grid_w: 1,
                grid_h: 1,
            })
        );
        let empty_grid = Collider::Bits {
            bbox,
            bits: BitMask::new(2, 2).unwrap(),
        };
        assert_eq!(empty_grid.validate(frame_size), Err(ColliderError::EmptyBits));
        assert_eq!(Collider::None.validate(frame_size), Ok(()));
        assert_eq!(
            ColliderSet::PerFrame(vec![Collider::None]).validate(frame_size, 2),
            Err(ColliderError::FrameCount {
                expected: 2,
                found: 1
            })
        );
    }

    #[test]
    fn hex_rows_round_trip_and_reject_padding() {
        let bbox = IRect {
            left: 0,
            top: 0,
            right: 4,
            bottom: 1,
        };
        let mut grid = BitMask::new(5, 2).unwrap();
        grid.set(0, 0, true);
        grid.set(4, 1, true);
        let rows = bits_to_hex_rows(&grid);
        assert_eq!(rows, vec!["80".to_owned(), "08".to_owned()]);
        assert_eq!(hex_rows_to_bits(&rows, 5, 2).unwrap(), grid);
        assert_eq!(
            hex_rows_to_bits(&["8f".to_owned(), "00".to_owned()], 5, 2),
            Err(BitGridError::Padding { row: 0, width: 5 })
        );
        assert_eq!(
            hex_rows_to_bits(&["8".to_owned(), "00".to_owned()], 5, 2),
            Err(BitGridError::RowLength {
                row: 0,
                found: 1,
                expected: 2,
                width: 5
            })
        );
        assert_eq!(
            hex_rows_to_bits(&["zz".to_owned(), "00".to_owned()], 5, 2),
            Err(BitGridError::NotHex { row: 0, ch: 'z' })
        );
        assert_eq!(
            hex_rows_to_bits(&["80".to_owned()], 5, 2),
            Err(BitGridError::RowCount {
                found: 1,
                expected: 2
            })
        );
        assert_eq!(
            ColliderSet::from_masks(true, &[]),
            ColliderSet::PerFrame(Vec::new())
        );
        assert_eq!(
            ColliderSet::from_masks(false, &[]),
            ColliderSet::Shared(Collider::None)
        );
        assert_eq!(Collider::Bits { bbox, bits: grid }.to_mask().bbox, bbox);
    }
}
