//! GML collision queries (docs/plans/sim-core.md 3.4, C12, C13, R21, R22).
//!
//! Each excludes `me`, skips inactive instances, skips dead ones unless
//! `TickRules::dead_in_queries` (R22), and returns `World::first_of` order (R21).
//! WP0 declares; WP3 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP3 fills the bodies")]

use serde::{Deserialize, Serialize};

use crate::def::{ClassMask, KindId};
use crate::error::SimError;
use crate::num::Real;
use crate::world::{Ctx, InstId};

/// What a query looks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Target {
    /// Any instance with one of these classes (GM parent emulation).
    Class(ClassMask),
    /// Instances of one kind.
    Kind(KindId),
    /// One instance.
    Inst(InstId),
    /// Any instance (`all`).
    Any,
}

/// GML `place_free`: no solid instance at `(x, y)` (solids only, Confirmed).
pub fn place_free(ctx: &Ctx, me: InstId, x: Real, y: Real) -> Result<bool, SimError> {
    unimplemented!("WP3: query::place_free")
}

/// GML `place_empty`: no instance at all at `(x, y)`.
pub fn place_empty(ctx: &Ctx, me: InstId, x: Real, y: Real) -> Result<bool, SimError> {
    unimplemented!("WP3: query::place_empty")
}

/// GML `place_meeting`.
pub fn place_meeting(ctx: &Ctx, me: InstId, x: Real, y: Real, t: Target) -> Result<bool, SimError> {
    unimplemented!("WP3: query::place_meeting")
}

/// GML `instance_place`.
pub fn instance_place(
    ctx: &Ctx,
    me: InstId,
    x: Real,
    y: Real,
    t: Target,
) -> Result<Option<InstId>, SimError> {
    unimplemented!("WP3: query::instance_place")
}

/// GML `instance_position`.
pub fn instance_position(ctx: &Ctx, x: Real, y: Real, t: Target) -> Result<Option<InstId>, SimError> {
    unimplemented!("WP3: query::instance_position")
}

/// GML `collision_point`.
pub fn collision_point(
    ctx: &Ctx,
    x: Real,
    y: Real,
    t: Target,
    prec: bool,
    notme: Option<InstId>,
) -> Result<Option<InstId>, SimError> {
    unimplemented!("WP3: query::collision_point")
}

/// GML `collision_rectangle`; corners per C12.
#[allow(clippy::too_many_arguments, reason = "mirrors GML collision_rectangle")]
pub fn collision_rectangle(
    ctx: &Ctx,
    x1: Real,
    y1: Real,
    x2: Real,
    y2: Real,
    t: Target,
    prec: bool,
    notme: Option<InstId>,
) -> Result<Option<InstId>, SimError> {
    unimplemented!("WP3: query::collision_rectangle")
}

/// GML `collision_line`; cells per C12.
#[allow(clippy::too_many_arguments, reason = "mirrors GML collision_line")]
pub fn collision_line(
    ctx: &Ctx,
    x1: Real,
    y1: Real,
    x2: Real,
    y2: Real,
    t: Target,
    prec: bool,
    notme: Option<InstId>,
) -> Result<Option<InstId>, SimError> {
    unimplemented!("WP3: query::collision_line")
}

/// GML `distance_to_object`: the gap between bounding boxes per C13; `None` when no instance
/// exists (A3 U4; callers treat it as a large value).
pub fn distance_to_object(ctx: &Ctx, me: InstId, t: Target) -> Result<Option<Real>, SimError> {
    unimplemented!("WP3: query::distance_to_object")
}
