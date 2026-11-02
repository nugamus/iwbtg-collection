//! Room change switches (docs/plans/sim-core.md 4, register R16, R23).
//!
//! WP0 declares; WP4a owns.

/// Rules for the room change sequence and game_restart.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RoomRules {
    /// R23 (UNKNOWN): a room request raised during the room change sequence.
    pub nested_request: NestedRequest,
    /// R23: chained room changes allowed in one tick before `SimError` (Design: 8). The change
    /// that was requested first does not count; the 9th change raised during the sequences
    /// halts the sim.
    pub max_chained_changes: u8,
    /// R16 (UNKNOWN): globals survive game_restart. Default false (cleared). Confirmed
    /// (recorder) that they are cleared: little-runner tr-room-04 rows 33-34.
    pub globals_survive_restart: bool,
    /// R16 (UNKNOWN): instances removed by game_restart run events. Default false. When true,
    /// they get Room End (in [`RoomRules::room_end_order`]) before they are removed (Design: the
    /// spec does not say which events, if any).
    pub restart_events: bool,
    /// Room change step 7: the order of Room Start on room entry. Confirmed (recorder):
    /// [`RoomStartOrder::RoomThenCarried`].
    pub room_start_order: RoomStartOrder,
    /// Room change step 1: the order of Room End. Still open in the spec (rooms Confirmed
    /// (recorder), "Room start sequence" item 1).
    pub room_end_order: RoomEndOrder,
    /// Room change step 6 on a revisit of a persistent room (rooms UNKNOWN: whether room
    /// creation code runs again). Default true: the manual's step list conditions only step 4
    /// on the room being new (Design reading). No persistent room of the 20 games has creation
    /// code, so no game depends on it.
    pub creation_code_on_revisit: bool,
}

impl Default for RoomRules {
    fn default() -> RoomRules {
        RoomRules {
            nested_request: NestedRequest::default(),
            max_chained_changes: 8,
            globals_survive_restart: false,
            restart_events: false,
            room_start_order: RoomStartOrder::default(),
            room_end_order: RoomEndOrder::default(),
            creation_code_on_revisit: true,
        }
    }
}

/// R23: what happens to a room request raised while a room change runs (room creation code).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NestedRequest {
    /// Finish all 7 steps, then change again before the next tick (Design). Default. Consistent
    /// with the recorder: kaenbyou spends 0 ticks in rLoad (traces/kaenbyou/tr-room-12.csv rows
    /// 136-137); whether rLoad's Room Start ran is still open.
    #[default]
    ChainAfterSequence,
    /// The new room gets one tick first.
    NextTick,
}

/// The order of Room Start when a room is entered (room change step 7).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RoomStartOrder {
    /// The new room's own instances (placed, restored, or created during the sequence) in
    /// creation order, then the persistent instances carried over from the previous room in
    /// creation order. Confirmed (recorder): CLONE_SPEC tick loop "Exception, Room Start on
    /// room entry" (traces/rely-the-needle-4-2/t-room-1.csv row 21, traces/showdown/tr-room-14,
    /// traces/clear-the-easy-miku-2/audio-miku-sync-mute-d.csv row 3). Default.
    #[default]
    RoomThenCarried,
    /// The ordinary dispatch order of every phase (`events::dispatch_order`, R2): the plan's
    /// first reading, before the recorder result.
    Dispatch,
}

/// The order of Room End when a room is left (room change step 1). Open in the spec.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RoomEndOrder {
    /// The ordinary dispatch order (`events::dispatch_order`, R2). Default (plan 4.2).
    #[default]
    Dispatch,
    /// Draw order: depth descending, then creation order. Built after the spec's Inferred
    /// reading of showdown's Room End log (traces/showdown/tr-room-14: the instance list looks
    /// depth-sorted), but it does **not** reproduce that log: the log puts the carried world
    /// (depth 0) after the 5 depth-0 warpStarts, while here the boot world has the lowest id and
    /// so comes first among equal depths. The tie-break among equal depths is unknown.
    Depth,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_register() {
        let r = RoomRules::default();
        assert_eq!(r.nested_request, NestedRequest::ChainAfterSequence);
        assert_eq!(r.max_chained_changes, 8);
        assert!(!r.globals_survive_restart);
        assert!(!r.restart_events);
        assert_eq!(r.room_start_order, RoomStartOrder::RoomThenCarried);
        assert_eq!(r.room_end_order, RoomEndOrder::Dispatch);
        assert!(r.creation_code_on_revisit);
    }
}
