//! World and context API (docs/plans/sim-core.md 1.4).
//!
//! WP0 fixes the signatures and ships working versions of the plain store reads (`get`,
//! `get_mut`, `exists`, `iter_alive`, `count_of`, `instances`) and of `Ctx::halt`; every
//! rule-dependent body is a stub that names its owner. WP1 owns the file.
#![allow(unused_variables, reason = "WP0 stubs: WP1 fills the bodies")]

use super::{Ctx, InstId, Instance, SimStatus, World};
use crate::def::{ClassMask, EngineKind, GroupId, KindId, NameId, Params, VariantSel};
use crate::error::SimError;
use crate::num::Real;
use crate::output::OutputEvent;
use crate::room::RoomRequest;
use crate::rules::TickRules;

/// What a handler tells the dispatcher.
///
/// Handler entry points (`kid::on_event`, `builtins::on_event` and each built-in's `on_event`,
/// `script::on_event`, kid actions) return `Result<Flow, SimError>`, so they can use `?` on
/// setters and queries. `events::dispatch` turns an `Err` into [`Ctx::halt`], which returns
/// [`Flow::Halt`]. Calls that run events themselves (`Ctx::destroy`, `Ctx::change_kind`,
/// `events::dispatch`) return `Flow` and can return `Halt`; a handler that gets `Halt` back
/// returns it at once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flow {
    /// Go on with the phase.
    Continue,
    /// A room change was requested: no further events this tick (R8).
    AbortTick,
    /// The sim halted (`World::status` is `Halted`, set by [`Ctx::halt`]). Stop at once: no
    /// further events, no destroy sweep, no room change, no `tick += 1`; the world stays frozen
    /// where it stopped (plan 2.2). `tick::run_tick` returns as soon as a phase gives `Halt`.
    Halt,
}

impl Flow {
    /// True for [`Flow::Halt`].
    pub const fn is_halt(self) -> bool {
        matches!(self, Flow::Halt)
    }
}

/// How a new instance is set up.
///
/// `SpawnInit::default()` is a plain `instance_create`: runs Create, rank from
/// `GameDef::rank_for`, unit scale, angle 0, no name, no group, no params.
#[derive(Clone, PartialEq, Debug)]
pub struct SpawnInit {
    /// Params, applied before Create (instance creation code).
    pub params: Params,
    /// The variant selector that picks the rank (`GameDef::rank_for`).
    pub variant: Option<VariantSel>,
    /// A rank fixed in advance (room-file instances carry `PlacedInstance::rank`). `None` asks
    /// `GameDef::rank_for(kind, variant)`.
    pub rank: Option<u16>,
    /// `image_xscale` from the room file.
    pub xscale: Real,
    /// `image_yscale` from the room file.
    pub yscale: Real,
    /// `image_angle` from the room file.
    pub angle: Real,
    /// Instance name from the room file (triggers, scripts, probe `Named` targets).
    pub name: Option<NameId>,
    /// Instance group from the room file.
    pub group: Option<GroupId>,
    /// Run the Create event inside `spawn` (Inferred). False only for restored instances.
    pub run_create: bool,
}

impl Default for SpawnInit {
    /// A plain `instance_create`: Create runs. Only restored instances turn `run_create` off.
    fn default() -> SpawnInit {
        SpawnInit {
            params: Params::default(),
            variant: None,
            rank: None,
            xscale: Real::ONE,
            yscale: Real::ONE,
            angle: Real::ZERO,
            name: None,
            group: None,
            run_create: true,
        }
    }
}

impl World {
    /// Every stored instance, alive or destroyed this tick, in creation order (sorted by id).
    /// Read-only: mutable access goes through [`World::get_mut`], which keeps the broadphase
    /// grid in step.
    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    /// The instance with this id, alive or destroyed this tick.
    pub fn get(&self, id: InstId) -> Option<&Instance> {
        self.instances.get(self.index_of(id)?)
    }

    /// The instance with this id, alive or destroyed this tick, for changing it.
    ///
    /// Marks the instance as moved in the broadphase grid (`Grid::mark_moved`), because the
    /// caller may change its position, mask, sprite, image index, scale or angle through `m` or
    /// `img`. Queries treat moved instances as candidates everywhere until the grid re-buckets
    /// them, so a stale grid never hides an instance (plan 3.4).
    pub fn get_mut(&mut self, id: InstId) -> Option<&mut Instance> {
        let i = self.index_of(id)?;
        self.grid.mark_moved(id);
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

    /// GML `instance_destroy`: runs Destroy now, then marks the instance dead. `Flow::Halt` when
    /// the Destroy handler halted the sim.
    pub fn destroy(&mut self, id: InstId) -> Flow {
        unimplemented!("WP1: Ctx::destroy")
    }

    /// GML `instance_change`. `Flow::Halt` when an event it ran halted the sim.
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

    /// Halts the sim: sets `World::status` to `Halted(e)` (the first error wins if it is
    /// already halted) and returns [`Flow::Halt`]. The one way a handler that returns `Flow`
    /// stops the sim (plan 2.2, 10.2); handlers that return `Result` just return the error and
    /// `events::dispatch` calls this.
    pub fn halt(&mut self, e: SimError) -> Flow {
        if self.w.status == SimStatus::Running {
            self.w.status = SimStatus::Halted(e);
        }
        Flow::Halt
    }
}
