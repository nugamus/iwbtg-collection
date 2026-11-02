//! Timelines (docs/plans/sim-core.md 2.1 row 3, R6, R6b). WP0 declares; WP2 owns.

/// One timeline. Its moments run as `EventKind::Moment(step)` on the instance that runs it.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TimelineDef {
    /// Package timeline id.
    pub name: String,
    /// Moment steps, ascending, no duplicates.
    pub moments: Vec<u32>,
    /// The function each moment calls (`moment_<call>`), parallel to `moments`; empty for a def
    /// built without names (tests).
    pub calls: Vec<String>,
    /// The timeline's `loop` flag: a run started through [`start_with_def`] begins with
    /// `timeline_loop` set.
    ///
    /// [`start_with_def`]: crate::timeline::start_with_def
    pub looping: bool,
}
