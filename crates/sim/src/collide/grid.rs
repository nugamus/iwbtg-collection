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
//!   brute force: [`Grid::is_built`] is false and queries test every instance.
//!
//! **What is bucketed.** Each instance goes into the cells its collision box and its reported
//! box (`bbox::reported_box`) touch, padded by [`PAD`] pixels. An instance with neither box
//! (no mask) is not bucketed; queries never see it. An instance whose boxes cannot be computed
//! (an unsupported angle, say), or that covers more than [`MAX_CELLS`] cells, goes in the
//! overflow set, which every query tests, so its error surfaces exactly where brute force would
//! raise it. Cells are keyed in a `BTreeMap`, so instances outside the room need no special
//! case (Design: the plan's out-of-room overflow list is not needed with sparse cells).

use std::collections::{BTreeMap, BTreeSet};

use super::bbox::{instance_box, reported_box};
use crate::def::{GameDef, IRect};
use crate::error::SimError;
use crate::rules::CollideRules;
use crate::world::{InstId, Instance, World};

/// Cell size in pixels.
pub const CELL: i64 = 64;

/// Pixels added around each bucketed box, so a query that rounds differently by a pixel still
/// finds the instance.
pub const PAD: i64 = 2;

/// Most cells one instance is bucketed in; bigger ones go in the overflow set.
pub const MAX_CELLS: i64 = 1024;

/// Where one instance is bucketed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Slot {
    /// In every cell of this inclusive cell range (cell coordinates).
    Cells(IRect),
    /// In the overflow set.
    Overflow,
    /// Not bucketed: no mask, so no query can find it.
    Nowhere,
}

/// The broadphase cache.
#[derive(Clone, Debug, Default)]
pub struct Grid {
    /// Instances whose box may have changed since they were bucketed (see the module docs).
    moved: BTreeSet<InstId>,
    /// False until [`Grid::rebuild`]: queries then test every instance.
    built: bool,
    /// Bucketed ids per cell, keyed `(cell_y, cell_x)`.
    cells: BTreeMap<(i64, i64), Vec<InstId>>,
    /// Instances every query tests.
    overflow: BTreeSet<InstId>,
    /// Where each bucketed instance sits, to remove it when it moves.
    placed: BTreeMap<InstId, Slot>,
}

/// A cache never makes two worlds differ: two worlds with the same instances are equal whatever
/// their grids hold.
impl PartialEq for Grid {
    fn eq(&self, _other: &Grid) -> bool {
        true
    }
}

impl Eq for Grid {}

/// The cell range of an inclusive pixel box padded by [`PAD`].
fn cell_range(b: IRect) -> Result<IRect, SimError> {
    let pad = |v: i64, d: i64| v.checked_add(d).ok_or_else(|| SimError::overflow("grid cell"));
    Ok(IRect {
        left: pad(b.left, -PAD)?.div_euclid(CELL),
        top: pad(b.top, -PAD)?.div_euclid(CELL),
        right: pad(b.right, PAD)?.div_euclid(CELL),
        bottom: pad(b.bottom, PAD)?.div_euclid(CELL),
    })
}

/// Number of cells in a cell range, or `None` when it does not fit in an i64.
fn cell_count(c: IRect) -> Option<i64> {
    let w = c.right.checked_sub(c.left)?.checked_add(1)?;
    let h = c.bottom.checked_sub(c.top)?.checked_add(1)?;
    w.checked_mul(h)
}

/// Where an instance belongs (module docs).
fn slot_for(inst: &Instance, def: &GameDef, rules: &CollideRules) -> Slot {
    let at = (inst.m.x(), inst.m.y());
    let collision = instance_box(def, inst, at, rules).map(|p| p.map(|p| p.bbox));
    let reported = reported_box(def, inst, at, rules);
    let boxes: Vec<IRect> = match (collision, reported) {
        (Err(_), Err(_)) => return Slot::Overflow,
        (c, r) => [c.ok().flatten(), r.ok().flatten()].into_iter().flatten().collect(),
    };
    let Some(first) = boxes.first() else {
        return Slot::Nowhere;
    };
    let union = boxes.iter().fold(*first, |u, b| IRect {
        left: u.left.min(b.left),
        top: u.top.min(b.top),
        right: u.right.max(b.right),
        bottom: u.bottom.max(b.bottom),
    });
    match cell_range(union) {
        Ok(c) if cell_count(c).is_some_and(|n| n <= MAX_CELLS) => Slot::Cells(c),
        _ => Slot::Overflow,
    }
}

