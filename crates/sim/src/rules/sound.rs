//! Sound model switches (docs/plans/sim-core.md 7, register S1-S4).
//!
//! S5 lives in the DLL adapters. WP0 declares; WP10 owns.

/// Rules for the sound model.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SoundRules {
    /// S1 (UNKNOWN): ticks before `sound_isplaying` sees a started sound. Default 0.
    pub isplaying_latency: u32,
    /// S2 (UNKNOWN): a second MULTIMEDIA start while one plays.
    pub multimedia_second: MultimediaSecond,
    /// S3 (UNKNOWN): `sound_loop` on an already playing MULTIMEDIA sound.
    pub loop_playing: LoopPlaying,
    /// S4 (Inferred): sounds keep playing through game_restart. Default true.
    pub keep_on_restart: bool,
}

impl Default for SoundRules {
    fn default() -> SoundRules {
        SoundRules {
            isplaying_latency: 0,
            multimedia_second: MultimediaSecond::default(),
            loop_playing: LoopPlaying::default(),
            keep_on_restart: true,
        }
    }
}

/// S2: a second MULTIMEDIA start while one plays.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MultimediaSecond {
    /// Stop the first, start the second. Default.
    #[default]
    StopFirst,
    /// Ignore the second start.
    Ignore,
}

/// S3: `sound_loop` on a MULTIMEDIA sound that already plays.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LoopPlaying {
    /// Ignore the call. Default.
    #[default]
    Ignore,
    /// Restart the sound.
    Restart,
}
