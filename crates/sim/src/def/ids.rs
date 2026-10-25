//! Typed indices into [`GameDef`](super::GameDef) tables (docs/plans/sim-core.md 1.1, 8.2).
//!
//! Each id is a `u32` index assigned by `crates/format` at load. Ids never hold a `usize`, so
//! they serialize and hash the same on wasm32 and x86_64. WP0 declares; WP7a owns afterwards.

use serde::{Deserialize, Serialize};

macro_rules! def_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize)]
        pub struct $name(pub u32);

        impl $name {
            /// The table index, or `None` if it does not fit in `usize` (never on supported
            /// targets, but the conversion stays checked).
            pub fn index(self) -> Option<usize> {
                usize::try_from(self.0).ok()
            }
        }
    };
}

def_id!(
    /// A kind: a built-in, a preset or a scripted package object. Index into `GameDef::kinds`.
    KindId
);
def_id!(
    /// A room. Index into `GameDef::rooms`.
    RoomId
);
def_id!(
    /// A sprite. Index into `GameDef::sprites`.
    SpriteId
);
def_id!(
    /// A collision mask. Index into `GameDef::masks`.
    MaskId
);
def_id!(
    /// A custom variable name. Ids for names known at load come from the sorted name list
    /// (plan 1.1), so they never depend on run order.
    VarId
);
def_id!(
    /// An instance name from a room file (`name = "..."`). Index into `GameDef::names`.
    NameId
);
def_id!(
    /// An instance group from a room file. Index into `GameDef::groups`.
    GroupId
);
def_id!(
    /// A sound. Index into `GameDef::sounds`.
    SoundId
);
def_id!(
    /// A timeline. Index into `GameDef::timelines`.
    TimelineId
);
def_id!(
    /// A path. Index into `GameDef::paths`.
    PathId
);
def_id!(
    /// A script file of a scripted kind. Index into the script host's table (WP9).
    ScriptId
);
