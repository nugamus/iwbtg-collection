//! The `physics.toml` sections that only families A2-A5, B and C use: `[ice]`, `[slope]`,
//! `[ladder]`, `[flip]` (docs/plans/sim-core.md 5.2, 5.3).
//!
//! Split out of `physics.rs` (WP5a) so WP5b and WP5c add their fields without editing WP5a's
//! file. WP0 declares; WP5c owns this file. WP5b's only field here is the `yflag` flip model,
//! which WP0 already lists in [`FlipKind`]; anything else WP5b needs goes through WP5c.

use serde::Deserialize;

use crate::num::Real;

/// `[ice]`: slip blocks and ice (kid/A1.md Step action 3, kid/A3.md showdown Step action 3).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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

/// `[slope]`: the slopes of group A and eat-the-new-year (kid/A1.md poronkusema, rainbow, rori,
/// fairy-tale; kid/A4.md). `slope` rises to the right, `slope_mirror` to the left. A game's slope
/// kinds carry only those classes (not `block`), so the kid's `slope` and `slope_mirror`
/// collision rows run alone, as the original's child-object events do.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SlopeModel {
    /// How the horizontal block contact climbs (`block_slope_step`).
    pub block_step: BlockSlopeStep,
    /// The climb: 3 px sideways and 3 px up.
    pub step: Real,
    /// The End Step downhill snap (`downhill_snap`).
    pub downhill: Downhill,
    /// `slope_jump`: with a slope or mirrored slope within this many pixels below a Jump press
    /// forces a single jump (rainbow, rori, fairy-tale: 2). 0 means off.
    pub jump_probe: Real,
    /// `air_sprite`: no fall sprite while a slope lies this many pixels below (rori, fairy-tale: 2).
    /// 0 means off.
    pub air_sprite_probe: Real,
    /// `walk` keeps `hspeed` while a slope is 1 px below and the key points downhill
    /// (eat-the-new-year, kid/A4.md).
    pub keep_hspeed: bool,
}

impl Default for SlopeModel {
    fn default() -> SlopeModel {
        SlopeModel {
            block_step: BlockSlopeStep::None,
            step: Real::new(3.0),
            downhill: Downhill::None,
            jump_probe: Real::ZERO,
            air_sprite_probe: Real::ZERO,
            keep_hspeed: false,
        }
    }
}

/// How `block_slope_step` climbs.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockSlopeStep {
    /// No slope rule in the block contact.
    #[default]
    None,
    /// Rainbow: after the contact, the matching slope `step` px ahead and a free spot `step` up
    /// move the kid there.
    FreeSpot,
    /// Rori, fairy-tale: instead of the contact, the shifted kid overlapping the matching slope
    /// moves it `step` sideways and up, with no free-spot test.
    Replace,
}

/// The End Step downhill snap models.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Downhill {
    /// No snap.
    #[default]
    None,
    /// Poronkusema: `vspeed >= 0`, free below, on a slope last step and a slope within 12 px: down
    /// to contact by at most 12, running or idle sprite; then remember a slope 1 px below.
    Por,
    /// Rainbow: a slope within 5 px below, then a mirrored one: down to contact by at most 10.
    Rainbow,
    /// Fairy-tale: no block 1 px below and a slope 3 px below at `xprevious`: down by at most 10.
    FairyTale,
}

/// `[ladder]`: ladder entry (kid/A1.md Step action 2).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LadderRules {
    /// Which keys grab a ladder.
    pub entry: LadderEntry,
}

/// The keys that start climbing.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
