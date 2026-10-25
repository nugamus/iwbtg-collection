//! The solid collision put-back (docs/plans/sim-core.md 2.3 step 4, C7, C9, M7).
//! WP0 declares; WP3 owns.
#![allow(unused_variables, reason = "WP0 stub: WP3 fills the body")]

use crate::error::SimError;
use crate::world::{Ctx, Flow, InstId};

/// For a solid pair `(a, b)`: put `a` at its previous place, run the event (`run`), then
/// `x += hspeed; y += vspeed` from wherever the event left it; if `a` overlaps again (C9),
/// put it back where the event left it (Inferred).
pub fn solid_resolve(
    ctx: &mut Ctx,
    a: InstId,
    b: InstId,
    run: impl FnOnce(&mut Ctx) -> Flow,
) -> Result<Flow, SimError> {
    unimplemented!("WP3: resolve::solid_resolve")
}
