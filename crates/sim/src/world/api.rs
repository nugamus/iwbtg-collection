//! World and context API (docs/plans/sim-core.md 1.4).
//!
//! WP0 fixed the signatures; WP1 owns the file. CLONE_SPEC rooms and tick loop, Confirmed
//! (recorder): `instance_create` runs Create at once, inside the call
//! (traces/showdown/tr-room-14.md); after `instance_destroy` the rest of the current event still
//! runs, the instance gets no later events (traces/rely-the-needle-4-2/tr-room-03-a-log.csv row
//! 8) and `instance_exists` stops counting it at once (traces/kaenbyou/tr-room-08.csv row 51).

use super::{Ctx, InstId, Instance, KindState, SimStatus, Value, World};
use crate::def::{Builtin, ClassMask, EngineKind, GameDef, GroupId, KindDef, KindId, NameId, ParamValue, Params, VariantSel};
use crate::error::SimError;
use crate::events::{self, EventKind, ProbeState};
use crate::num::Real;
use crate::output::OutputEvent;
use crate::room::RoomRequest;
use crate::rules::TickRules;
use crate::rules::tick::{ChangeKindProps, FirstRule};
use crate::{builtins, script};

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

    /// The stronger of two flows: `Halt` over `AbortTick` over `Continue`.
    pub const fn combine(self, other: Flow) -> Flow {
        match (self, other) {
            (Flow::Halt, _) | (_, Flow::Halt) => Flow::Halt,
            (Flow::AbortTick, _) | (_, Flow::AbortTick) => Flow::AbortTick,
            _ => Flow::Continue,
        }
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

    /// Like [`World::get_mut`] without marking the grid, for changes that cannot move the
    /// instance's box (alarms, timelines, variables, probe state).
    pub(crate) fn get_mut_unmoved(&mut self, id: InstId) -> Option<&mut Instance> {
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

    /// "The first" active instance whose classes intersect `class`, by
    /// [`TickRules::first_rule`] (R21). Instances destroyed this tick count only under
    /// `TickRules::dead_in_queries` (R22).
    pub fn first_of(&self, class: ClassMask, rules: &TickRules) -> Option<InstId> {
        let ids = self
            .instances
            .iter()
            .filter(|i| i.active && (i.alive || rules.dead_in_queries) && i.classes.intersects(class))
            .map(|i| i.id);
        self.first_among(ids, rules)
    }

    /// The first of `ids` by [`TickRules::first_rule`] (R21), for queries that found several
    /// candidates (WP3 sorts its results through this, so the grid never decides). Ids that name
    /// no stored instance are ignored.
    pub fn first_among(&self, ids: impl IntoIterator<Item = InstId>, rules: &TickRules) -> Option<InstId> {
        let found = ids.into_iter().filter_map(|id| self.get(id));
        match rules.first_rule {
            FirstRule::LowestId => found.map(|i| i.id).min(),
            FirstRule::DispatchOrder => found
                .min_by_key(|i| events::order_key(i, rules.dispatch, events::Order::Dispatch))
                .map(|i| i.id),
        }
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
        self.first_of(ClassMask::KID | ClassMask::KID_FLIPPED, rules)
    }
}

/// The kind `kind`, or an invariant error naming it.
fn kind_def(def: &GameDef, kind: KindId) -> Result<&KindDef, SimError> {
    def.kind(kind)
        .ok_or_else(|| SimError::invariant(format!("unknown kind {}", kind.0)))
}

/// `base` params overridden by `over` (a placed instance's params override the kind's).
fn merge_params(base: &Params, over: &Params) -> Params {
    let mut merged = base.clone();
    for (k, v) in &over.0 {
        merged.0.insert(k.clone(), v.clone());
    }
    merged
}

/// A param as a GML value: reals and text as they are, flags as 1 or 0, resource ids as their
/// index (GML resource ids are reals).
fn param_value(v: &ParamValue) -> Value {
    match v {
        ParamValue::Real(r) => Value::Real(*r),
        ParamValue::Bool(b) => Value::Real(if *b { Real::ONE } else { Real::ZERO }),
        ParamValue::Str(s) => Value::Str(s.clone()),
        ParamValue::Room(id) => Value::Real(Real::new(f64::from(id.0))),
        ParamValue::Kind(id) => Value::Real(Real::new(f64::from(id.0))),
        ParamValue::Sound(id) => Value::Real(Real::new(f64::from(id.0))),
    }
}

/// The per-owner state a new instance of `kd` starts with (plan 1.5).
fn initial_state(def: &GameDef, kd: &KindDef, params: &Params) -> Result<KindState, SimError> {
    Ok(match kd.base {
        Builtin::Kid => KindState::Kid(Box::default()),
        Builtin::Probe => KindState::Probe(ProbeState::from_params(params, def)),
        Builtin::Scripted => KindState::Script(script::ScriptState::default()),
        Builtin::Inert => KindState::None,
        b => KindState::Builtin(builtins::init_state(b, params).map_err(|e| {
            SimError::invariant(format!("kind {}: {e} (the loader checks params)", kd.name))
        })?),
    })
}

impl Ctx<'_> {
    /// GML `instance_create`: a new instance of `kind` at `(x, y)`. Runs Create now (Inferred;
    /// Confirmed (recorder)), so when this returns, Create has run.
    ///
    /// The instance takes the kind's sprite, mask, solid, visible, depth, persistent and
    /// classes, `init`'s scale, angle, name and group, and its rank from `init.rank` or
    /// `GameDef::rank_for`. Params (the kind's, overridden by `init.params`) build the
    /// built-in state and are stored as custom variables when the game knows their names
    /// (`GameDef::var_names`). An error from Create halts the sim and is returned.
    pub fn spawn(&mut self, kind: KindId, x: Real, y: Real, init: SpawnInit) -> Result<InstId, SimError> {
        let def = self.def;
        let kd = kind_def(def, kind)?;
        if !x.is_finite() || !y.is_finite() {
            return Err(SimError::non_finite("instance_create position"));
        }
        let rank = init
            .rank
            .or_else(|| def.rank_for(kind, init.variant.as_ref()))
            .ok_or_else(|| SimError::invariant(format!("kind {} has no rank (CR4)", kd.name)))?;
        let params = merge_params(&kd.params, &init.params);
        let state = initial_state(def, kd, &params)?;
        let id = self.w.alloc_id()?;
        let mut inst = Instance::new(id, kind, rank, x, y);
        inst.name = init.name;
        inst.group = init.group;
        inst.classes = kd.classes;
        inst.solid = kd.solid;
        inst.visible = kd.visible;
        inst.depth = kd.depth;
        inst.persistent = kd.persistent;
        inst.img.set_sprite(kd.sprite);
        inst.img.set_mask(kd.mask);
        inst.img.set_xscale(init.xscale)?;
        inst.img.set_yscale(init.yscale)?;
        inst.img.set_angle(init.angle)?;
        inst.state = state;
        for (name, v) in &params.0 {
            if let Some(var) = def.var_names.get(name) {
                inst.vars.insert(*var, param_value(v));
            }
        }
        self.w.insert(inst)?;
        if init.run_create && events::dispatch(self, id, &EventKind::Create).is_halt() {
            return Err(self.halted_error());
        }
        Ok(id)
    }

    /// Spawns the kind the game uses for an engine role; `None` when the game has none.
    pub fn spawn_engine(
        &mut self,
        k: EngineKind,
        x: Real,
        y: Real,
        init: SpawnInit,
    ) -> Result<Option<InstId>, SimError> {
        match self.def.engine_kind(k) {
            Some(kind) => self.spawn(kind, x, y, init).map(Some),
            None => Ok(None),
        }
    }

    /// GML `instance_destroy`: runs Destroy now (the instance still exists during it), then
    /// marks the instance dead; it runs no further events and the sweep at the end of the tick
    /// removes it. An instance that is already destroyed, deactivated or unknown is left alone
    /// (`Continue`). Returns the Destroy handler's flow; `Flow::Halt` when it halted the sim.
    pub fn destroy(&mut self, id: InstId) -> Flow {
        if !self.w.exists(id) {
            return Flow::Continue;
        }
        let flow = events::dispatch(self, id, &EventKind::Destroy);
        if flow.is_halt() {
            return flow;
        }
        if let Some(inst) = self.w.get_mut(id) {
            inst.alive = false;
        }
        flow
    }

    /// GML `instance_change`. With `run_events`, the old kind's Destroy runs first and the new
    /// kind's Create last. The instance keeps its id, position, motion, alarms and variables; it
    /// takes the new kind's rank, classes and fresh built-in state, plus the kind properties
    /// `TickRules::change_kind` names (not in the spec, Design). `Flow::Halt` when an event it
    /// ran halted the sim; an unknown kind or a kind without a rank halts it.
    pub fn change_kind(&mut self, id: InstId, kind: KindId, run_events: bool) -> Flow {
        if !self.w.exists(id) {
            return Flow::Continue;
        }
        let mut flow = Flow::Continue;
        if run_events {
            flow = events::dispatch(self, id, &EventKind::Destroy);
            if flow.is_halt() || !self.w.exists(id) {
                return flow;
            }
        }
        let def = self.def;
        let kd = match kind_def(def, kind) {
            Ok(kd) => kd,
            Err(e) => return self.halt(e),
        };
        let Some(rank) = def.rank_for(kind, None) else {
            return self.halt(SimError::invariant(format!("kind {} has no rank (CR4)", kd.name)));
        };
        let state = match initial_state(def, kd, &kd.params) {
            Ok(s) => s,
            Err(e) => return self.halt(e),
        };
        let props = self.rules.tick.change_kind;
        if let Some(inst) = self.w.get_mut(id) {
            inst.kind = kind;
            inst.rank = rank;
            inst.classes = kd.classes;
            inst.state = state;
            if props == ChangeKindProps::KindDefaults {
                inst.img.set_sprite(kd.sprite);
                inst.img.set_mask(kd.mask);
                inst.solid = kd.solid;
                inst.visible = kd.visible;
                inst.depth = kd.depth;
                inst.persistent = kd.persistent;
            }
        }
        if run_events {
            flow = flow.combine(events::dispatch(self, id, &EventKind::Create));
        }
        flow
    }

    /// GML `room_goto` and friends: deferred to tick row 15 (Confirmed); the last request wins
    /// (R13). The phase running the handler applies R8 when the handler returns.
    pub fn request_room(&mut self, req: RoomRequest) {
        self.w.pending = Some(req);
        self.w.room_requests = self.w.room_requests.saturating_add(1);
    }

    /// Queues an event for the client.
    pub fn emit(&mut self, ev: OutputEvent) {
        self.w.output.push(ev);
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

    /// The error the sim halted with, for calls that return `Result` after a `Flow::Halt`.
    pub(crate) fn halted_error(&self) -> SimError {
        match &self.w.status {
            SimStatus::Halted(e) => e.clone(),
            _ => SimError::invariant("a handler returned Halt without halting the sim"),
        }
    }
}
