//! Room entry and the room change sequence (docs/plans/sim-core.md 4.2).
//!
//! WP0 declares; WP4a owns.
#![allow(unused_variables, reason = "WP0 stubs: WP4a fills the bodies")]

use serde::{Deserialize, Serialize};

use crate::def::RoomId;
use crate::error::SimError;
use crate::save::SaveSlot;
use crate::world::Ctx;

/// A deferred room change (tick row 15).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RoomRequest {
    /// `room_goto`.
    Goto(RoomId),
    /// `room_goto_next`.
    Next,
    /// `room_goto_previous`.
    Previous,
    /// `room_restart`.
    Restart,
    /// `game_restart`.
    GameRestart,
    /// `game_end`.
    GameEnd,
}

/// Where a run starts.
#[derive(Clone, PartialEq, Debug)]
pub enum StartPoint {
    /// What warpStart does: delete the save, set the difficulty, go to `GameDef::new_game_room`.
    NewGame {
        /// Difficulty index.
        difficulty: u8,
    },
    /// Trace cases: the recorder's `--room` jump.
    Room {
        /// The room.
        room: RoomId,
        /// Difficulty index.
        difficulty: u8,
    },
    /// From a save slot (practice, route testing).
    FromSave(SaveSlot),
}

/// Room changes and transitions for the client (`RoomOutput::Transition` etc.). WP4a adds the
/// variants.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RoomOutput {}

/// Creates the boot instances, enters the first room, runs Game Start.
pub fn start_game(ctx: &mut Ctx, start: &StartPoint) -> Result<(), SimError> {
    unimplemented!("WP4a: room::start_game")
}

/// The manual's 7-step room change (Confirmed), plus nested requests (R23).
pub fn perform_change(ctx: &mut Ctx, req: RoomRequest) -> Result<(), SimError> {
    unimplemented!("WP4a: room::perform_change")
}
