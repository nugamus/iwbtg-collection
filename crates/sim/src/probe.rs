//! Read-only trace sampling, compiled into every build (docs/plans/sim-core.md 9.3).
//!
//! Sample point: after tick row 13, before the room change. One row per alive instance of the
//! traced kind, in dispatch order. WP0 declares; WP8a owns.
#![allow(unused_variables, reason = "WP0 stub: WP8a fills the body")]

pub mod registry;

use crate::collide::Target;
use crate::def::{NameId, VarId};
use crate::input::Action;
use crate::kid::api::KidField;
use crate::sim::Sim;
use crate::trace;

/// What to sample each frame.
#[derive(Clone, PartialEq, Debug)]
pub struct Probe {
    /// The traced object: one row per alive instance.
    pub object: Target,
    /// Columns after the recorder's fixed ones.
    pub columns: Vec<Column>,
}

/// One probe column.
#[derive(Clone, PartialEq, Debug)]
pub enum Column {
    /// One of the recorder's fixed columns (`trace::FIXED_COLUMNS`).
    Fixed(FixedColumn),
    /// A field of some instance.
    Field {
        /// Whose field.
        target: ProbeTarget,
        /// Which field.
        field: FieldRef,
    },
    /// A global.
    Global(VarId),
    /// `instance_number(...)`.
    Count(Target),
    /// `keyboard_check*(...)`.
    Key(KeyRead),
    /// A save slot field (decoded save files, plan 9.1 kind 3).
    Save(SaveField),
}

/// The recorder's fixed columns, in `trace::FIXED_COLUMNS` order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FixedColumn {
    /// `frame`.
    Frame,
    /// `room`.
    Room,
    /// `object`.
    Object,
    /// `id` (ignored by diffs).
    Id,
    /// `x`.
    X,
    /// `y`.
    Y,
    /// `xprevious`.
    Xprevious,
    /// `yprevious`.
    Yprevious,
    /// `hspeed`.
    Hspeed,
    /// `vspeed`.
    Vspeed,
    /// `speed`.
    Speed,
    /// `direction`.
    Direction,
    /// `gravity`.
    Gravity,
    /// `gravity_direction`.
    GravityDirection,
    /// `friction`.
    Friction,
    /// `image_index`.
    ImageIndex,
    /// `sprite_index`.
    SpriteIndex,
    /// `sprite_name`.
    SpriteName,
}

impl FixedColumn {
    /// Every fixed column, in `trace::FIXED_COLUMNS` order.
    pub const ALL: [FixedColumn; 18] = [
        FixedColumn::Frame,
        FixedColumn::Room,
        FixedColumn::Object,
        FixedColumn::Id,
        FixedColumn::X,
        FixedColumn::Y,
        FixedColumn::Xprevious,
        FixedColumn::Yprevious,
        FixedColumn::Hspeed,
        FixedColumn::Vspeed,
        FixedColumn::Speed,
        FixedColumn::Direction,
        FixedColumn::Gravity,
        FixedColumn::GravityDirection,
        FixedColumn::Friction,
        FixedColumn::ImageIndex,
        FixedColumn::SpriteIndex,
        FixedColumn::SpriteName,
    ];

    /// The column's name in the recorder CSV.
    pub fn name(self) -> &'static str {
        match self {
            FixedColumn::Frame => "frame",
            FixedColumn::Room => "room",
            FixedColumn::Object => "object",
            FixedColumn::Id => "id",
            FixedColumn::X => "x",
            FixedColumn::Y => "y",
            FixedColumn::Xprevious => "xprevious",
            FixedColumn::Yprevious => "yprevious",
            FixedColumn::Hspeed => "hspeed",
            FixedColumn::Vspeed => "vspeed",
            FixedColumn::Speed => "speed",
            FixedColumn::Direction => "direction",
            FixedColumn::Gravity => "gravity",
            FixedColumn::GravityDirection => "gravity_direction",
            FixedColumn::Friction => "friction",
            FixedColumn::ImageIndex => "image_index",
            FixedColumn::SpriteIndex => "sprite_index",
            FixedColumn::SpriteName => "sprite_name",
        }
    }
}

/// Whose field a column reads.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ProbeTarget {
    /// The instance the row is for.
    Row,
    /// The kid (`World::kid`, R21).
    Kid,
    /// The first instance of a target (`World::first_of`).
    First(Target),
    /// The instance with this room-file name.
    Named(NameId),
}

/// A field, resolved from its GM name by `registry::resolve`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FieldRef {
    /// A built-in variable that is also a fixed column.
    Fixed(FixedColumn),
    /// `alarm[n]`.
    Alarm(u8),
    /// `image_speed`, `image_xscale` and the other image variables not in the fixed columns.
    Image(ImageField),
    /// A custom variable.
    Var(VarId),
    /// A kid field (`kid::api::read_field`), so the probe never depends on `KidState` layout.
    Kid(KidField),
}

/// Image variables a probe can read besides the fixed columns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImageField {
    /// `image_speed`.
    Speed,
    /// `image_xscale`.
    Xscale,
    /// `image_yscale`.
    Yscale,
    /// `image_angle`.
    Angle,
    /// `image_alpha`.
    Alpha,
    /// `visible`.
    Visible,
    /// `solid`.
    Solid,
    /// `depth`.
    Depth,
}

/// A keyboard read column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyRead {
    /// `keyboard_check`.
    Held(Action),
    /// `keyboard_check_pressed`.
    Pressed(Action),
    /// `keyboard_check_released`.
    Released(Action),
    /// `keyboard_check_direct`: **consumes** the latch, as the recorder's read did (I1).
    Direct(Action),
}

/// A save slot field.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveField {
    /// A save exists.
    Exists,
    /// Saved room.
    Room,
    /// Saved x.
    X,
    /// Saved y.
    Y,
    /// Saved flip.
    Flipped,
    /// Saved facing.
    Facing,
    /// Difficulty.
    Difficulty,
    /// Deaths.
    Deaths,
    /// Time.
    Time,
    /// Boss flags.
    Bosses,
    /// Item flags.
    Items,
    /// Clear flag.
    Clear,
}

/// The probe's rows for this frame. Takes `&mut Sim` because `KeyRead::Direct` consumes the
/// latch.
pub fn sample(sim: &mut Sim, probe: &Probe, frame: i64) -> Vec<Vec<trace::Value>> {
    unimplemented!("WP8a: probe::sample")
}

#[cfg(test)]
mod tests {
    use super::FixedColumn;
    use crate::trace::FIXED_COLUMNS;

    #[test]
    fn fixed_columns_match_the_recorder() {
        let names: Vec<&str> = FixedColumn::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(names, FIXED_COLUMNS);
    }
}
