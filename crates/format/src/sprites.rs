//! Sprite strips and their sidecars (docs/plans/sim-core.md 8.1, 8.2 step 2). WP0 declares;
//! WP7a owns.
#![allow(unused_variables, reason = "WP0 stub: WP7a fills the body")]

use iwbtg_sim::def::{Mask, SpriteDef};

use crate::diag::Diagnostic;

/// A sprite's sidecar TOML (frames, origin, `collision`, `bbox`, `alpha_tolerance`,
/// `separate_masks`). WP7a adds the fields.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SpriteSidecar {}

/// Decodes a PNG strip, splits frames and builds the masks. Image pixels never reach the sim.
pub fn load_sprite(
    id: &str,
    png: &[u8],
    sidecar: &SpriteSidecar,
) -> Result<(SpriteDef, Vec<Mask>), Diagnostic> {
    unimplemented!("WP7a: sprites::load_sprite")
}
