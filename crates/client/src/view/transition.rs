//! Explicit room-transition state (E4a, #1192): what the sim asked to play, when it started on
//! the wall clock, how long it lasts, and the input policy each phase declares.
//!
//! Representation only: no drawing or input system reads this yet, so the room change stays
//! instant and the picture is unchanged. E4b (#1247) draws the fade and enforces the policy,
//! gated by the ruleset/profile value (#1160) so a `gm8` package keeps today's behavior.
//!
//! The sim emits [`RoomOutput::Transition`] for the next room change and resets it to 0 after one
//! change (Confirmed); it also emits [`RoomOutput::Entered`] during the change. This state
//! consumes those events and never writes the sim, so it cannot move a tick or a hash.
//!
//! Kind 21 is "fade out and in". The only measured transition (30 steps) took 278 ms of real
//! time, so the standard model declares [`STEP_MS`] per step instead of deriving the length from
//! the tick rate, the fixed clock or slow motion. There is no hold phase in kind 21; a kind that
//! needs one extends [`Phase`].

#![allow(
    dead_code,
    reason = "the state is recorded now and read by the drawing/input slice E4b (#1247)"
)]

use bevy::prelude::Resource;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::room::RoomOutput;

use crate::time::{Duration, Instant};

/// `transition_kind` 21, "fade out and in": the only kind this model plays.
pub(crate) const FADE: u8 = 21;

/// The wall-clock length of one transition step. The measured 30-step transition took 278 ms
/// (CLONE_SPEC, "Room transitions": the recorder's clock jumps 278 ms), so the standard model
/// uses 10 ms per step. It is a declared design value, not a tick count.
pub(crate) const STEP_MS: u64 = 10;

/// What the sim's `RoomOutput::Transition` asked for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Spec {
    /// `transition_kind`.
    pub kind: u8,
    /// `transition_steps` (default 80 in GM8).
    pub steps: u32,
}

impl Spec {
    /// The transition for a kind and step count, or `None` for kind 0 (no transition) and kinds
    /// the client does not play yet.
    pub(crate) fn new(kind: u8, steps: u32) -> Option<Spec> {
        (kind == FADE && steps > 0).then_some(Spec { kind, steps })
    }

    /// The transition's wall-clock length. Independent of the tick rate, the fixed clock and
    /// slow motion.
    pub(crate) fn duration(self) -> Duration {
        Duration::from_millis(u64::from(self.steps) * STEP_MS)
    }
}

/// The phases of a fade out and in. Kind 21 has no hold; the out and in halves are equal, and
/// the exact split is a visible-slice decision (E4b) that does not affect this state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Phase {
    /// Fading the old room out.
    Out,
    /// Fading the new room in.
    In,
    /// The transition is over.
    Done,
}

impl Phase {
    /// The declared input policy for this phase.
    pub(crate) fn input(self) -> InputPolicy {
        match self {
            Phase::Out | Phase::In => InputPolicy::ClientOnly,
            Phase::Done => InputPolicy::All,
        }
    }
}

/// Which input may reach the sim during a phase.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum InputPolicy {
    /// Sim input and client keys both work.
    All,
    /// Client keys (pause, retry, quit) work; the sim's `InputFrame` is empty, so a press cannot
    /// leak into the first ticks of the new room.
    ClientOnly,
}

/// The transition playing now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Playing {
    /// What is playing.
    pub spec: Spec,
    /// When the room change started it.
    pub started: Instant,
    /// Its wall-clock length.
    pub duration: Duration,
}

impl Playing {
    fn elapsed(self, now: Instant) -> Duration {
        now.checked_duration_since(self.started)
            .unwrap_or(Duration::ZERO)
    }

    /// The phase at `now`.
    pub(crate) fn phase(self, now: Instant) -> Phase {
        let elapsed = self.elapsed(now);
        if elapsed >= self.duration {
            Phase::Done
        } else if elapsed * 2 < self.duration {
            Phase::Out
        } else {
            Phase::In
        }
    }

    /// How far through the transition `now` is, 0 to 1.
    pub(crate) fn progress(self, now: Instant) -> f64 {
        let total = self.duration.as_secs_f64();
        if total <= 0.0 {
            return 1.0;
        }
        (self.elapsed(now).as_secs_f64() / total).clamp(0.0, 1.0)
    }
}

/// The client's transition state: the pending request and the transition a room change started.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub(crate) struct Transition {
    pending: Option<Spec>,
    playing: Option<Playing>,
}

impl Transition {
    /// A built-in set the transition for the next room change.
    pub(crate) fn request(&mut self, kind: u8, steps: u32) {
        self.pending = Spec::new(kind, steps);
    }

    /// A room was entered: play the pending transition, if any. A change without a pending
    /// request plays none, so a transition never carries over to a later room.
    pub(crate) fn entered(&mut self, now: Instant) {
        self.playing = self.pending.take().map(|spec| Playing {
            spec,
            started: now,
            duration: spec.duration(),
        });
    }

    /// `game_restart` ran: nothing pending or playing survives it.
    pub(crate) fn restarted(&mut self) {
        self.pending = None;
        self.playing = None;
    }

