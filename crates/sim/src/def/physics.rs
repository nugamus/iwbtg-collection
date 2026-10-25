//! The kid's physics profile, `physics.toml` (docs/plans/sim-core.md 5.1, 5.2).
//!
//! WP0 declares the skeleton: every section is an empty struct. WP5a owns the file and adds the
//! fields of 5.2 and 5.3 (WP5b asks WP5a for its fields in WP5a's sections). The `[ice]`,
//! `[slope]`, `[ladder]` and `[flip]` sections live in `physics_ext.rs` (WP5c). Every struct
//! denies unknown fields, so a typo in a profile is a load error, not a silently ignored key.

use serde::Deserialize;

pub use super::physics_ext::{FlipKind, FlipModel, IceRules, LadderRules, SlopeModel};

/// The kid engine family a profile starts from (`preset = "A1"`): documentation and defaults
/// (`kid::presets::apply`). Nothing in the sim branches on it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
pub enum KidFamily {
    /// kid/A1.md.
    #[default]
    A1,
    /// kid/A2.md.
    A2,
    /// kid/A3.md.
    A3,
    /// kid/A4.md.
    A4,
    /// kid/A5.md.
    A5,
    /// kid/B.md.
    B,
    /// kid/C.md.
    C,
}

/// A full physics profile, after `kid::presets::apply` filled the defaults.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PhysicsProfile {
    /// Family the defaults came from.
    pub preset: KidFamily,
    /// `[numbers]`.
    pub numbers: KidNumbers,
    /// `[input]`.
    pub input: KidInput,
    /// `[events]`: ordered action lists per event.
    pub events: KidEvents,
    /// `[jump]`.
    pub jump: JumpRules,
    /// `[fall_cap]`.
    pub fall_cap: FallCap,
    /// `[gravity]`.
    pub gravity: GravityRules,
    /// `[platform]`.
    pub platform: PlatformRules,
    /// `[vine]`.
    pub vine: VineRules,
    /// `[water]`.
    pub water: WaterRules,
    /// `[ice]`.
    pub ice: IceRules,
    /// `[slope]`.
    pub slope: SlopeModel,
    /// `[ladder]`.
    pub ladder: LadderRules,
    /// `[flip]`.
    pub flip: FlipModel,
    /// `[death]`.
    pub death: DeathRules,
    /// `[bullet]`.
    pub bullet: BulletRules,
    /// `[save]`.
    pub save: SaveRules,
    /// `[misc]`.
    pub misc: KidMisc,
}

/// A profile as written, before the preset fills its defaults. `toml::from_str` target of
/// `crates/format` (WP7b); `kid::presets::apply` turns it into a [`PhysicsProfile`].
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PhysicsProfileDraft {
    /// `preset = "A1"`.
    pub preset: Option<KidFamily>,
}

macro_rules! profile_section {
    ($($(#[$doc:meta])* $name:ident;)*) => {
        $(
            $(#[$doc])*
            #[derive(Clone, PartialEq, Debug, Default, Deserialize)]
            #[serde(deny_unknown_fields, default)]
            pub struct $name {}
        )*
    };
}

profile_section! {
    /// `[numbers]`: jumps, gravity, run speed, caps, release multiplier (WP5a).
    KidNumbers;
    /// `[input]`: walk read mode, tap fix, vine keys (WP5a).
    KidInput;
    /// `[events]`: action lists per event, collisions, boundary (WP5a).
    KidEvents;
    /// `[jump]` (WP5a).
    JumpRules;
    /// `[fall_cap]` (WP5a).
    FallCap;
    /// `[gravity]` (WP5a).
    GravityRules;
    /// `[platform]` (WP5a).
    PlatformRules;
    /// `[vine]` (WP5a).
    VineRules;
    /// `[water]` (WP5a).
    WaterRules;
    /// `[death]` (WP5a).
    DeathRules;
    /// `[bullet]` (WP5a).
    BulletRules;
    /// `[save]` (WP5a).
    SaveRules;
    /// `[misc]` (WP5a).
    KidMisc;
}
