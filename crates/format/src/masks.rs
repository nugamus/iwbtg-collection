//! Collision masks from RGBA pixels (docs/plans/sim-core.md 3.4, 8.1, C5, C15). WP0 declares;
//! WP7a owns.
#![allow(unused_variables, reason = "WP0 stub: WP7a fills the body")]

use iwbtg_sim::def::{IRect, Mask};

/// How a pixel's alpha is compared to the tolerance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlphaRule {
    /// In the mask when `alpha > tolerance`.
    Greater,
    /// In the mask when `alpha >= tolerance`.
    GreaterOrEqual,
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
    /// A disk inside the box, rasterized (Inferred).
    Disk,
    /// A diamond inside the box, rasterized (Inferred).
    Diamond,
}

/// Builds one mask from a frame's RGBA pixels. A manual `bbox` is shared by all frames
/// (Confirmed); `left > right` gives an empty mask (C15).
pub fn build_mask(
    rgba: &[u8],
    w: u32,
    h: u32,
    shape: CollisionShape,
    bbox: Option<IRect>,
    tolerance: u8,
) -> Mask {
    unimplemented!("WP7a: masks::build_mask")
}
