//! The minimal instance store: a `Vec` in creation order (docs/plans/sim-core.md 1.2).
//!
//! No map: ids are handed out in increasing order and appended, so the `Vec` stays sorted by id
//! and lookup is a binary search. WP0 ships this working version; WP1 owns it.

use super::{InstId, Instance, World};
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

    /// Hands out the next instance id. Ids are never reused.
    pub fn alloc_id(&mut self) -> Result<InstId, SimError> {
        let id = InstId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| SimError::overflow("next_id"))?;
        Ok(id)
    }

    /// Appends an instance. Its id must be greater than every id already stored (ids from
    /// [`World::alloc_id`] are), which keeps the store sorted; anything else is an error. Past
    /// [`MAX_LIVE_INSTANCES`] live instances it is an error too.
    pub fn insert(&mut self, inst: Instance) -> Result<(), SimError> {
        // `len` counts instances destroyed this tick, so the exact count runs only near the cap.
        if self.instances.len() >= MAX_LIVE_INSTANCES && self.iter_alive().count() >= MAX_LIVE_INSTANCES {
            return Err(SimError::unsupported(format!(
                "instance cap exceeded: a room holds at most {MAX_LIVE_INSTANCES} live instances"
            )));
        }
        if let Some(last) = self.instances.last()
            && last.id >= inst.id
        {
            return Err(SimError::unsupported(format!(
                "instance id {} inserted after {}",
                inst.id.0, last.id.0
            )));
        }
        self.grid.mark_moved(inst.id);
        self.instances.push(inst);
        Ok(())
    }

    /// Re-buckets the instances `get_mut` and `insert` marked as moved (`Grid::flush`, plan
    /// 3.4). Queries are correct without it; it only keeps them fast.
    pub fn flush_grid(&mut self, def: &GameDef, rules: &CollideRules) -> Result<(), SimError> {
        self.grid.flush(&self.instances, def, rules)
    }

    /// Removes dead instances, keeping the order of the rest (tick row 14).
    pub fn sweep(&mut self) {
        self.instances.retain(|i| i.alive);
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
        assert!(w.insert(stale).is_err(), "out-of-order insert is refused");
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
