//! World and context API (docs/plans/sim-core.md 1.4).
//!
//! WP0 fixes the signatures and ships working versions of the plain store reads (`get`,
//! `get_mut`, `exists`, `iter_alive`, `count_of`); every rule-dependent body is a stub that
//! names its owner. WP1 owns the file.
#![allow(unused_variables, reason = "WP0 stubs: WP1 fills the bodies")]

use super::{Ctx, InstId, Instance, World};
use crate::def::{ClassMask, EngineKind, KindId, Params, VariantSel};
use crate::error::SimError;
use crate::num::Real;
use crate::output::OutputEvent;
use crate::room::RoomRequest;
use crate::rules::TickRules;

/// What a handler tells the dispatcher.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flow {
    /// Go on with the phase.
    Continue,
    /// A room change was requested: no further events this tick (R8).
    AbortTick,
}

/// How a new instance is set up.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct SpawnInit {
    /// Params, applied before Create (instance creation code).
    pub params: Params,
    /// The variant selector that picks the rank (`GameDef::rank_for`).
    pub variant: Option<VariantSel>,
    /// Run the Create event inside `spawn` (Inferred). False only for restored instances.
    pub run_create: bool,
}

impl World {
    /// The instance with this id, alive or destroyed this tick.
    pub fn get(&self, id: InstId) -> Option<&Instance> {
        self.instances.get(self.index_of(id)?)
    }

    /// The instance with this id, alive or destroyed this tick.
    pub fn get_mut(&mut self, id: InstId) -> Option<&mut Instance> {
        let i = self.index_of(id)?;
        self.instances.get_mut(i)
    }

    /// GML `instance_exists` on one id: alive and active.
    pub fn exists(&self, id: InstId) -> bool {
        self.get(id).is_some_and(|i| i.alive && i.active)
    }

    /// Alive instances (active or not) in creation order.
    pub fn iter_alive(&self) -> impl Iterator<Item = &Instance> {
        self.instances.iter().filter(|i| i.alive)
    }

    /// "The first" alive, active instance whose classes intersect `class`, by
    /// [`TickRules::first_rule`] (R21).
    pub fn first_of(&self, class: ClassMask, rules: &TickRules) -> Option<InstId> {
        unimplemented!("WP1: World::first_of")
    }

    /// GML `instance_number` on a class: alive, active instances whose classes intersect it.
    pub fn count_of(&self, class: ClassMask) -> u32 {
        let n = self
            .iter_alive()
            .filter(|i| i.active && i.classes.intersects(class))
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// The kid: `first_of(KID | KID_FLIPPED)`.
    pub fn kid(&self, rules: &TickRules) -> Option<InstId> {
        unimplemented!("WP1: World::kid")
    }
}

impl Ctx<'_> {
    /// GML `instance_create`: a new instance of `kind` at `(x, y)`. Runs Create now (Inferred).
    pub fn spawn(&mut self, kind: KindId, x: Real, y: Real, init: SpawnInit) -> Result<InstId, SimError> {
        unimplemented!("WP1: Ctx::spawn")
    }

    /// Spawns the kind the game uses for an engine role; `None` when the game has none.
    pub fn spawn_engine(
        &mut self,
        k: EngineKind,
        x: Real,
        y: Real,
        init: SpawnInit,
    ) -> Result<Option<InstId>, SimError> {
        unimplemented!("WP1: Ctx::spawn_engine")
    }

    /// GML `instance_destroy`: runs Destroy now, then marks the instance dead.
    pub fn destroy(&mut self, id: InstId) -> Flow {
        unimplemented!("WP1: Ctx::destroy")
    }

    /// GML `instance_change`.
    pub fn change_kind(&mut self, id: InstId, kind: KindId, run_events: bool) -> Flow {
        unimplemented!("WP1: Ctx::change_kind")
    }

    /// GML `room_goto` and friends: deferred to tick row 15 (Confirmed); the last request wins
    /// (R13).
    pub fn request_room(&mut self, req: RoomRequest) {
        unimplemented!("WP1: Ctx::request_room")
    }

    /// Queues an event for the client.
    pub fn emit(&mut self, ev: OutputEvent) {
        unimplemented!("WP1: Ctx::emit")
    }
}
