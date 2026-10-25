//! Placement and bounding boxes: the only place collision code rounds (docs/plans/sim-core.md
//! 3.4, C1, C2). WP0 declares; WP3 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP3 fills the bodies")]

use super::xform::Xform;
use crate::def::{GameDef, IRect, Mask};
use crate::error::SimError;
use crate::num::Real;
use crate::rules::CollideRules;
use crate::world::Instance;

/// A mask placed in the room.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlacedMask<'a> {
    /// Bounding box in room space, inclusive.
    pub bbox: IRect,
    /// The mask.
    pub mask: &'a Mask,
    /// How sprite space maps to room space.
    pub xf: Xform,
}

/// The integer origin a coordinate places at: banker's rounding (Inferred, C2).
pub fn place_origin(v: Real, rules: &CollideRules) -> Result<i64, SimError> {
    unimplemented!("WP3: bbox::place_origin")
}

/// The instance's mask placed as if it stood at `at`; `None` when it has no mask or an empty
/// one. Unit scale, angle 0: `bbox_left = round(x) + (mask.left - origin.x)` (Inferred, C1).
pub fn instance_box<'a>(
    def: &'a GameDef,
    inst: &Instance,
    at: (Real, Real),
    rules: &CollideRules,
) -> Result<Option<PlacedMask<'a>>, SimError> {
    unimplemented!("WP3: bbox::instance_box")
}
