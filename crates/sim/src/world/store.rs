//! The minimal instance store: a `Vec` sorted by id (docs/architecture/sim/world.md).
//!
//! No map: runtime ids are handed out in increasing order and appended, so the `Vec` stays
//! sorted by id and lookup is a binary search. Room-placed instances with a GM8 id (`gm_id`,
//! #669) are below every runtime id and go in at their sorted place, so for them id order is
//! not creation order (CLONE_SPEC, Instances, "Placed instance ids"). WP0 ships this working
//! version; WP1 owns it.

use std::collections::{BTreeMap, BTreeSet};

use super::{InstId, Instance, World};
use crate::collide::grid::Grid;
use crate::def::GameDef;
use crate::error::SimError;
use crate::rules::CollideRules;

/// Most live instances a world holds (decision 0001, host caps; the room format caps a room's
/// own instances at the same number). Spawning past it halts the sim.
pub const MAX_LIVE_INSTANCES: usize = 50_000;

impl World {
    /// Position of `id` in [`World::instances`], alive or dead-this-tick.
    pub fn index_of(&self, id: InstId) -> Option<usize> {
        self.instances.binary_search_by_key(&id, |i| i.id).ok()
    }

    /// The id the runtime counter starts at for `def` (CLONE_SPEC, Instances, "Placed instance
    /// ids"): `[engine].first_runtime_id` (the game file's last-instance-id + 1). A package that
    /// places GM8 ids without it starts one above the highest placed id, so runtime ids stay
    /// above every placed id; with neither, [`InstId::FIRST`].
    pub fn first_runtime_id(def: &GameDef) -> u64 {
        if let Some(first) = def.first_runtime_id {
            return u64::from(first);
        }
        def.rooms
            .iter()
            .flat_map(|r| r.instances.iter().filter_map(|p| p.gm_id))
            .max()
            .map_or(InstId::FIRST.0, |id| InstId::FIRST.0.max(u64::from(id) + 1))
    }

