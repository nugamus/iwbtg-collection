//! The immutable game definition the sim runs (docs/plans/sim-core.md 1, 3.4, 4.1, 8).
//!
//! `crates/format` parses a package into a [`GameDef`]. The sim never reads files and never
//! changes a `GameDef`. File owners after WP0 (plan 13, "File overlap check"): `physics.rs`
//! WP5a, `builtin.rs` WP6a, `sound.rs` WP10, `timeline.rs` and `path.rs` WP2, `class.rs` WP11,
//! the rest WP7a.

pub mod builtin;
pub mod class;
pub mod game;
pub mod ids;
pub mod kind;
pub mod path;
pub mod physics;
pub mod room;
pub mod sound;
pub mod sprite;
pub mod testing;
pub mod timeline;

pub use builtin::Builtin;
pub use class::ClassMask;
pub use game::{
    DeathOnRestart, EngineKind, GameDef, ResetKind, ResetModel, RestartKind, RestartModel, RestartTrigger,
};
pub use ids::{
    GroupId, KindId, MaskId, NameId, PathId, RoomId, ScriptId, SoundId, SpriteId, TimelineId, VarId,
};
pub use kind::{KindDef, ParamValue, Params, RankEntry, RankTable, VariantSel};
pub use path::PathDef;
pub use physics::{KidFamily, PhysicsProfile, PhysicsProfileDraft};
pub use room::{PlacedInstance, RoleSet, RoomAction, RoomDef, RoomRole, ViewDef};
pub use sound::{SoundDef, SoundKind};
pub use sprite::{BitMask, IRect, Mask, MaskSet, MaskShape, SpriteDef};
pub use timeline::TimelineDef;
