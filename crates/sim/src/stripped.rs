//! Ranked (stripped) mode: the run's play mode and the package's `[stripped]` table resolved to
//! ids (docs/format/v0.md, Stripped mode; docs/product.md, Two ways to play each game; #561).
//!
//! A [`PlayMode::Stripped`] run starts at its map's start (`GameDef::map_start`) with its globals set
//! (`room::start_game`), sends every room change to a cut room to its `to` room, and leaves out
//! cut kinds' placed instances and room `spawn` actions and cut placed instances (the room
//! change sequence, `room.rs`). A [`PlayMode::Original`] run ignores all of it.
//!
//! Cutscene and dialogue regions (#34): [`begin_tick`], at tick row 0 before the input is
//! latched, looks at which regions hold in the world as the previous tick left it. A tick that
//! starts with a region holding is frozen: it does not count as ranked time
//! (`TimingState::ranked_ticks`). A dialogue region that has held for `timeout_ticks` ticks
//! without a press of its `advance` action presses it in that tick's frame. That press is
//! derived from the world, so the replay keeps the player's frame and never stores it.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::def::{GameDef, KindId, NameId, RoomId, VarId};
use crate::input::{Action, InputFrame};
use crate::world::{Value, World};

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
    /// `[stripped].start`. A run starts at its map's start (`MapDef::start`, this one merged
    /// with the map's `stripped_start`); this one only when `GameDef::maps` is empty.
    pub start: RankedStart,
    /// Cut rooms: any room change to the key goes to the value instead.
    pub cut_rooms: BTreeMap<RoomId, RoomId>,
    /// Cut kinds: their placed instances and room `spawn` actions are left out.
    pub cut_kinds: BTreeSet<KindId>,
    /// Cut placed instances, by room and instance name.
    pub cut_instances: BTreeSet<(RoomId, NameId)>,
    /// Cutscene regions: they freeze the ranked timer ([`begin_tick`]).
    pub cutscenes: Vec<Region>,
    /// Dialogue regions: they freeze the ranked timer and press `advance` on a timeout.
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

/// A cutscene region: the ranked timer is frozen while it holds ([`begin_tick`]).
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

/// A dialogue region: a cutscene region that advances on its own after a timeout ([`begin_tick`]).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DialogueRegion {
    /// The id and condition.
    pub region: Region,
    /// The action the original reads to advance the text.
    pub advance: Action,
    /// Ticks without a press of `advance` before the sim presses it (at least 1).
    pub timeout_ticks: u32,
}

impl RegionCondition {
    /// Whether the region holds in `w`: an alive, active instance of the kind exists (what
    /// `instance_exists` counts), or the world is in the room.
    pub fn holds(self, w: &World) -> bool {
        match self {
            RegionCondition::Kind(kind) => w.iter_alive().any(|i| i.active && i.kind == kind),
            RegionCondition::Room(room) => w.room == room,
        }
    }
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

/// Whether a cutscene or dialogue region holds in `w`: always false outside a
/// [`PlayMode::Stripped`] run. Between ticks it says whether the next tick is frozen
/// ([`begin_tick`] looks at the same world); the client's skip runs ticks while it holds (#573).
pub fn region_holds(w: &World, def: &GameDef) -> bool {
    let regions = &def.stripped;
    w.mode == PlayMode::Stripped
        && (regions.cutscenes.iter().any(|r| r.holds.holds(w))
            || regions.dialogues.iter().any(|d| d.region.holds.holds(w)))
}

/// Tick row 0, before `input` is latched: the region step of #34. Returns the frame the tick
/// runs with: `input`, plus each dialogue's derived press of `advance` (held and pressed).
///
/// Regions are looked at in the world as the previous tick left it (the room after its room
/// change, instances after its sweep), and only in a [`PlayMode::Stripped`] run.
///
/// - The ranked timer: while the timing chain has started and not ended, the tick counts as
///   `TimingState::frozen_ticks` when any region holds, else as `ranked_ticks`. The tick `end`
///   fires in still counts and the tick `start` fires in does not, so the two add up to
///   `end.tick - start.tick` (`timing` module docs).
/// - Each dialogue region counts the ticks it has held in `TimingState::dialogue_waits`: a
///   region that does not hold resets its count to 0; a real press of `advance` in `input`
///   resets it to 0 and the tick runs as the player gave it; otherwise the count goes up by one,
///   and when it reaches `timeout_ticks` the frame gets the press and the count goes back to 0.
///   So the first derived press comes in the region's `timeout_ticks`-th tick.
/// - The derived press is press-only: no `released` edge follows on the next tick, because no
///   text box in the 20 games advances on a release (docs/format/v0.md, Stripped mode; #574).
pub fn begin_tick(w: &mut World, def: &GameDef, input: InputFrame) -> InputFrame {
    let stripped = w.mode == PlayMode::Stripped;
    let regions = &def.stripped;
    let frozen = region_holds(w, def);
    let t = &mut w.timing;
    if t.started && !t.ended {
        if frozen {
            t.frozen_ticks = t.frozen_ticks.saturating_add(1);
        } else {
            t.ranked_ticks = t.ranked_ticks.saturating_add(1);
        }
    }
    if !stripped || regions.dialogues.is_empty() {
        return input;
    }
    let holds: Vec<bool> = regions
        .dialogues
        .iter()
        .map(|d| d.region.holds.holds(w))
        .collect();
    let waits = &mut w.timing.dialogue_waits;
    waits.resize(regions.dialogues.len(), 0);
    let mut frame = input;
    for ((d, wait), held) in regions.dialogues.iter().zip(waits.iter_mut()).zip(holds) {
        if !held || input.was_pressed(d.advance) {
            *wait = 0;
            continue;
        }
        *wait = wait.saturating_add(1);
        if *wait >= d.timeout_ticks {
            *wait = 0;
            frame.held |= d.advance.bit();
            frame.pressed |= d.advance.bit();
        }
    }
    frame
}
