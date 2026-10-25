//! Broadphase grid (docs/plans/sim-core.md 3.4, Design).
//!
//! Uniform 64 px cells per room, all cell math in `i64` with checked arithmetic (never an
//! `as usize` cast: wasm32 has a 32-bit `usize`). A cache in `World`, rebuilt on room entry and
//! savestate restore, never hashed or serialized; results are re-sorted by `World::first_of`
//! order, so the grid never changes an outcome. WP0 declares; WP3 owns.
//!
//! **Staying in step without a hook in every setter.** Position and image setters live on
//! `Motion` and `Image`, which cannot reach the grid, and queries take `&Ctx`, so they cannot
//! re-bucket. Instead the only way to get `&mut Instance` is `World::get_mut` (the instance list
//! is private to `world`), and it calls [`Grid::mark_moved`] for that instance; `insert` does the
//! same for new ones. The contract WP3 implements:
//!
//! - A query's candidates are the bucket hits **plus every instance in [`Grid::moved`]**, each
//!   then tested against its current box. Stale bucket entries only cost a test; a moved
//!   instance is never missed.
//! - [`Grid::flush`] re-buckets the moved instances and clears the set. WP1 calls
//!   `World::flush_grid` between handlers or phases (its choice); how often only changes speed.
//! - A grid that was never built (after a savestate restore, before `rebuild`) must answer like
//!   brute force.
#![allow(unused_variables, reason = "WP0 stubs: WP3 fills the bodies")]

use std::collections::BTreeSet;

use crate::def::GameDef;
use crate::error::SimError;
use crate::rules::CollideRules;
use crate::world::{InstId, Instance, World};

/// Cell size in pixels.
pub const CELL: i64 = 64;

/// The broadphase cache. WP3 adds its buckets.
#[derive(Clone, Debug, Default)]
pub struct Grid {
    /// Instances whose box may have changed since they were bucketed (see the module docs).
    moved: BTreeSet<InstId>,
}

/// A cache never makes two worlds differ: two worlds with the same instances are equal whatever
/// their grids hold.
impl PartialEq for Grid {
    fn eq(&self, _other: &Grid) -> bool {
        true
    }
}

impl Eq for Grid {}

impl Grid {
    /// Rebuilds every bucket from the world's instances.
    pub fn rebuild(w: &World, def: &GameDef, rules: &CollideRules) -> Result<Grid, SimError> {
        unimplemented!("WP3: Grid::rebuild")
    }

    /// Candidate instances from the buckets whose boxes may touch the inclusive room rectangle,
    /// unsorted. Queries add [`Grid::moved`] to these (module docs).
    pub fn candidates(&self, x1: i64, y1: i64, x2: i64, y2: i64) -> Result<Vec<InstId>, SimError> {
        unimplemented!("WP3: Grid::candidates")
    }

    /// Records that `id`'s box may have changed. Called by `World::get_mut` and `World::insert`.
    pub fn mark_moved(&mut self, id: InstId) {
        self.moved.insert(id);
    }

    /// Instances marked since the last [`Grid::flush`]; every query treats them as candidates.
    pub fn moved(&self) -> &BTreeSet<InstId> {
        &self.moved
    }

    /// Re-buckets the moved instances from their current boxes and clears the set. Called
    /// through `World::flush_grid`.
    ///
    /// WP0 has no buckets, so there is nothing to re-bucket and the set stays as a superset;
    /// WP3 replaces this body.
    pub fn flush(
        &mut self,
        instances: &[Instance],
        def: &GameDef,
        rules: &CollideRules,
    ) -> Result<(), SimError> {
        Ok(())
    }
}
