//! Kid access for other WPs (docs/plans/sim-core.md 1.6).
//!
//! Built-ins never touch `KidState` fields directly; they call these. WP0 declares the
//! signatures; WP5a owns the bodies.
#![allow(unused_variables, reason = "WP0 stubs: WP5a fills the bodies")]

use crate::def::Params;
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId, Value, World};

/// A kid parameter zone's settings (rainbow, showdown; `kid_param_zone` built-in).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ParamZone {
    /// The kid fields the zone sets, by profile name.
    pub params: Params,
}

/// What the bow copies from the kid each Step (1-tick lag, Inferred).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BowSnapshot {
    /// Kid x.
    pub x: Real,
    /// Kid y.
    pub y: Real,
    /// Kid `image_xscale`.
    pub xscale: Real,
    /// Kid `image_yscale`.
    pub yscale: Real,
    /// Kid `image_alpha`.
    pub alpha: Real,
}

/// Why the kid died.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KillCause {
    /// A killer instance.
    Killer(InstId),
    /// The suicide key (Q).
    Suicide,
    /// Left the room or crossed its boundary.
    Boundary,
    /// A built-in or script killed it (`killPlayer`).
    Script,
}

/// A kid field by its GM name, for the probe registry (plan 9.3): `djump`, `curJumps`,
/// `jump[1]` is `{ name: "jump", index: Some(1) }`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KidField {
    /// GM variable name.
    pub name: String,
    /// Array index, if any.
    pub index: Option<u32>,
}

/// The kid's facing, -1 or 1.
pub fn facing(w: &World, kid: InstId) -> i8 {
    unimplemented!("WP5a: kid::api::facing")
}

/// Refreshes the air jump (water, refreshers, platforms).
pub fn refresh_air_jump(ctx: &mut Ctx, kid: InstId) {
    unimplemented!("WP5a: kid::api::refresh_air_jump")
}

/// Sets the on-platform flag.
pub fn set_on_platform(ctx: &mut Ctx, kid: InstId, on: bool) {
    unimplemented!("WP5a: kid::api::set_on_platform")
}

/// Applies a parameter zone.
pub fn apply_param_zone(ctx: &mut Ctx, kid: InstId, z: &ParamZone) {
    unimplemented!("WP5a: kid::api::apply_param_zone")
}

/// What the bow copies from the kid.
pub fn bow_snapshot(w: &World, kid: InstId) -> BowSnapshot {
    unimplemented!("WP5a: kid::api::bow_snapshot")
}

/// Kills the kid per the family's `[death]` order.
pub fn kill(ctx: &mut Ctx, kid: InstId, cause: KillCause) -> Flow {
    unimplemented!("WP5a: kid::api::kill")
}

/// A kid field for the probe; `None` when this kid has no such field.
pub fn read_field(w: &World, kid: InstId, f: &KidField) -> Option<Value> {
    unimplemented!("WP5a: kid::api::read_field")
}
