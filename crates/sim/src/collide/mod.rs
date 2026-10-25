//! Collision: placed masks, overlap, queries, contact moves, the solid put-back, the broadphase
//! grid (docs/plans/sim-core.md 3.4, register C1-C15).
//!
//! `bbox.rs` is the only place that rounds. The grid never decides order: results are re-sorted
//! by `World::first_of` order. WP0 declares; WP3 owns.

pub mod bbox;
pub mod contact;
pub mod grid;
pub mod overlap;
pub mod query;
pub mod resolve;
pub mod xform;

pub use bbox::{PlacedMask, instance_box, place_origin};
pub use contact::{move_bounce_solid, move_contact_solid, move_outside_solid};
pub use query::{
    Target, collision_line, collision_point, collision_rectangle, distance_to_object, instance_place,
    instance_position, place_empty, place_free, place_meeting,
};
pub use resolve::solid_resolve;
pub use xform::Xform;
