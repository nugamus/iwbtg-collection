//! Sounds as the sim models them (docs/plans/sim-core.md 7). WP0 declares; WP10 owns.
//!
//! Audio data never reaches the sim; it only needs to know how long a sound plays.

use super::game::GameDef;
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

impl GameDef {
    /// The length in ticks of a file a game loads through a DLL (`SS_LoadSound(file, ...)`,
    /// `FMODSoundAdd(file, ...)`), for the script host to pass as `length_ticks` (#288). The DLLs
    /// never report a length, so the package supplies it: the file name's stem (after the last
    /// `/` or `\`, without its extension, compared without regard to case, as Windows file names
    /// are) names a package sound or music id, and its sidecar `length_ticks` is the answer
    /// (docs/format/v0.md, Sounds). A file that matches no sound gives 0, which the adapters
    /// read as "unknown": a one-shot then plays until stopped.
    pub fn dll_sound_length(&self, file: &str) -> u32 {
        let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        self.sounds
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(stem))
            .map_or(0, |s| s.length_ticks)
    }
}
