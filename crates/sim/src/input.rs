//! Input frames: one per tick, with edge bits. See docs/architecture.md, "Input and replay format".
//!
//! Minimal version for `replay`; the full module (with `Action`) replaces it at merge.

use serde::{Deserialize, Serialize};

/// The action bits for one tick. `pressed` and `released` keep sub-tick taps, which GM8 sees as
/// both events in the same step.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct InputFrame {
    /// Actions held down during this tick.
    pub held: u16,
    /// Actions pressed since the previous tick.
    pub pressed: u16,
    /// Actions released since the previous tick.
    pub released: u16,
}
