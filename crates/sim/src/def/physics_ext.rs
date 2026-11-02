//! The `physics.toml` sections that only families A2-A5, B and C use: `[ice]`, `[slope]`,
//! `[ladder]`, `[flip]` (docs/plans/sim-core.md 5.2, 5.3).
//!
//! Split out of `physics.rs` (WP5a) so WP5b and WP5c add their fields without editing WP5a's
//! file. WP0 declares; WP5c owns this file. WP5b's only field here is the `yflag` flip model,
//! which WP0 already lists in [`FlipKind`]; anything else WP5b needs goes through WP5c.

use serde::Deserialize;

use crate::num::Real;

/// `[ice]`: slip blocks and ice (kid/A1.md Step action 3, kid/A3.md showdown Step action 3).
#[derive(Clone, PartialEq, Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct IceRules {
    /// The model.
    pub model: IceModel,
    /// Pixels in the gravity direction probed for a `slip` block (4).
    pub probe: Real,
    /// Pixels in the gravity direction at which the slip value is read (10).
    pub read: Real,
    /// Slip is clamped to the run speed (C); otherwise it is not (A1, A2, B).
    pub clamp: bool,
    /// `ice_temp_hspeed` (showdown): pixels probed for a `slip` block (2).
    pub temp_probe: Real,
    /// `ice_temp_hspeed`: `tempHspeed` is multiplied by this each tick off a push (0.95).
    pub temp_decay: Real,
    /// `ice_temp_hspeed`: `tempHspeed` becomes 0 once its size is below this (0.2).
    pub temp_zero_below: Real,
}

impl Default for IceRules {
    /// No ice, with the rules' usual numbers so a profile can switch it on with `model` alone.
    fn default() -> IceRules {
        IceRules {
            model: IceModel::None,
            probe: Real::new(4.0),
            read: Real::new(10.0),
            clamp: false,
            temp_probe: Real::new(2.0),
            temp_decay: Real::new(0.95),
            temp_zero_below: Real::new(0.2),
        }
    }
}

/// How the kid's walk reads `slip` blocks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IceModel {
    /// The game has no ice, or the walk ignores it.
    #[default]
    None,
    /// While a slip block is within `probe` px, walking adds `slip * h` to `hspeed` (read at
    /// `read` px) and standing subtracts `slip * sign(hspeed)`.
    Slip,
}

/// `[slope]`: slope models (WP5c).
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SlopeModel {}

/// `[ladder]`: ladder entry (kid/A1.md Step action 2).
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LadderRules {
    /// Which keys grab a ladder.
    pub entry: LadderEntry,
}

/// The keys that start climbing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LadderEntry {
    /// Up or Down held (little-trap, rainbow).
    #[default]
    UpOrDown,
    /// Up held only (rori, fairy-tale).
    UpOnly,
}

/// `[flip]`: how gravity flips (plan 5.3 flip rows). WP5c adds the other fields
/// (`mask_flipped`, ...).
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FlipModel {
    /// `model = "..."`.
    pub model: FlipKind,
}

/// The flip models of plan 5.3, by their `physics.toml` names.
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
