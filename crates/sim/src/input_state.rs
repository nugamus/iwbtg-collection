//! This tick's input and the `keyboard_check_direct` latch (docs/plans/sim-core.md 2.1 row 0,
//! 2.2 "Direct key reads", I1).
//!
//! Every built-in and kid action that ports a `keyboard_check_direct` calls
//! [`InputState::direct`]; nothing else reads or clears the latch (the latch bits are private).
//! WP0 declared; WP1 owns.
//!
//! The latch is CLONE_SPEC input Inferred: one "pressed since the last direct read" bit per key,
//! shared by every direct read and consumed by each. The recorder does not model it
//! (traces/rely-the-needle-4-2/t4-input-direct-tap-a.csv), so `TickRules::direct_latch` can turn
//! it off for trace comparison.

use serde::{Deserialize, Serialize};

use crate::input::{Action, InputFrame};

/// The input frame of the tick plus one latch bit per action.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct InputState {
    /// This tick's frame: held, pressed and released stay fixed through the tick (CLONE_SPEC
    /// input Inferred, "updates it once at the start of each step").
    pub frame: InputFrame,
    /// Bit per action: pressed since the last direct read of that action.
    direct_latch: u16,
}

impl InputState {
    /// Tick row 0: take the frame for this tick and set `direct_latch |= pressed`.
    pub fn latch(&mut self, frame: InputFrame) {
        self.latch_with(frame, true);
    }

    /// Tick row 0 under `TickRules::direct_latch`: take the frame; when `latch_presses` is false
    /// (the recorder's model, no latch) no press is latched, so [`InputState::direct`] reads the
    /// held state only.
    pub fn latch_with(&mut self, frame: InputFrame, latch_presses: bool) {
        self.frame = frame;
        if latch_presses {
            self.direct_latch |= frame.pressed & Action::MASK;
        }
    }

    /// GML `keyboard_check` on the action.
    pub fn held(&self, a: Action) -> bool {
        self.frame.is_held(a)
    }

    /// GML `keyboard_check_pressed` on the action.
    pub fn pressed(&self, a: Action) -> bool {
        self.frame.was_pressed(a)
    }

    /// GML `keyboard_check_released` on the action.
    pub fn released(&self, a: Action) -> bool {
        self.frame.was_released(a)
    }

    /// GML `keyboard_check_direct`: held, or latched since the last direct read of this action;
    /// consumes the latch. Two calls in one Step can differ (ladder code reads Up twice).
    pub fn direct(&mut self, a: Action) -> bool {
        let latched = self.direct_latch & a.bit() != 0;
        self.direct_latch &= !a.bit();
        self.held(a) || latched
    }

    /// GML `io_clear`: drop held state, edges and the latch (xia unpause, I5).
    pub fn clear(&mut self) {
        self.frame = InputFrame::EMPTY;
        self.direct_latch = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(held: u16, pressed: u16, released: u16) -> InputFrame {
        InputFrame {
            held,
            pressed,
            released,
        }
    }

    #[test]
    fn direct_reads_held_or_latch_and_consumes_it() {
        let r = Action::Right.bit();
        let mut s = InputState::default();
        // Tap inside one tick: pressed and released, not held at the sample.
        s.latch(frame(0, r, r));
        assert!(!s.held(Action::Right));
        assert!(s.pressed(Action::Right) && s.released(Action::Right));
        assert!(s.direct(Action::Right), "the latch reports the tap");
        assert!(!s.direct(Action::Right), "the first read consumed it");
        // The latch survives ticks nobody reads it in (restart glitch).
        s.latch(frame(0, r, r));
        s.latch(InputFrame::EMPTY);
        s.latch(InputFrame::EMPTY);
        assert!(s.direct(Action::Right));
        // Held keys read true on every call.
        s.latch(frame(r, r, 0));
        assert!(s.direct(Action::Right) && s.direct(Action::Right));
        // Other actions are untouched by a read.
        let l = Action::Left.bit();
        s.latch(frame(0, l, l));
        assert!(!s.direct(Action::Right));
        assert!(s.direct(Action::Left));
    }

    #[test]
    fn no_latch_mode_reads_held_only() {
        let r = Action::Right.bit();
        let mut s = InputState::default();
        s.latch_with(frame(0, r, r), false);
        assert!(!s.direct(Action::Right));
        s.latch_with(frame(r, r, 0), false);
        assert!(s.direct(Action::Right));
    }

    #[test]
    fn clear_drops_everything() {
        let r = Action::Right.bit();
        let mut s = InputState::default();
        s.latch(frame(r, r, 0));
        s.clear();
        assert!(!s.held(Action::Right) && !s.pressed(Action::Right));
        assert!(!s.direct(Action::Right));
    }
}
