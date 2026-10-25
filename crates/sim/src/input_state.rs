//! This tick's input and the `keyboard_check_direct` latch (docs/plans/sim-core.md 2.1 row 0,
//! 2.2 "Direct key reads", I1).
//!
//! Every built-in and kid action that ports a `keyboard_check_direct` calls
//! [`InputState::direct`]; nothing else reads or clears the latch. WP0 declares; WP1 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP1 fills the bodies")]

use serde::{Deserialize, Serialize};

use crate::input::{Action, InputFrame};

/// The input frame of the tick plus one latch bit per action (Inferred input: shared by every
/// direct read, consumed by each read).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct InputState {
    /// This tick's frame.
    pub frame: InputFrame,
    /// Bit per action: pressed since the last direct read of that action.
    pub direct_latch: u16,
}

impl InputState {
    /// Tick row 0: take the frame for this tick and set `direct_latch |= pressed`.
    pub fn latch(&mut self, frame: InputFrame) {
        unimplemented!("WP1: InputState::latch")
    }

    /// GML `keyboard_check` on the action.
    pub fn held(&self, a: Action) -> bool {
        unimplemented!("WP1: InputState::held")
    }

    /// GML `keyboard_check_pressed` on the action.
    pub fn pressed(&self, a: Action) -> bool {
        unimplemented!("WP1: InputState::pressed")
    }

    /// GML `keyboard_check_released` on the action.
    pub fn released(&self, a: Action) -> bool {
        unimplemented!("WP1: InputState::released")
    }

    /// GML `keyboard_check_direct`: held or latched since the last direct read; consumes the
    /// latch.
    pub fn direct(&mut self, a: Action) -> bool {
        unimplemented!("WP1: InputState::direct")
    }

    /// GML `io_clear`: drop held state, edges and the latch (xia unpause, I5).
    pub fn clear(&mut self) {
        unimplemented!("WP1: InputState::clear")
    }
}
