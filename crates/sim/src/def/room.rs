//! Rooms as loaded from a package (docs/plans/sim-core.md 4.1).
//!
//! WP0 declares; WP7a owns afterwards.

use super::ids::{GroupId, KindId, NameId, VarId};
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
    /// Instance creation code, as params.
    pub params: Params,
    /// Instance name, for triggers and scripts.
    pub name: Option<NameId>,
    /// Instance group.
    pub group: Option<GroupId>,
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
            params: Params::default(),
            name: None,
            group: None,
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
    /// `instance_destroy()` in room creation code: `SimError::Unsupported` until the spec says
    /// a no-op (R24, Confirmed).
    DestroySelf,
}
