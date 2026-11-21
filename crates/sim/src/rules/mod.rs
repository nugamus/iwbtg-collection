//! Runner rules: every UNKNOWN, Inferred or Conflict rule the sim needs, as a switch with its
//! engine_rev 0 default (registers in docs/plans/sim-core/).
//!
//! Each field names its register id. A default changes only after the spec records a trace
//! result; once a package is verified, a changed default becomes a new `engine_rev` branch in
//! [`RunnerRules::for_runner`], so replays verified under the old rule still verify
//! (architecture "Versions"). Rules are values, never `const`s.
//!
//! File owners after WP0: `tick` WP1, `num`, `rng` and `motion` WP2, `collide` WP3, `room` WP4a,
//! `sound` WP10, this file WP11.

pub mod collide;
pub mod draw;
pub mod motion;
pub mod num;
pub mod rng;
pub mod room;
pub mod sound;
pub mod tick;

use serde::{Deserialize, Serialize};

pub use collide::CollideRules;
pub use draw::DrawRules;
pub use motion::MotionRules;
pub use num::NumRules;
pub use rng::RngRules;
pub use room::RoomRules;
pub use sound::SoundRules;
pub use tick::TickRules;

use crate::error::SimError;

/// The original GameMaker runner (`runner` in `game.toml`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runner {
    /// GameMaker 8.0.
    Gm80,
    /// GameMaker 8.1.
    Gm81,
}

/// Highest `engine_rev` this build knows.
pub const MAX_ENGINE_REV: u16 = 0;

/// All rule switches for one run.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct RunnerRules {
    /// Tick pipeline and events (R1-R10, R21, R22), plus the run's clock and timing units (R30).
    pub tick: TickRules,
    /// Automatic motion (M1, M2).
    pub motion: MotionRules,
    /// Collision (C2, C7b, C9, C10, C13, R12).
    pub collide: CollideRules,
    /// Rooms and restarts (R16, R23).
    pub room: RoomRules,
    /// Numbers (N1-N6, N10-N13, R3b).
    pub num: NumRules,
    /// Random-number streams (N10).
    pub rng: RngRules,
    /// Sound model (S1-S4).
    pub sound: SoundRules,
    /// Draw slot (R15): whether Draw may mutate.
    pub draw: DrawRules,
}

impl RunnerRules {
    /// The rules for a runner at an engine revision. GM 8.1 keeps the 8.0 rules: research#15
    /// found no simulation-visible difference (the 8.1 games differ only in renderer/content;
    /// U12, R11). `runner` records the build for provenance; only the real 8.1 event-order and
    /// input check remains OPEN (research#175). Any difference found later becomes a field, never
    /// a branch on the runner inside built-ins.
    pub fn for_runner(runner: Runner, engine_rev: u16) -> Result<RunnerRules, SimError> {
        if engine_rev > MAX_ENGINE_REV {
            return Err(SimError::Unsupported {
                what: format!("engine_rev {engine_rev} (this build knows up to {MAX_ENGINE_REV})"),
            });
        }
        match runner {
            Runner::Gm80 | Runner::Gm81 => Ok(RunnerRules::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::Real;

    #[test]
    fn rev_0_defaults_match_the_register() {
        let r = RunnerRules::for_runner(Runner::Gm80, 0).unwrap();
        assert_eq!(r, RunnerRules::for_runner(Runner::Gm81, 0).unwrap());
        assert_eq!(r.tick.timing.clock, tick::ClockPolicy::RoomSpeed);
        assert!(r.tick.timing.all_ticks());
        assert_eq!(r.tick.dispatch, tick::DispatchOrder::RankThenCreation);
        assert_eq!(r.tick.anim_slot, tick::AnimSlot::AfterDraw);
        assert_eq!(r.tick.new_instances_join_phase, tick::JoinPhase::LaterInOrder);
        assert_eq!(r.num.compare, num::CompareMode::Epsilon(Real::new(1e-13)));
        assert_eq!(r.num.truth, num::Truth::GeHalf);
        assert!(r.collide.recheck_nonsolid);
        assert_eq!(r.room.max_chained_changes, 8);
        assert_eq!(r.sound.isplaying_latency, 0);
        assert!(r.num.particles_draw_rng);
        assert_eq!(r.rng.layout, rng::StreamLayout::Shared);
        assert_eq!(r.draw.purity, draw::DrawPurity::Ambient);
    }

    #[test]
    fn unknown_engine_rev_is_unsupported() {
        assert!(matches!(
            RunnerRules::for_runner(Runner::Gm80, MAX_ENGINE_REV + 1),
            Err(SimError::Unsupported { .. })
        ));
    }
}
