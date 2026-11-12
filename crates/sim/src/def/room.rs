//! Rooms as loaded from a package (docs/plans/sim-core.md 4.1).
//!
//! WP0 declares; WP7a owns afterwards.

use super::ids::{GroupId, KindId, NameId, SoundId, VarId};
use super::kind::Params;
use crate::num::Real;
use crate::world::Value;

/// One room.
#[derive(Clone, PartialEq, Debug)]
pub struct RoomDef {
    /// Package room id (`room_1`).
    pub name: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// What the room is for; replaces the engines' "not rInit/rMenu/rTitle/rSelectStage" tests.
    pub role: RoomRole,
    /// R does nothing here (animus room52: its world Key R event is off in that room, #445).
    pub no_restart: bool,
    /// The double jump also sets `tjump` here (showdown room028: playerJump step 2, A3.md C4
    /// showdown). Only read with `jump.air = "djump_then_tjump"`.
    pub djump_grants_tjump: bool,
    /// `room_speed` on entry, before the world's Room Start (Inferred, R14).
    pub file_speed: u32,
    /// GM persistent room.
    pub persistent: bool,
    /// GM "enable the use of views".
    pub views_enabled: bool,
    /// The room's eight views.
    pub views: [ViewDef; 8],
    /// Placed instances in file order, which is creation order (Inferred). Never sorted.
    pub instances: Vec<PlacedInstance>,
    /// Room creation code, as data.
    pub on_create: Vec<RoomAction>,
}

impl RoomDef {
    /// A play room of `width` x `height` with file speed 50, not persistent, no views, no
    /// instances.
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> RoomDef {
        RoomDef {
            name: name.into(),
            width,
            height,
            role: RoomRole::Play,
            no_restart: false,
            djump_grants_tjump: false,
            file_speed: 50,
            persistent: false,
            views_enabled: false,
            views: Default::default(),
            instances: Vec::new(),
            on_create: Vec::new(),
        }
    }
}

/// What a room is for (CR6). Defaults to `Play` when the converter does not say.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub enum RoomRole {
    /// The engine's init room.
    Boot,
    /// Title screen.
    Title,
    /// Menu (file select, options).
    Menu,
    /// Difficulty or stage select.
    Select,
    /// Gameplay.
    #[default]
    Play,
}

impl RoomRole {
    /// The role's bit in a [`RoleSet`].
    pub const fn bit(self) -> u8 {
        match self {
            RoomRole::Boot => 1 << 0,
            RoomRole::Title => 1 << 1,
            RoomRole::Menu => 1 << 2,
            RoomRole::Select => 1 << 3,
            RoomRole::Play => 1 << 4,
        }
    }
}

/// A set of room roles, one bit each ([`RoomRole::bit`]).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct RoleSet(pub u8);

impl RoleSet {
    /// True when `role` is in the set.
    pub const fn contains(self, role: RoomRole) -> bool {
        self.0 & role.bit() != 0
    }
}

/// One view as the room file sets it.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ViewDef {
    /// Visible when the room starts.
    pub visible: bool,
    /// View in the room: x, y, width, height.
    pub view: (Real, Real, Real, Real),
    /// Port on the screen: x, y, width, height.
    pub port: (Real, Real, Real, Real),
    /// Object to follow, by kind.
    pub follow: Option<KindId>,
    /// Horizontal border.
    pub hborder: Real,
    /// Vertical border.
    pub vborder: Real,
    /// Horizontal speed limit (-1 is no limit).
    pub hspeed: Real,
    /// Vertical speed limit (-1 is no limit).
    pub vspeed: Real,
}

/// One instance placed in a room file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlacedInstance {
    /// Its kind.
    pub kind: KindId,
    /// Dispatch rank, from `object_order` (selectors included).
    pub rank: u16,
    /// Position.
    pub x: Real,
    /// Position.
    pub y: Real,
    /// `image_xscale`.
    pub xscale: Real,
    /// `image_yscale`.
    pub yscale: Real,
    /// `image_angle`.
    pub angle: Real,
    /// `visible` from the room file; `None` keeps the kind's.
    pub visible: Option<bool>,
    /// `depth` from the room file; `None` keeps the kind's.
    pub depth: Option<Real>,
    /// Instance creation code, as params.
    pub params: Params,
    /// Instance name, for triggers and scripts.
    pub name: Option<NameId>,
    /// Instance group.
    pub group: Option<GroupId>,
    /// GM8 instance id from the room file (`gm_id`, #680): unique in the room, may repeat in
    /// other rooms. `None` when the package does not give one. Not used by the sim yet (#669).
    pub gm_id: Option<u32>,
}

