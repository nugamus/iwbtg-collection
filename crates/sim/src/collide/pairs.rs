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
    fn of(w: &World, def: &GameDef, inst: &Instance) -> Entry {
        let handles = event_set(def, inst.kind)
            .collisions
            .iter()
            .fold(ClassMask::NONE, |m, c| m | *c);
        Entry {
            rank: inst.rank,
            classes: inst.classes,
            handles,
            ready: w.visible(inst.id, false),
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
            entries: w
                .instances()
                .iter()
                .map(|i| (i.id, Entry::of(w, def, i)))
                .collect(),
        }
    }

    /// Re-reads the instances `ids` (those marked moved since the last update). An id no longer
    /// stored keeps its entry, marked not ready.
    pub(crate) fn update(&mut self, w: &World, def: &GameDef, ids: impl IntoIterator<Item = InstId>) {
        for id in ids {
            let found = self.entries.binary_search_by_key(&id, |(i, _)| *i);
            let entry = match w.get(id) {
                Some(inst) => Entry::of(w, def, inst),
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

/// The static kind facts that let the pair walk skip a turn with no possible inner instance
/// (#1139): for each kind, its highest rank, its classes and the classes its collision handlers
/// accept. Kind ranks, classes and handlers never change at run time, so a kind with no rank
/// at or above an outer's and no class or handler that meets it cannot supply the outer's
/// `Members::key` a candidate. That is the common case for the 800 stuck `blood` drops of a
/// death: a blood handles `BLOCK` only, and no instance of rank greater than or equal to a
/// blood's has class `BLOCK` or handles `BLOOD`.
#[derive(Clone, Debug)]
pub(crate) struct Reach {
    /// Per rankable kind: highest rank, classes, and every handler class of its event set.
    kinds: Vec<(u16, ClassMask, ClassMask)>,
}

impl Reach {
    /// The facts of `def`'s kinds.
    pub(crate) fn of(def: &GameDef) -> Reach {
        let mut top: Vec<Option<u16>> = vec![None; def.kinds.len()];
        for entry in &def.ranks.entries {
            let Some(at) = entry.kind.index() else {
                continue;
            };
            if let Some(slot) = top.get_mut(at) {
                *slot = Some(slot.map_or(entry.rank, |r| r.max(entry.rank)));
            }
        }
        let kinds = def
            .kinds
            .iter()
            .enumerate()
            .filter_map(|(i, kind)| {
                // A kind with no rank entry can hold no instance (`World::spawn` refuses it),
                // so it can never be an inner; leaving it out keeps a package's unplaced
                // built-ins (a `platform`, say, which has class BLOCK) from disabling the
                // skip.
                let rank = top.get(i).copied().flatten()?;
                let handles = kind.events.collisions.iter().fold(ClassMask::NONE, |m, c| m | *c);
                Some((rank, kind.classes, handles))
            })
            .collect();
        Reach { kinds }
    }

    /// Whether some kind can be an inner of an outer of this rank, with these classes and
    /// handler classes, under [`Members::key`]'s test (a superset: kinds are checked, not live
    /// instances, and a kind's highest rank is used).
    pub(crate) fn may_meet(&self, rank: u16, classes: ClassMask, mine: ClassMask) -> bool {
        self.kinds.iter().any(|&(kind_rank, kind_classes, kind_handles)| {
            kind_rank >= rank && (kind_classes.intersects(mine) || kind_handles.intersects(classes))
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::testing::GameDefBuilder;
    use crate::def::{Builtin, KindDef, RoomDef};

    /// #1139: a turn is skippable only when no kind at or above its rank can meet it. A kind
    /// with no rank entry can hold no instance and must not keep a turn alive: a package's
    /// unplaced built-ins (whose classes and handlers would otherwise match) would disable the
    /// skip.
    #[test]
    fn a_turn_no_kind_can_meet_is_skippable() {
        let mut b = GameDefBuilder::new("reach");
        let mut low = KindDef::new("low", Builtin::Probe);
        low.classes = ClassMask::SAVE;
        low.events.collisions.push(ClassMask::PLATFORM);
        let low = b.kind(low);
        let mut high = KindDef::new("high", Builtin::Probe);
        high.classes = ClassMask::KID;
        high.events.collisions.push(ClassMask::BLOCK);
        let high = b.kind(high);
        // Left out of the order, so it has no rank entry: no instance of it can exist, and it
        // must not make any turn reachable (its KID class would answer `may_meet(2, ..., KID)`).
        let mut rankless = KindDef::new("rankless", Builtin::Probe);
        rankless.classes = ClassMask::KID;
        rankless.events.collisions.push(ClassMask::PLATFORM);
        b.kind(rankless);
        b.order(&[low, high]);
        b.room(RoomDef::new("room", 64, 64));
        let def = b.build().unwrap();
        let reach = Reach::of(&def);

        // low (rank 0, classes SAVE, handlers PLATFORM): high (rank 1) has neither a SAVE class
        // nor a PLATFORM handler.
        assert!(!reach.may_meet(0, ClassMask::SAVE, ClassMask::PLATFORM));
        // high's KID class answers an outer that handles KID.
        assert!(reach.may_meet(0, ClassMask::BLOCK, ClassMask::KID));
        // Only low and high have ranks; the rankless kind is ignored.
        assert!(!reach.may_meet(2, ClassMask::BLOCK, ClassMask::KID));
    }
}
