//! The save slot (docs/plans/sim-core.md 4.4, N5-N8).
//!
//! The sim never touches files: the save is sim state, the client stores its bytes (postcard).
//! WP0 declares; WP4b owns.
#![allow(unused_variables, reason = "WP0 stubs: WP4b fills the bodies")]

use serde::{Deserialize, Serialize};

use crate::def::RoomId;
use crate::error::SimError;
use crate::num::Real;
use crate::rules::NumRules;
use crate::world::{Ctx, InstId};

/// What the game's save file holds.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SaveSlot {
    /// Saved room.
    pub room: RoomId,
    /// Saved position, after [`quantize`].
    pub x: Real,
    /// Saved position, after [`quantize`].
    pub y: Real,
    /// Kid was flipped.
    pub flipped: bool,
    /// Kid facing, -1 or 1.
    pub facing: i8,
    /// Difficulty index.
    pub difficulty: u8,
    /// Death counter (checked).
    pub deaths: u32,
    /// Play time counter (checked).
    pub time: u32,
    /// Boss flags.
    pub bosses: u16,
    /// Item flags.
    pub items: u16,
    /// Game cleared.
    pub clear: bool,
}

/// What the original's save format does to a position (rooms Confirmed group table).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SavedPosition {
    /// Base-100 bytes (13 games; last byte through `byte_from_real`, N5).
    XyBase100Bytes,
    /// Base-256 div/mod bytes (little-runner, N8).
    XyBase256Bytes,
    /// `string()` with two decimals (machinary, eat-the-new-year, N6).
    XyText2Dec,
    /// `xprevious`/`yprevious` exactly (kaenbyou, eat-the-lemon, N7).
    XyPrevious,
    /// A fixed point (poronkusema).
    FixedPoint,
    /// Floor with the three-step +1 nudge using `place_free` (xia).
    FloorNudge,
}

/// Saves now from `kid` with the game's position model.
pub fn save_now(
    ctx: &mut Ctx,
    kid: InstId,
    model: SavedPosition,
    fixed: Option<(Real, Real)>,
) -> Result<(), SimError> {
    unimplemented!("WP4b: save::save_now")
}

/// A death-only save: counters only.
pub fn save_counters(ctx: &mut Ctx) -> Result<(), SimError> {
    unimplemented!("WP4b: save::save_counters")
}

/// The position the save format gives back.
pub fn quantize(x: Real, y: Real, model: SavedPosition, r: &NumRules) -> (Real, Real) {
    unimplemented!("WP4b: save::quantize")
}

/// Loads a slot: creates the kid at the saved spot and returns it.
pub fn load_into(ctx: &mut Ctx, slot: &SaveSlot) -> Result<InstId, SimError> {
    unimplemented!("WP4b: save::load_into")
}