impl Grid {
    /// Rebuilds every bucket from the world's instances.
    pub fn rebuild(w: &World, def: &GameDef, rules: &CollideRules) -> Result<Grid, SimError> {
        let mut g = Grid {
            built: true,
            ..Grid::default()
        };
        for inst in w.instances() {
            g.place(inst, def, rules);
        }
        Ok(g)
    }

    /// True after [`Grid::rebuild`]. An unbuilt grid has no buckets, and queries test every
    /// instance.
    pub fn is_built(&self) -> bool {
        self.built
    }

    /// Buckets one instance that is not bucketed now.
    fn place(&mut self, inst: &Instance, def: &GameDef, rules: &CollideRules) {
        let slot = slot_for(inst, def, rules);
        match slot {
            Slot::Cells(c) => {
                for cy in c.top..=c.bottom {
                    for cx in c.left..=c.right {
                        self.cells.entry((cy, cx)).or_default().push(inst.id);
                    }
                }
            }
            Slot::Overflow => {
                self.overflow.insert(inst.id);
            }
            Slot::Nowhere => {}
        }
        self.placed.insert(inst.id, slot);
    }

    /// Removes one instance from its buckets.
    fn unplace(&mut self, id: InstId) {
        match self.placed.remove(&id) {
            Some(Slot::Cells(c)) => {
                for cy in c.top..=c.bottom {
                    for cx in c.left..=c.right {
                        if let Some(v) = self.cells.get_mut(&(cy, cx)) {
                            v.retain(|i| *i != id);
                            if v.is_empty() {
                                self.cells.remove(&(cy, cx));
                            }
                        }
                    }
                }
            }
            Some(Slot::Overflow) => {
                self.overflow.remove(&id);
            }
            Some(Slot::Nowhere) | None => {}
        }
    }

    /// Candidate instances from the buckets whose boxes may touch the inclusive room rectangle,
    /// unsorted. Queries add [`Grid::moved`] to these (module docs). An unbuilt grid gives none;
    /// queries check [`Grid::is_built`] first.
    pub fn candidates(&self, x1: i64, y1: i64, x2: i64, y2: i64) -> Result<Vec<InstId>, SimError> {
        let mut out: BTreeSet<InstId> = self.overflow.clone();
        let r = IRect {
            left: x1.min(x2),
            top: y1.min(y2),
            right: x1.max(x2),
            bottom: y1.max(y2),
        };
        let c = IRect {
            left: r.left.div_euclid(CELL),
            top: r.top.div_euclid(CELL),
            right: r.right.div_euclid(CELL),
            bottom: r.bottom.div_euclid(CELL),
        };
        // Only rows that hold a cell: a huge query rectangle never walks empty rows.
        if let (Some(((lo, _), _)), Some(((hi, _), _))) = (self.cells.first_key_value(), self.cells.last_key_value()) {
            let top = c.top.max(*lo);
            let bottom = c.bottom.min(*hi);
            let mut cy = top;
            while cy <= bottom {
                for (_, ids) in self.cells.range((cy, c.left)..=(cy, c.right)) {
                    out.extend(ids.iter().copied());
                }
                match cy.checked_add(1) {
                    Some(n) => cy = n,
                    None => break,
                }
            }
        }
        Ok(out.into_iter().collect())
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
    /// through `World::flush_grid`. Entries of instances no longer in `instances` (swept) are
    /// dropped along the way, so the grid does not grow over a long room.
    pub fn flush(&mut self, instances: &[Instance], def: &GameDef, rules: &CollideRules) -> Result<(), SimError> {
        let moved = std::mem::take(&mut self.moved);
        if !self.built {
            // Queries test every instance until the grid is built.
            return Ok(());
        }
        let find = |id: InstId| {
            instances
                .binary_search_by_key(&id, |i| i.id)
                .ok()
                .and_then(|i| instances.get(i))
        };
        for id in moved {
            self.unplace(id);
            if let Some(inst) = find(id) {
                self.place(inst, def, rules);
            }
        }
        let limit = instances.len().saturating_mul(2).saturating_add(64);
        if self.placed.len() > limit {
            let gone: Vec<InstId> = self.placed.keys().copied().filter(|id| find(*id).is_none()).collect();
            for id in gone {
                self.unplace(id);
            }
        }
        Ok(())
    }
}
