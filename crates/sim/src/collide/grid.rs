//! Broadphase grid (docs/plans/sim-core.md 3.4, Design).
//!
//! Uniform 64 px cells per room, all cell math in `i64` with checked arithmetic (never an
//! `as usize` cast: wasm32 has a 32-bit `usize`). A cache in `World`, rebuilt on room entry and
//! savestate restore, never hashed or serialized; results are re-sorted by `World::first_of`
//! order, so the grid never changes an outcome. WP0 declares; WP3 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP3 fills the bodies")]

use crate::def::GameDef;
use crate::error::SimError;
use crate::rules::CollideRules;
use crate::world::{InstId, World};

/// Cell size in pixels.
pub const CELL: i64 = 64;

/// The broadphase cache. Empty until WP3 adds its buckets.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Grid {}

impl Grid {
    /// Rebuilds every bucket from the world's instances.
    pub fn rebuild(w: &World, def: &GameDef, rules: &CollideRules) -> Result<Grid, SimError> {
        unimplemented!("WP3: Grid::rebuild")
    }

    /// Candidate instances whose boxes may touch the inclusive room rectangle, unsorted.
    pub fn candidates(&self, x1: i64, y1: i64, x2: i64, y2: i64) -> Result<Vec<InstId>, SimError> {
        unimplemented!("WP3: Grid::candidates")
    }
}