    /// Consumes one tick's output events.
    pub(crate) fn observe(&mut self, output: &[OutputEvent], now: Instant) {
        for event in output {
            match event {
                OutputEvent::Room(RoomOutput::Transition { kind, steps }) => {
                    self.request(*kind, *steps);
                }
                OutputEvent::Room(RoomOutput::Entered { .. }) => self.entered(now),
                OutputEvent::Room(RoomOutput::GameRestarted) => self.restarted(),
                _ => {}
            }
        }
    }

    /// The transition playing at `now`, if any.
    pub(crate) fn playing(&self) -> Option<Playing> {
        self.playing
    }

    /// The pending transition for the next room change, if any.
    pub(crate) fn pending(&self) -> Option<Spec> {
        self.pending
    }

    /// The phase at `now`; `Done` when nothing is playing.
    pub(crate) fn phase(&self, now: Instant) -> Phase {
        self.playing.map_or(Phase::Done, |p| p.phase(now))
    }

    /// The input policy at `now`; `All` when nothing is playing.
    pub(crate) fn input(&self, now: Instant) -> InputPolicy {
        self.phase(now).input()
    }
}

#[cfg(test)]
mod tests {
    use iwbtg_sim::def::RoomId;

    use super::*;

    fn base() -> Instant {
        Instant::now()
    }

    #[test]
    fn only_kind_21_with_steps_is_a_transition() {
        assert_eq!(Spec::new(0, 30), None);
        assert_eq!(Spec::new(FADE, 0), None);
        assert_eq!(
            Spec::new(FADE, 30),
            Some(Spec {
                kind: FADE,
                steps: 30
            })
        );
    }

    #[test]
    fn the_duration_is_wall_clock_not_ticks() {
        // The same 30 steps take 300 ms at any tick rate, clock speed or draw rate.
        let spec = Spec::new(FADE, 30).unwrap();
        assert_eq!(spec.duration(), Duration::from_millis(300));
        let other = Spec::new(FADE, 80).unwrap();
        assert_eq!(other.duration(), Duration::from_millis(800));
    }

    #[test]
    fn a_pending_transition_plays_at_the_next_room_entry() {
        let start = base();
        let mut t = Transition::default();
        t.request(FADE, 30);
        assert!(t.playing().is_none());
        t.entered(start);
        assert!(t.pending().is_none(), "the request is consumed");
        let playing = t.playing().unwrap();
        assert_eq!(playing.spec, Spec { kind: FADE, steps: 30 });
        assert_eq!(playing.duration, Duration::from_millis(300));
        // A later room change without a new request starts nothing.
        t.entered(start + Duration::from_secs(1));
        assert!(t.playing().is_none());
    }

    #[test]
    fn the_phases_split_the_wall_clock_and_end_at_the_duration() {
        let start = base();
        let mut t = Transition::default();
        t.request(FADE, 30); // 300 ms
        t.entered(start);
        assert_eq!(t.phase(start), Phase::Out);
        assert_eq!(t.phase(start + Duration::from_millis(149)), Phase::Out);
        assert_eq!(t.phase(start + Duration::from_millis(150)), Phase::In);
        assert_eq!(t.phase(start + Duration::from_millis(299)), Phase::In);
        assert_eq!(t.phase(start + Duration::from_millis(300)), Phase::Done);
        // A clock that goes backwards is treated as the start, not a panic.
        assert_eq!(t.phase(start - Duration::from_millis(1)), Phase::Out);
    }

    #[test]
    fn progress_runs_from_zero_to_one_and_clamps() {
        let start = base();
        let mut t = Transition::default();
        t.request(FADE, 30);
        t.entered(start);
        assert_eq!(t.playing().unwrap().progress(start), 0.0);
        let half = t
            .playing()
            .unwrap()
            .progress(start + Duration::from_millis(150));
        assert!((half - 0.5).abs() < 1e-9, "half is {half}");
        assert_eq!(
            t.playing().unwrap().progress(start + Duration::from_secs(9)),
            1.0
        );
    }

    #[test]
    fn the_input_policy_blocks_sim_input_only_while_the_fade_plays() {
        let start = base();
        let mut t = Transition::default();
        assert_eq!(t.input(start), InputPolicy::All, "no transition");
        t.request(FADE, 30);
        t.entered(start);
        assert_eq!(t.input(start), InputPolicy::ClientOnly);
        assert_eq!(
            t.input(start + Duration::from_millis(299)),
            InputPolicy::ClientOnly
        );
        assert_eq!(t.input(start + Duration::from_millis(300)), InputPolicy::All);
    }

    #[test]
    fn a_game_restart_drops_everything() {
        let start = base();
        let mut t = Transition::default();
        t.request(FADE, 30);
        t.entered(start);
        t.restarted();
        assert!(t.playing().is_none());
        assert!(t.pending().is_none());
        assert_eq!(t.phase(start), Phase::Done);
    }

    #[test]
    fn observing_the_sim_events_drives_the_state_in_order() {
        let start = base();
        let mut t = Transition::default();
        // The sim emits the transition in the request tick, then Entered during the change.
        t.observe(
            &[
                OutputEvent::Room(RoomOutput::Transition {
                    kind: FADE,
                    steps: 30,
                }),
                OutputEvent::Room(RoomOutput::Entered {
                    from: None,
                    to: RoomId(0),
                }),
            ],
            start,
        );
        assert!(t.playing().is_some());
        assert!(t.pending().is_none());
        // A restart clears it.
        t.observe(&[OutputEvent::Room(RoomOutput::GameRestarted)], start);
        assert!(t.playing().is_none());
    }
}
