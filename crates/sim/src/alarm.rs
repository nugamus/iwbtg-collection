//! Alarms (docs/plans/sim-core.md 2.1 row 4, R3, R3b). WP0 declares; WP1 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP1 fills the bodies")]

use crate::error::SimError;
use crate::num::Real;
use crate::rules::NumRules;
use crate::world::Instance;

/// One alarm's countdown: if the value is `> 0`, subtract 1; returns true when it reaches 0
/// (fire). A value `<= 0` is idle (Inferred, R3).
pub fn tick(alarm: &mut Real) -> bool {
    unimplemented!("WP1: alarm::tick")
}

/// GML `alarm[n] = v`: stored through `num::alarm_from_real` (R3b); rejects `n >= 12` and
/// non-finite values.
pub fn set(inst: &mut Instance, n: u8, v: Real, rules: &NumRules) -> Result<(), SimError> {
    unimplemented!("WP1: alarm::set")
}
