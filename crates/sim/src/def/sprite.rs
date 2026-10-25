//! Sprites and collision masks as the sim sees them (docs/plans/sim-core.md 3.4).
//!
//! `crates/format` builds these from PNG strips at load. Image pixels never reach the sim; only
//! mask bits do. WP0 declares; WP7a owns afterwards.

use super::ids::MaskId;

/// An integer rectangle with inclusive edges. In sprite space for a [`Mask`], in room space for
/// a placed mask (`collide::bbox::PlacedMask`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct IRect {
    /// Leftmost column inside the rectangle.
    pub left: i64,
    /// Top row inside the rectangle.
    pub top: i64,
    /// Rightmost column inside the rectangle.
    pub right: i64,
    /// Bottom row inside the rectangle.
    pub bottom: i64,
}

/// A sprite: frame count, size, origin and its masks.
#[derive(Clone, PartialEq, Debug)]
pub struct SpriteDef {
    /// Number of frames; 0 for an empty sprite.
    pub frames: u16,
    /// Frame size in pixels (width, height).
    pub size: (u32, u32),
    /// Origin in sprite space (x, y).
    pub origin: (i32, i32),
    /// One mask for all frames, or one per frame ("separate collision masks", Confirmed).
    pub masks: MaskSet,
}

/// Which masks a sprite's frames use.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MaskSet {
    /// Every frame uses this mask.
    Shared(MaskId),
    /// Frame `i` uses entry `i`.
    PerFrame(Vec<MaskId>),
}

/// One collision mask.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Mask {
    /// Bounding box in sprite space, inclusive. Only pixels inside it count (Confirmed).
    pub bbox: IRect,
    /// What is solid inside the box.
    pub shape: MaskShape,
}

/// What is solid inside a mask's bounding box.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MaskShape {
    /// The whole box.
    Rect,
    /// The set bits, bbox-relative (precise masks, and rasterized disk and diamond shapes).
    Bits(BitMask),
    /// Nothing collides (a `left > right` box such as sprBow, Inferred, C15).
    Empty,
}

/// A bit grid, row-major, each row padded to whole `u64` words. Bit `x % 64` of word
/// `y * words_per_row + x / 64` is pixel `(x, y)`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct BitMask {
    width: u32,
    height: u32,
    words: Vec<u64>,
}

impl BitMask {
    /// An all-clear grid of `width` x `height` pixels.
    pub fn new(width: u32, height: u32) -> BitMask {
        let len = u64::from(BitMask::row_words(width)) * u64::from(height);
        BitMask {
            width,
            height,
            words: vec![0; usize::try_from(len).unwrap_or(0)],
        }
    }

    /// Words per row for a width.
    const fn row_words(width: u32) -> u32 {
        width.div_ceil(64)
    }

    /// Width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The word index and bit for a pixel, or `None` outside the grid.
    fn locate(&self, x: i64, y: i64) -> Option<(usize, u32)> {
        let x = u32::try_from(x).ok().filter(|&x| x < self.width)?;
        let y = u32::try_from(y).ok().filter(|&y| y < self.height)?;
        let word = u64::from(y) * u64::from(BitMask::row_words(self.width)) + u64::from(x / 64);
        Some((usize::try_from(word).ok()?, x % 64))
    }

    /// The pixel at `(x, y)`; false outside the grid.
    pub fn get(&self, x: i64, y: i64) -> bool {
        self.locate(x, y)
            .and_then(|(w, b)| self.words.get(w).map(|word| word >> b & 1 == 1))
            .unwrap_or(false)
    }

    /// Sets the pixel at `(x, y)`. Ignored outside the grid.
    pub fn set(&mut self, x: i64, y: i64, on: bool) {
        if let Some((w, b)) = self.locate(x, y)
            && let Some(word) = self.words.get_mut(w)
        {
            if on {
                *word |= 1 << b;
            } else {
                *word &= !(1 << b);
            }
        }
    }

    /// Number of set pixels.
    pub fn count(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmask_get_set_across_words() {
        let mut m = BitMask::new(70, 3);
        assert_eq!(m.count(), 0);
        m.set(0, 0, true);
        m.set(65, 2, true);
        m.set(70, 0, true); // outside: ignored
        m.set(-1, 0, true); // outside: ignored
        assert!(m.get(0, 0));
        assert!(m.get(65, 2));
        assert!(!m.get(64, 2));
        assert!(!m.get(70, 0));
        assert_eq!(m.count(), 2);
        m.set(0, 0, false);
        assert_eq!(m.count(), 1);
    }
}
