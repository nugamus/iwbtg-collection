//! Contact moves (docs/plans/sim-core.md 3.4, C10, C11, C14). WP0 declares; WP3 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP3 fills the bodies")]

use crate::error::SimError;
use crate::num::Real;
use crate::world::{Ctx, InstId};

/// GML `move_contact_solid` (Inferred): unit steps `(dcos d, -dsin d)` until one more step would
/// hit a solid, at most `maxdist` steps (C11), no limit when negative (bounded by room plus mask
/// size, then `Unsupported`). Overlapping start per C10.
pub fn move_contact_solid(ctx: &mut Ctx, me: InstId, dir: Real, maxdist: Real) -> Result<(), SimError> {
    unimplemented!("WP3: contact::move_contact_solid")
}

/// GML `move_outside_solid`.
pub fn move_outside_solid(ctx: &mut Ctx, me: InstId, dir: Real, maxdist: Real) -> Result<(), SimError> {
    unimplemented!("WP3: contact::move_outside_solid")
}

/// GML `move_bounce_solid` (C14, UNKNOWN U-MECH-5): `Unsupported` until specified. Tier 3.
pub fn move_bounce_solid(ctx: &mut Ctx, me: InstId, precise: bool) -> Result<(), SimError> {
    unimplemented!("WP3: contact::move_bounce_solid")
}

/// C11: how many unit steps `move_contact_solid` may take for `maxdist`; `None` for no limit.
pub fn mcs_step_limit(maxdist: Real) -> Result<Option<u64>, SimError> {
    unimplemented!("WP3: contact::mcs_step_limit")
}
