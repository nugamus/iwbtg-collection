//! Room change switches (docs/plans/sim-core.md 4, register R16, R23).
//!
//! WP0 declares; WP4a owns.

/// Rules for the room change sequence and game_restart.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RoomRules {
    /// R23 (UNKNOWN): a room request raised during the room change sequence.
    pub nested_request: NestedRequest,
    /// R23: chained room changes allowed in one tick before `SimError` (Design: 8).
    pub max_chained_changes: u8,
    /// R16 (UNKNOWN): globals survive game_restart. Default false (cleared).
    pub globals_survive_restart: bool,
    /// R16 (UNKNOWN): instances removed by game_restart run events. Default false.
    pub restart_events: bool,
}

impl Default for RoomRules {
    fn default() -> RoomRules {
        RoomRules {
            nested_request: NestedRequest::default(),
            max_chained_changes: 8,
            globals_survive_restart: false,
            restart_events: false,
        }
    }
}

/// R23: what happens to a room request raised while a room change runs (room creation code).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NestedRequest {
    /// Finish all 7 steps, then change again before the next tick (Design). Default.
    #[default]
    ChainAfterSequence,
    /// The new room gets one tick first.
    NextTick,
}
