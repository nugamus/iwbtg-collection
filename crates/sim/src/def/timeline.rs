//! Timelines (docs/plans/sim-core.md 2.1 row 3, R6, R6b). WP0 declares; WP2 owns.

/// One timeline. Its moments run as `EventKind::Moment(step)` on the instance that runs it.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TimelineDef {
    /// Package timeline id.
    pub name: String,
    /// Moment steps, ascending, no duplicates.
    pub moments: Vec<u32>,
}
