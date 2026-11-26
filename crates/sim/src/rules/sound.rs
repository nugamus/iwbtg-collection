//! Sound model switches (docs/architecture/sim/sound.md, register S1-S5).
//!
//! S5 lives in the DLL adapters.

/// The named sound model a run plays under (a ruleset/profile value, never package data;
/// docs/decisions/0005-standard-mechanics.md "One sound model").
///
/// The format-0 default, [`SoundModel::Gm8`], reproduces today's behavior: both gm8 DLL
/// adapters are modeled. A format-1 profile names `event`, `supersound`, `gmfmod` or `stub`, and
/// a recorder `--stub-dlls` run (`DllMode::Stub`) overrides any of them with [`SoundModel::Stub`]
/// ([`crate::world::Ctx::sound_model`]). The built-in copies and the client event stream run in
/// every model: only the DLL adapters are selected here.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SoundModel {
    /// No profile named yet (format 0): both gm8 adapters are modeled, as before profiles.
    #[default]
    Gm8,
    /// `event`: the standard model. No DLL adapters; the built-in copies and sound events only.
    Event,
    /// `supersound`: the gm8 SuperSound adapter (kaenbyou, eat-the-lemon).
    SuperSound,
    /// `gmfmod`: the gm8 GMFMODSimple adapter (go-the-spook-house, keep-celebrating-for-xia).
    Gmfmod,
    /// `stub`: the recorder's `--stub-dlls`; every DLL call takes its stub value.
    Stub,
}

impl SoundModel {
    /// The profile name (`gm8`, `event`, `supersound`, `gmfmod`, `stub`).
    pub fn name(self) -> &'static str {
        match self {
            SoundModel::Gm8 => "gm8",
            SoundModel::Event => "event",
            SoundModel::SuperSound => "supersound",
            SoundModel::Gmfmod => "gmfmod",
            SoundModel::Stub => "stub",
        }
    }

    /// Whether the SuperSound adapter answers calls (it is modeled and not stubbed).
    pub fn models_supersound(self) -> bool {
        matches!(self, SoundModel::Gm8 | SoundModel::SuperSound)
    }

    /// Whether the GMFMODSimple adapter answers calls (it is modeled and not stubbed).
    pub fn models_gmfmod(self) -> bool {
        matches!(self, SoundModel::Gm8 | SoundModel::Gmfmod)
    }
}

/// Rules for the sound model.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SoundRules {
    /// The named model: `event` for `standard`, a gm8 adapter (or `gm8`, the format-0 default)
    /// for a port. A recorder `--stub-dlls` run overrides it with `stub`.
    pub model: SoundModel,
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
            model: SoundModel::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_named_models_match_the_profile_values() {
        assert_eq!(SoundModel::Event.name(), "event");
        assert_eq!(SoundModel::SuperSound.name(), "supersound");
        assert_eq!(SoundModel::Gmfmod.name(), "gmfmod");
        assert_eq!(SoundModel::Stub.name(), "stub");
        assert_eq!(SoundModel::Gm8.name(), "gm8");
    }

    #[test]
    fn the_format_0_default_models_both_gm8_adapters() {
        let s = SoundRules::default();
        assert_eq!(s.model, SoundModel::Gm8);
        assert!(s.model.models_supersound() && s.model.models_gmfmod());
        assert!(!SoundModel::Event.models_supersound() && !SoundModel::Event.models_gmfmod());
        assert!(!SoundModel::Stub.models_supersound() && !SoundModel::Stub.models_gmfmod());
        assert!(SoundModel::SuperSound.models_supersound());
        assert!(!SoundModel::SuperSound.models_gmfmod());
        assert!(SoundModel::Gmfmod.models_gmfmod());
        assert!(!SoundModel::Gmfmod.models_supersound());
    }
}