    /// Hands out the next instance id. Ids are never reused.
    pub fn alloc_id(&mut self) -> Result<InstId, SimError> {
        let id = InstId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| SimError::overflow("next_id"))?;
        Ok(id)
    }

    /// Stores an instance at its place in id order. A runtime id (from [`World::alloc_id`]) is
    /// above every stored id, so it is appended; a placed instance's GM8 id (#669) can go in
    /// before others. An id that is already stored is an error, and so is going past
    /// [`MAX_LIVE_INSTANCES`] live instances.
    pub fn insert(&mut self, inst: Instance) -> Result<(), SimError> {
        // `len` counts instances destroyed this tick, so the exact count runs only near the cap.
        if self.instances.len() >= MAX_LIVE_INSTANCES && self.iter_alive().count() >= MAX_LIVE_INSTANCES {
            return Err(SimError::unsupported(format!(
                "instance cap exceeded: a room holds at most {MAX_LIVE_INSTANCES} live instances"
            )));
        }
        let at = match self.instances.last() {
            Some(last) if last.id >= inst.id => self
                .instances
                .binary_search_by_key(&inst.id, |i| i.id)
                .err()
                .ok_or_else(|| {
                    SimError::unsupported(format!("instance id {} is already in use", inst.id.0))
                })?,
            _ => self.instances.len(),
        };
        // The GM instance list's room-entry sequence: every instance is appended when it enters
        // the room (research#142). The room change re-ranks the carried and placed instances
        // (World::set_room_entry); later creations keep this creation order.
        let id = inst.id;
        let rank = self.next_entry;
        self.next_entry = self
            .next_entry
            .checked_add(1)
            .ok_or_else(|| SimError::overflow("next_entry"))?;
        self.room_entry.insert(id, rank);
        self.grid.mark_moved(id);
        self.instances.insert(at, inst);
        Ok(())
    }

    /// The place of `id` in the room-entry sequence (research#142). `u64::MAX` when the id has no
    /// place, which only a world built before the sequence existed could have; it then sorts
    /// after every ranked instance, by id.
    pub(crate) fn entry_rank(&self, id: InstId) -> u64 {
        self.room_entry.get(&id).copied().unwrap_or(u64::MAX)
    }

    /// Starts a room entry: returns the previous room's entry sequence, which
    /// [`World::set_room_entry`] uses to keep the carried persistent instances in their old list
    /// order (research#142). The new room's instances are ranked from 0 as they are inserted.
    pub(crate) fn take_room_entry(&mut self) -> BTreeMap<InstId, u64> {
        self.next_entry = 0;
        std::mem::take(&mut self.room_entry)
    }

    /// Fixes the room entry sequence after the new room's placed instances exist (room change
    /// step 4): `placed` in room-file order, then the ids `previous` still stores — the carried
    /// persistent instances — in their old list order, then everything created during entry in
    /// creation order (research#142).
    pub(crate) fn set_room_entry(&mut self, placed: &[InstId], previous: &BTreeMap<InstId, u64>) {
        let mut entry = BTreeMap::new();
        let mut rank = 0u64;
        let push = |entry: &mut BTreeMap<InstId, u64>, rank: &mut u64, id: InstId| {
            entry.insert(id, *rank);
            *rank += 1;
        };
        for id in placed {
            if self.get(*id).is_some() {
                push(&mut entry, &mut rank, *id);
            }
        }
        let mut carried: Vec<(u64, InstId)> = previous
            .iter()
            .filter(|(id, _)| !entry.contains_key(id) && self.get(**id).is_some())
            .map(|(id, r)| (*r, *id))
            .collect();
        carried.sort_unstable();
        for (_, id) in carried {
            push(&mut entry, &mut rank, id);
        }
        let mut created: Vec<(u64, InstId)> = self
            .room_entry
            .iter()
            .filter(|(id, _)| !entry.contains_key(id))
            .map(|(id, r)| (*r, *id))
            .collect();
        created.sort_unstable();
        for (_, id) in created {
            push(&mut entry, &mut rank, id);
        }
        self.room_entry = entry;
        self.next_entry = rank;
    }

    /// Re-buckets the instances `get_mut` and `insert` marked as moved (`Grid::flush`, plan
    /// 3.4). Queries are correct without it; it only keeps them fast.
    pub fn flush_grid(&mut self, def: &GameDef, rules: &CollideRules) -> Result<(), SimError> {
        self.grid.flush(&self.instances, def, rules)
    }

    /// Builds the broadphase grid if it is not built (it starts unbuilt on room entry and after
    /// a savestate restore), else re-buckets the moved instances. Either way the grid then
    /// holds every instance at its current box and nothing is marked moved.
    pub fn ensure_grid(&mut self, def: &GameDef, rules: &CollideRules) -> Result<(), SimError> {
        if self.grid.is_built() {
            return self.flush_grid(def, rules);
        }
        self.grid = Grid::rebuild(self, def, rules)?;
        Ok(())
    }

    /// Removes dead instances, keeping the order of the rest (tick row 14), and forgets their
    /// room-entry rank.
    pub fn sweep(&mut self) {
        self.instances.retain(|i| i.alive);
        let alive: BTreeSet<InstId> = self.instances.iter().map(|i| i.id).collect();
        self.room_entry.retain(|id, _| alive.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use crate::collide::grid::Grid;
    use crate::def::{KindId, RoomId};
    use crate::error::SimError;
    use crate::num::Real;
    use crate::world::{Ctx, Flow, InstId, Instance, SimStatus, SpawnInit, World};

    fn spawn(w: &mut World, kind: u32) -> InstId {
        let id = w.alloc_id().unwrap();
        w.insert(Instance::new(id, KindId(kind), 0, Real::ZERO, Real::ZERO))
            .unwrap();
        id
    }

    #[test]
    fn ids_are_monotonic_and_lookup_finds_them() {
        let mut w = World::new(RoomId(0), 1);
        let a = spawn(&mut w, 0);
        let b = spawn(&mut w, 1);
        assert_eq!(a, InstId::FIRST);
        assert_eq!(b, InstId(100_002));
        assert_eq!(w.index_of(b), Some(1));
        assert_eq!(w.get(a).map(|i| i.kind), Some(KindId(0)));
        assert!(w.get(InstId(5)).is_none());
        let stale = Instance::new(a, KindId(0), 0, Real::ZERO, Real::ZERO);
        assert!(w.insert(stale).is_err(), "an id in use is refused");
    }

    /// The GM instance list order (research#142): depth largest first, then room entry — the
    /// room's placed instances in file order, the carried persistent ones in their old order,
    /// then the instances created during the room.
    #[test]
    fn the_instance_list_is_depth_then_room_entry() {
        use std::collections::BTreeMap;

        use crate::def::ClassMask;
        use crate::rules::TickRules;

        let mut w = World::new(RoomId(0), 1);
        // The first room: a persistent instance created at boot, then the room's placed one.
        let carried = w.alloc_id().unwrap();
        let mut boot = Instance::new(carried, KindId(0), 0, Real::ZERO, Real::ZERO);
        boot.persistent = true;
        w.insert(boot).unwrap();
        let first_placed = w.alloc_id().unwrap();
        w.insert(Instance::new(first_placed, KindId(1), 0, Real::ZERO, Real::ZERO))
            .unwrap();
        w.set_room_entry(&[first_placed], &BTreeMap::new());
        assert_eq!(w.instance_list(), [first_placed, carried]);

        // Room change: everything is marked dead and swept, the persistent instance is carried
        // into the new room and re-inserted (before the placed ones, as room.rs does), then the
        // room places one instance at another depth and one at the first one's, and creates one.
        let previous = w.take_room_entry();
        let carried_inst = w.get(carried).unwrap().clone();
        w.get_mut(first_placed).unwrap().alive = false;
        w.get_mut(carried).unwrap().alive = false;
        w.sweep();
        w.insert(carried_inst.clone()).unwrap();
        let placed_deep = w.alloc_id().unwrap();
        let mut deep = Instance::new(placed_deep, KindId(2), 0, Real::ZERO, Real::ZERO);
        deep.depth = Real::new(5.0);
        w.insert(deep).unwrap();
        let placed_flat = w.alloc_id().unwrap();
        let mut flat = Instance::new(placed_flat, KindId(3), 0, Real::ZERO, Real::ZERO);
        flat.classes = ClassMask::BLOCK;
        w.insert(flat).unwrap();
        let created = w.alloc_id().unwrap();
        w.insert(Instance::new(created, KindId(4), 0, Real::ZERO, Real::ZERO))
            .unwrap();
        w.set_room_entry(&[placed_deep, placed_flat], &previous);

        assert_eq!(
            w.instance_list(),
            [placed_deep, placed_flat, carried, created],
            "depth first, then placed in file order, the carried persistent one, then creations"
        );

        // `first_of` follows the list: the placed instance, although the carried one has the
        // lowest id and the dispatch order may rank it first.
        w.get_mut(carried).unwrap().classes = ClassMask::BLOCK;
        let rules = TickRules::default();
        assert_eq!(w.first_of(ClassMask::BLOCK, &rules), Some(placed_flat));
    }

    #[test]
    fn a_lower_id_goes_in_at_its_sorted_place() {
        let mut w = World::new(RoomId(0), 1);
        w.next_id = 105_106;
        let runtime = spawn(&mut w, 0);
        for id in [104_190, 104_180] {
            w.insert(Instance::new(InstId(id), KindId(1), 0, Real::ZERO, Real::ZERO))
                .unwrap();
        }
        let ids: Vec<u64> = w.instances.iter().map(|i| i.id.0).collect();
        assert_eq!(ids, [104_180, 104_190, 105_106]);
        assert_eq!(w.index_of(runtime), Some(2));
        assert_eq!(
            spawn(&mut w, 0),
            InstId(105_107),
            "placed ids do not move the counter"
        );
        let again = Instance::new(InstId(104_180), KindId(1), 0, Real::ZERO, Real::ZERO);
        let err = w.insert(again).unwrap_err();
        assert!(err.to_string().contains("104180 is already in use"), "{err}");
    }

    #[test]
    fn the_live_instance_cap_halts_a_spawn() {
        use super::MAX_LIVE_INSTANCES;
        let mut w = World::new(RoomId(0), 1);
        let ids: Vec<InstId> = (0..MAX_LIVE_INSTANCES).map(|_| spawn(&mut w, 0)).collect();
        let id = w.alloc_id().unwrap();
        let err = w
            .insert(Instance::new(id, KindId(0), 0, Real::ZERO, Real::ZERO))
            .unwrap_err();
        assert!(err.to_string().contains("50000 live instances"), "{err}");
        // Instances destroyed this tick do not count, even before the sweep.
        w.get_mut(ids[0]).unwrap().alive = false;
        spawn(&mut w, 0);
        assert_eq!(w.iter_alive().count(), MAX_LIVE_INSTANCES);
    }

    #[test]
    fn sweep_keeps_order_and_dead_stay_until_swept() {
        let mut w = World::new(RoomId(0), 1);
        let ids: Vec<InstId> = (0..5).map(|k| spawn(&mut w, k)).collect();
        for id in [ids[1], ids[3]] {
            w.get_mut(id).unwrap().alive = false;
        }
        assert!(w.get(ids[1]).is_some(), "dead-this-tick is still stored");
        assert!(!w.exists(ids[1]));
        assert_eq!(w.iter_alive().count(), 3);
        w.sweep();
        let left: Vec<InstId> = w.instances.iter().map(|i| i.id).collect();
        assert_eq!(left, vec![ids[0], ids[2], ids[4]]);
        assert_eq!(spawn(&mut w, 9), InstId(100_006), "ids are never reused");
    }

    #[test]
    fn mutable_access_marks_the_grid() {
        let mut w = World::new(RoomId(0), 1);
        let a = spawn(&mut w, 0);
        assert!(w.grid.moved().contains(&a), "insert marks the new instance");
        w.grid = Grid::default();
        assert!(w.get(a).is_some());
        assert!(w.grid.moved().is_empty(), "reads do not mark");
        w.get_mut(a).unwrap().solid = true;
        assert!(w.grid.moved().contains(&a), "get_mut marks");
        let mut other = w.clone();
        other.grid = Grid::default();
        assert_eq!(w, other, "the grid never makes worlds differ");
    }

    #[test]
    fn spawn_init_default_runs_create() {
        let init = SpawnInit::default();
        assert!(init.run_create);
        assert_eq!(
            (init.xscale, init.yscale, init.angle),
            (Real::ONE, Real::ONE, Real::ZERO)
        );
        assert_eq!((init.rank, init.name, init.group), (None, None, None));
    }

    #[test]
    fn halt_sets_status_once_and_returns_halt() {
        let mut b = crate::def::testing::GameDefBuilder::new("t");
        b.room(crate::def::RoomDef::new("r", 64, 64));
        let def = b.build().unwrap();
        let rules = crate::rules::RunnerRules::default();
        let scripts = crate::script::ScriptRuntime::default();
        let mut w = World::new(RoomId(0), 1);
        let mut ctx = Ctx {
            def: &def,
            rules: &rules,
            w: &mut w,
            scripts: &scripts,
            dll_mode: crate::sound::DllMode::Stub,
        };
        assert_eq!(ctx.halt(SimError::non_finite("hspeed")), Flow::Halt);
        assert_eq!(ctx.halt(SimError::overflow("tick")), Flow::Halt);
        assert_eq!(w.status, SimStatus::Halted(SimError::non_finite("hspeed")));
    }

    #[test]
    fn next_id_overflow_is_an_error() {
        let mut w = World::new(RoomId(0), 1);
        w.next_id = u64::MAX;
        assert!(w.alloc_id().is_err());
    }
}