impl PlacedInstance {
    /// An instance at `(x, y)` with unit scale, angle 0, no params, no name, no group.
    pub fn at(kind: KindId, rank: u16, x: Real, y: Real) -> PlacedInstance {
        PlacedInstance {
            kind,
            rank,
            x,
            y,
            xscale: Real::ONE,
            yscale: Real::ONE,
            angle: Real::ZERO,
            visible: None,
            depth: None,
            params: Params::default(),
            name: None,
            group: None,
            gm_id: None,
        }
    }
}

/// One room creation code action, as data (plan 4.2 step 6).
#[derive(Clone, PartialEq, Debug)]
pub enum RoomAction {
    /// `global.<var> = value`.
    SetGlobal {
        /// The global.
        var: VarId,
        /// Its new value.
        value: Value,
        /// Set for the global `difficulty` (#568): the engine difficulty built-ins read, set with it.
        difficulty: Option<u8>,
    },
    /// `instance_create(x, y, kind)`.
    Spawn {
        /// The kind.
        kind: KindId,
        /// Position.
        x: Real,
        /// Position.
        y: Real,
        /// Params for the new instance.
        params: Params,
    },
    /// The engine's `loadGame` from room creation code (kaenbyou, eat-the-lemon rLoad; R23).
    LoadGame,
    /// `instance_destroy()` in room creation code: destroys nothing and the rest of the code
    /// runs (R24, Confirmed).
    DestroySelf,
    /// `global.<var>[index] = value` (#629, showdown `global.stage[global.savenum]`). The element
    /// is the run-time variable `<var>[<index>]` (`World::var_names`).
    SetGlobalIndexed {
        /// The array's global name.
        var: String,
        /// Where the index comes from.
        index: GlobalIndex,
        /// The element's new value.
        value: Value,
    },
    /// The game's save as its save point runs it (`[save]` position model); with no kid the
    /// slot holds x 0, y 0 (kid/A3.md "Saves", showdown a3-t20-a; #629).
    SaveGame,
    /// The game's death and time save: the slot's counters only (`save::save_counters`).
    SaveDeathTime,
    /// `player.x = x; player.y = y`: moves the first kid; nothing with no kid (#629).
    MoveKid {
        /// Position.
        x: Real,
        /// Position.
        y: Real,
    },
    /// `with (kind) instance_destroy()`: every instance of the kind, in event order (#629).
    Destroy {
        /// The kind.
        kind: KindId,
    },
    /// `sound_play` (`looping` false) or `sound_loop` (true).
    PlaySound {
        /// The sound.
        sound: SoundId,
        /// `sound_loop`.
        looping: bool,
    },
    /// `sound_stop`.
    StopSound {
        /// The sound.
        sound: SoundId,
    },
    /// `action` runs only while `when` holds (a row's `when`, #629).
    When {
        /// The condition.
        when: RoomWhen,
        /// The row.
        action: Box<RoomAction>,
    },
}

/// The index of [`RoomAction::SetGlobalIndexed`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GlobalIndex {
    /// A whole number.
    Const(u32),
    /// `global.savenum`: the save slot (`RunGlobals::slot`).
    Savenum,
    /// A package global, read when the row runs; it must hold a whole number from 0.
    Global(VarId),
}

/// A room action's condition; every part that is set must hold.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct RoomWhen {
    /// `global.difficulty < n` (the engine difficulty).
    pub difficulty_below: Option<Real>,
    /// A kid exists (`Some(true)`) or none does (`Some(false)`).
    pub kid_exists: Option<bool>,
    /// No alive, active instance of the kind exists.
    pub kind_absent: Option<KindId>,
}
