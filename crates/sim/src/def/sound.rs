//! Sounds as the sim models them (docs/plans/sim-core.md 7). WP0 declares; WP10 owns.
//!
//! Audio data never reaches the sim; it only needs to know how long a sound plays.

use crate::num::Real;

/// One sound.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SoundDef {
    /// Package sound id.
    pub name: String,
    /// GM sound kind.
    pub kind: SoundKind,
    /// Length in ticks (song length times tick rate, from the package, CR9).
    pub length_ticks: u32,
    /// Default volume.
    pub volume: Real,
    /// Default pan.
    pub pan: Real,
}

/// GM sound kinds that matter to `sound_isplaying`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SoundKind {
    /// Normal sound: copies may overlap.
    #[default]
    Normal,
    /// MULTIMEDIA (music): at most one copy (S2).
    Multimedia,
}
