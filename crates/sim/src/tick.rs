//! The tick pipeline (docs/plans/sim-core.md 2.1, 2.2).
//!
//! One function per row of the table in plan 2.1. Phase placement that the spec marks Inferred
//! or UNKNOWN is a `TickRules` field, never an `if` in built-ins. WP0 declares; WP1 owns.
#![allow(unused_variables, reason = "WP0 stub: WP1 fills the body")]

use crate::error::SimError;
use crate::input::InputFrame;
use crate::timing::Split;
use crate::world::Ctx;

/// Runs one tick (rows 0-16) and returns the splits it recorded. An error halts the sim. When a
/// phase returns `Flow::Halt` (the world is already `Halted`), the tick stops there: no sweep,
/// no room change, no `tick += 1`.
pub fn run_tick(ctx: &mut Ctx, input: InputFrame) -> Result<Vec<Split>, SimError> {
    unimplemented!("WP1: tick::run_tick")
}
