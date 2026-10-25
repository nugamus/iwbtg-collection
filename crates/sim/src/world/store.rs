//! The minimal instance store: a `Vec` in creation order (docs/plans/sim-core.md 1.2).
//!
//! No map: ids are handed out in increasing order and appended, so the `Vec` stays sorted by id
//! and lookup is a binary search. WP0 ships this working version; WP1 owns it.

use super::{InstId, Instance, World};
use crate::error::SimError;

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
    /// [`World::alloc_id`] are), which keeps the store sorted; anything else is an error.
    pub fn insert(&mut self, inst: Instance) -> Result<(), SimError> {
        if let Some(last) = self.instances.last()
            && last.id >= inst.id
        {
            return Err(SimError::unsupported(format!(
                "instance id {} inserted after {}",
                inst.id.0, last.id.0
            )));
        }
        self.instances.push(inst);
        Ok(())
    }

    /// Removes dead instances, keeping the order of the rest (tick row 14).
    pub fn sweep(&mut self) {
        self.instances.retain(|i| i.alive);
    }
}

#[cfg(test)]
mod tests {
    use crate::def::{KindId, RoomId};
    use crate::num::Real;
    use crate::world::{InstId, Instance, World};

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
    fn next_id_overflow_is_an_error() {
        let mut w = World::new(RoomId(0), 1);
        w.next_id = u64::MAX;
        assert!(w.alloc_id().is_err());
    }
}
