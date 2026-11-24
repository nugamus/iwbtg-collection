//! Room change switches (docs/architecture/sim/rooms.md, register R16, R23).
//!
//! WP0 declares; WP4a owns.

/// Rules for the room change sequence and game_restart.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RoomRules {
    /// R23: chained room changes allowed in one tick before `SimError` (Design: 8). The change
    /// that was requested first does not count; the 9th change raised during the sequences
    /// halts the sim.
    pub max_chained_changes: u8,
    /// R16: globals survive game_restart. Default false (cleared). Confirmed
    /// (recorder) that they are cleared: little-runner tr-room-04 rows 33-34.
    pub globals_survive_restart: bool,
    /// R16 (UNKNOWN, not traced): instances removed by game_restart run events. Default false. When true,
    /// they get Room End before they are removed (Design: the spec does not say which events, if
    /// any).
    pub restart_events: bool,
    /// Room change step 6 on a revisit of a persistent room (rooms UNKNOWN: whether room
    /// creation code runs again). Default true: the manual's step list conditions only step 4
    /// on the room being new (Design reading). No persistent room of the 20 games has creation
    /// code, so no game depends on it.
    pub creation_code_on_revisit: bool,
}

impl Default for RoomRules {
    fn default() -> RoomRules {
        RoomRules {
            max_chained_changes: 8,
            globals_survive_restart: false,
            restart_events: false,
            creation_code_on_revisit: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_register() {
        let r = RoomRules::default();
        assert_eq!(r.max_chained_changes, 8);
        assert!(!r.globals_survive_restart);
        assert!(!r.restart_events);
        assert!(r.creation_code_on_revisit);
    }
}
