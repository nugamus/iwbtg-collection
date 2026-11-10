//! Ranked (stripped) mode: the run's play mode and the package's `[stripped]` table resolved to
//! ids (docs/format/v0.md, Stripped mode; docs/product.md, Two ways to play each game; #561).
//!
//! A [`PlayMode::Stripped`] run starts at [`StrippedDef::start`] with its globals set
//! (`room::start_game`), sends every room change to a cut room to its `to` room, and leaves out
//! cut kinds' placed instances and room `spawn` actions and cut placed instances (the room
//! change sequence, `room.rs`). A [`PlayMode::Original`] run ignores all of it.
//!
//! The cutscene and dialogue regions are resolved here; what they do (the ranked timer freeze,
//! the dialogue auto-advance) is #34.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::def::{KindId, NameId, RoomId, VarId};
use crate::input::Action;
use crate::world::Value;

/// How a run plays the game (docs/product.md, Two ways to play each game). Recorded in the
/// replay header and hashed with the world, so a stripped and an original run never share a
/// state hash.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum PlayMode {
    /// Play original: the full game as ported, menus and all; `[stripped]` is ignored.
    #[default]
    Original,
    /// Ranked: the game without its frame, as `[stripped]` says.
    Stripped,
}

impl PlayMode {
    /// The CLI and replay dump name: `original` or `stripped`.
    pub fn name(self) -> &'static str {
        match self {
            PlayMode::Original => "original",
            PlayMode::Stripped => "stripped",
        }
    }

    /// Parses [`PlayMode::name`] (`ranked` is accepted for `stripped`).
    pub fn parse(s: &str) -> Result<PlayMode, String> {
        match s {
            "original" => Ok(PlayMode::Original),
            "stripped" | "ranked" => Ok(PlayMode::Stripped),
            other => Err(format!("unknown play mode `{other}` (original or stripped)")),
        }
    }
}

/// The `[stripped]` table of a package, resolved. Empty (the default) when the package has none:
/// a stripped run then starts in `GameDef::new_game_room` and nothing is cut.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct StrippedDef {
    /// `[stripped].start`: the ranked start point the sim uses.
    pub start: RankedStart,
    /// Each map's start, `stripped_start` merged over `[stripped].start` key by key (globals by
    /// name), by map id; one entry `main` for a package without `[[maps]]`. The sim does not pick
    /// a map yet (#505); until then it starts at [`StrippedDef::start`].
    pub map_starts: BTreeMap<String, RankedStart>,
    /// Cut rooms: any room change to the key goes to the value instead.
    pub cut_rooms: BTreeMap<RoomId, RoomId>,
    /// Cut kinds: their placed instances and room `spawn` actions are left out.
    pub cut_kinds: BTreeSet<KindId>,
    /// Cut placed instances, by room and instance name.
    pub cut_instances: BTreeSet<(RoomId, NameId)>,
    /// Cutscene regions (behavior: #34).
    pub cutscenes: Vec<Region>,
    /// Dialogue regions (behavior: #34).
    pub dialogues: Vec<DialogueRegion>,
}

/// Where a ranked run starts.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct RankedStart {
    /// The start room; `None` is `GameDef::new_game_room`.
    pub room: Option<RoomId>,
    /// GML globals set before the start room's creation, in name order.
    pub globals: Vec<(VarId, Value)>,
    /// The engine's difficulty, from a global named `difficulty` (what built-ins read,
    /// `RunGlobals::difficulty`); `None` keeps the `StartPoint::NewGame` difficulty.
    pub difficulty: Option<u8>,
}

/// A cutscene region: the ranked timer is frozen while it holds (#34).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Region {
    /// The region id from `game.toml`.
    pub id: String,
    /// When it holds.
    pub holds: RegionCondition,
}

/// When a region holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegionCondition {
    /// While any instance of this kind exists.
    Kind(KindId),
    /// While the world is in this room.
    Room(RoomId),
}

/// A dialogue region: a cutscene region that advances on its own after a timeout (#34).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DialogueRegion {
    /// The id and condition.
    pub region: Region,
    /// The action the original reads to advance the text.
    pub advance: Action,
    /// Ticks without a press of `advance` before the sim presses it (at least 1).
    pub timeout_ticks: u32,
}

impl StrippedDef {
    /// Where a room change to `room` goes in `mode`: the cut room's `to` in a stripped run, else
    /// `room`. One step: a `to` room is never cut (the loader checks).
    pub fn redirect(&self, mode: PlayMode, room: RoomId) -> RoomId {
        match mode {
            PlayMode::Stripped => self.cut_rooms.get(&room).copied().unwrap_or(room),
            PlayMode::Original => room,
        }
    }

    /// Whether `mode` leaves out instances (placed or room-spawned) of `kind`.
    pub fn kind_cut(&self, mode: PlayMode, kind: KindId) -> bool {
        mode == PlayMode::Stripped && self.cut_kinds.contains(&kind)
    }

    /// Whether `mode` leaves out the placed instance named `name` in `room`.
    pub fn instance_cut(&self, mode: PlayMode, room: RoomId, name: Option<NameId>) -> bool {
        mode == PlayMode::Stripped && name.is_some_and(|n| self.cut_instances.contains(&(room, n)))
    }
}
