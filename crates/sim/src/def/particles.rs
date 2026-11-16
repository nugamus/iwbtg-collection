//! Particle system definitions from `particles/<id>.toml` (docs/format/v0.md, Particles; #765).
//!
//! One [`PartSystemDef`] is what the original's code builds in one go (rely objEffect Create):
//! its types, its emitters with their regions, streams and bursts, and its
//! `part_particles_create` calls. The `particle_system` built-in builds it in
//! `World::particles` (`crate::particles`). Only the settings that decide draws reach the sim's
//! [`PartType`]; the rest ([`PartLook`]) is for the client's drawing.

use crate::num::Real;
use crate::particles::{PartRange, PartType, Region};

use super::ids::SpriteId;

/// One particle system definition.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PartSystemDef {
    /// The file stem.
    pub name: String,
    /// Types in name order (`part_type_create` order). Step and death types in [`PartType`] and
    /// the emitters' and creates' types are indexes into this list.
    pub types: Vec<PartTypeDef>,
    /// Emitters in creation order.
    pub emitters: Vec<PartEmitterDef>,
    /// `part_particles_create` calls, after every emitter's burst.
    pub creates: Vec<PartCreateDef>,
    /// `part_system_depth` (drawing only).
    pub depth: Real,
}

/// One particle type.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PartTypeDef {
    /// The name in `[types.<name>]`.
    pub name: String,
    /// The settings that decide draws; step and death types are local indexes.
    pub draws: PartType,
    /// Drawing-only settings.
    pub look: PartLook,
}

/// A particle type's drawing-only settings (no draws; GM's `part_type_*` arguments).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PartLook {
    /// `part_type_shape` word (`pixel`, `disk`, ...); `None` with a sprite.
    pub shape: Option<String>,
    /// `part_type_sprite`: sprite, animate, stretch.
    pub sprite: Option<(SpriteId, bool, bool)>,
    /// `part_type_size` increment and wiggle.
    pub size_change: [Real; 2],
    /// `part_type_scale` x and y.
    pub scale: [Real; 2],
    /// `part_type_speed` increment and wiggle.
    pub speed_change: [Real; 2],
    /// `part_type_direction` increment and wiggle.
    pub direction_change: [Real; 2],
    /// `part_type_orientation` increment and wiggle.
    pub orientation_change: [Real; 2],
    /// `part_type_orientation` relative.
    pub orientation_relative: bool,
    /// `part_type_gravity` amount and direction.
    pub gravity: [Real; 2],
    /// Colors as `0xRRGGBB`: one to three for `part_type_color1/2/3`, two for `color_mix`.
    pub colors: Vec<u32>,
    /// `part_type_alpha1/2/3`, one to three values. A range that draws (`max > min`, the
    /// format's `{ random = [min, max] }`) is one value the client picks per built system from
    /// its own generator (client#826); a fixed range is that value.
    pub alpha: Vec<PartRange>,
    /// `part_type_blend` additive.
    pub additive: bool,
}

/// One emitter.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PartEmitterDef {
    /// `part_emitter_region` xmin, xmax, ymin, ymax in room coordinates (drawing only: the sim
    /// draws points in unit coordinates, `Region::draw_point`).
    pub rect: [Real; 4],
    /// Shape and distribution.
    pub region: Region,
    /// `part_emitter_stream`: local type index and number.
    pub stream: Option<(u32, i32)>,
    /// `part_emitter_burst` when the system is built: local type index and number.
    pub burst: Option<(u32, i32)>,
}

/// One `part_particles_create`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PartCreateDef {
    /// Where, in room coordinates (drawing only).
    pub at: [Real; 2],
    /// Local type index.
    pub ty: u32,
    /// How many.
    pub number: i32,
}
