//! The `physics.toml` sections that only families A2-A5, B and C use: `[ice]`, `[slope]`,
//! `[ladder]`, `[flip]` (docs/plans/sim-core.md 5.2, 5.3).
//!
//! Split out of `physics.rs` (WP5a) so WP5b and WP5c add their fields without editing WP5a's
//! file. WP0 declares; WP5c owns this file. WP5b's only field here is the `yflag` flip model,
//! which WP0 already lists in [`FlipKind`]; anything else WP5b needs goes through WP5c.

use serde::Deserialize;

/// `[ice]`: slip blocks and ice (WP5c).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct IceRules {}

/// `[slope]`: slope models (WP5c).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SlopeModel {}

/// `[ladder]`: ladder entry and movement (WP5c).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LadderRules {}

/// `[flip]`: how gravity flips (plan 5.3 flip rows). WP5c adds the other fields
/// (`mask_flipped`, ...).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FlipModel {
    /// `model = "..."`.
    pub model: FlipKind,
}

/// The flip models of plan 5.3, by their `physics.toml` names.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlipKind {
    /// The game has no gravity flip.
    #[default]
    None,
    /// Group A (11 games): swap to the flipped kid object at y - 4 (`flip_swap`, WP5c).
    SwapObject,
    /// Family B: the yflag (WP5b).
    Yflag,
    /// Family C: the sign of grav (WP5c).
    GravSign,
}
