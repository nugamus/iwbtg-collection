//! Candidate gathering for the collision phase's pair walk (R12, #781): the inner instances of
//! an outer instance's turn come from the broadphase grid instead of a scan of every instance,
//! with the same pairs in the same order.
//!
//! The pair walk (`events::collision_phase_with`) fixes its inner list when a turn starts: every
//! other instance that is alive and active, of a rank at least the outer one's, whose classes
//! meet one side's handlers, by (rank, id), each tested for overlap when it is reached. Only
//! the members that overlap the outer instance at that moment do anything; an overlap test that
//! fails has no effect. So the walk can skip every member whose box cannot touch the outer one,
//! as long as it:
//!
//! - judges membership and order on the turn's starting state. [`Members`] keeps rank, classes,
//!   handler classes and alive-and-active for every instance as of the last turn start; it is
//!   built once per phase and, at each turn start, updated from the instances the grid has
//!   marked moved (every change to an instance goes through `World::get_mut` or
//!   `World::insert`, which mark it), before the grid is flushed;
//! - asks the grid again after every overlapping pair, from the outer instance's current box,
//!   because the handlers that ran may have moved either side ([`near`]). Moved instances are
//!   always candidates (grid contract), so nothing a handler moved is missed.
//!
//! Errors surface where the full scan would raise them: an instance whose box cannot be
//! computed sits in the grid's overflow set, which every gather returns, and an outer instance
//! whose own box fails gathers every instance.

use std::collections::BTreeSet;

use super::bbox::instance_box;
use crate::def::{ClassMask, GameDef};
use crate::events::event_set;
use crate::world::{Ctx, InstId, Instance, World};

/// What the pair walk reads of one instance, as of the last turn start.
#[derive(Clone, Copy, Debug)]
struct Entry {
    rank: u16,
    classes: ClassMask,
    /// Every handler class of its kind.
    handles: ClassMask,
    /// Alive and active.
    ready: bool,
}

impl Entry {
    fn of(def: &GameDef, inst: &Instance) -> Entry {
        let handles = event_set(def, inst.kind)
            .collisions
            .iter()
            .fold(ClassMask::NONE, |m, c| m | *c);
        Entry {
            rank: inst.rank,
            classes: inst.classes,
            handles,
            ready: inst.alive && inst.active,
        }
    }
}

/// [`Entry`] per instance id, as of the last turn start (module docs).
#[derive(Clone, Debug, Default)]
pub(crate) struct Members {
    /// Sorted by id.
    entries: Vec<(InstId, Entry)>,
}

impl Members {
    /// Every instance of `w` as it is now.
    pub(crate) fn of(w: &World, def: &GameDef) -> Members {
        Members {
            entries: w.instances().iter().map(|i| (i.id, Entry::of(def, i))).collect(),
        }
    }

    /// Re-reads the instances `ids` (those marked moved since the last update). An id no longer
    /// stored keeps its entry, marked not ready.
    pub(crate) fn update(&mut self, w: &World, def: &GameDef, ids: impl IntoIterator<Item = InstId>) {
        for id in ids {
            let found = self.entries.binary_search_by_key(&id, |(i, _)| *i);
            let entry = match w.get(id) {
                Some(inst) => Entry::of(def, inst),
                None => match found {
                    Ok(at) => match self.entries.get_mut(at) {
                        Some((_, e)) => Entry { ready: false, ..*e },
                        None => continue,
                    },
                    Err(_) => continue,
                },
            };
            match found {
                Ok(at) => {
                    if let Some(slot) = self.entries.get_mut(at) {
                        slot.1 = entry;
                    }
                }
                Err(at) => self.entries.insert(at, (id, entry)),
            }
        }
    }

    /// The (rank, id) key of `b` in the inner list of an outer instance `a` of rank `rank`, with
    /// classes `classes` and handler classes `mine`; `None` when `b` is not in it.
    pub(crate) fn key(
        &self,
        a: InstId,
        rank: u16,
        classes: ClassMask,
        mine: ClassMask,
        b: InstId,
    ) -> Option<(u16, InstId)> {
        if b == a {
            return None;
        }
        let at = self.entries.binary_search_by_key(&b, |(i, _)| *i).ok()?;
        let (_, e) = self.entries.get(at)?;
        (e.ready && e.rank >= rank && (e.classes.intersects(mine) || classes.intersects(e.handles)))
            .then_some((e.rank, b))
    }
}

/// Ids, sorted, of every instance that may overlap `a` now (with the engine's overlap test),
/// plus every instance whose overlap test with `a` could fail: the grid's cells under `a`'s
/// collision box, its moved and overflow instances. Every instance when the grid is not built
/// or `a`'s box cannot be computed; only the moved and overflow ones when `a` has no box.
pub(crate) fn near(ctx: &Ctx, a: InstId) -> Vec<InstId> {
    let grid = &ctx.w.grid;
    let all = || ctx.w.instances().iter().map(|i| i.id).collect();
    let Some(inst) = ctx.w.get(a) else {
        return Vec::new();
    };
    if !grid.is_built() {
        return all();
    }
    let at = (inst.m.x(), inst.m.y());
    let mut ids: BTreeSet<InstId> = match instance_box(ctx.def, inst, at, &ctx.rules.collide) {
        Err(_) => return all(),
        Ok(None) => grid.overflow().clone(),
        Ok(Some(p)) => match grid.candidates(p.bbox.left, p.bbox.top, p.bbox.right, p.bbox.bottom) {
            Ok(v) => v.into_iter().collect(),
            Err(_) => return all(),
        },
    };
    ids.extend(grid.moved().iter().copied());
    ids.into_iter().collect()
}
