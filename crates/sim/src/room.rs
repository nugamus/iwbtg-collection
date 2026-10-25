//! Room entry and the room change sequence (docs/plans/sim-core.md 4.2).
//!
//! WP0 declares; WP4a owns.
//!
//! The sequence follows the manual's 7 steps (CLONE_SPEC rooms, "Room start sequence",
//! Confirmed):
//!
//! 1. Room End for every instance of the old room.
//! 2. Non-persistent instances are removed, with no Destroy event. Deactivated persistent
//!    instances count as non-persistent here: they stay behind (Confirmed, manual 403_06). When
//!    the old room is persistent, everything that stays behind is stored with it.
//! 3. Persistent instances move to the new room with their ids.
//! 4. A new or non-persistent room creates its placed instances in file order, each with its
//!    params (instance creation code) applied before its Create. A persistent room that was
//!    visited before gets its stored instances back, without Create (Inferred, R18).
//! 5. First room of the game only: Game Start for every instance.
//! 6. The room creation actions run, as data.
//! 7. Room Start for every instance in the room when this step begins (R10): the room's own
//!    instances, then the carried persistent ones (Confirmed (recorder), [`RoomStartOrder`]).
//!
//! On entry `room_speed` is the room file's speed (Inferred, Confirmed (recorder), R14), the
//! views are reset from the room's view definitions, and the broadphase grid starts empty (every
//! instance marked, so queries answer like brute force until the grid is rebuilt).
//!
//! A request raised during the sequence (room creation code, Create, Room Start) follows
//! [`RoomRules::nested_request`](crate::rules::RoomRules::nested_request) (R23): the sequence
//! always finishes all 7 steps, and the next change either follows at once (at most
//! `max_chained_changes` times, then `SimError`) or waits for tick row 15 of the next tick.
//! `Flow::AbortTick` from a handler inside the sequence does not stop the sequence; `Flow::Halt`
//! does.
//!
//! The sequence reaches the event and spawn layer only through [`RoomHost`]. [`EngineHost`] is
//! the real one (`events::dispatch`, `events::dispatch_order`, `Ctx::spawn`, `save::load_into`);
//! tests pass a recording host, so the sequence is testable before those land.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::collide::Target;
use crate::collide::grid::Grid;
use crate::def::{KindId, RoomAction, RoomDef, RoomId, RoomRole};
use crate::error::SimError;
use crate::events::{self, EventKind};
use crate::num::Real;
use crate::output::OutputEvent;
use crate::rules::TickRules;
use crate::rules::room::{NestedRequest, RoomEndOrder, RoomStartOrder};
use crate::save::{self, SaveSlot};
use crate::view::{View, ViewPort};
use crate::world::{Ctx, EngineGlobals, Flow, InstId, Instance, SimStatus, SpawnInit, World};

/// A deferred room change (tick row 15).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RoomRequest {
    /// `room_goto`.
    Goto(RoomId),
    /// `room_goto_next`.
    Next,
    /// `room_goto_previous`.
    Previous,
    /// `room_restart`.
    Restart,
    /// `game_restart`.
    GameRestart,
    /// `game_end`.
    GameEnd,
}

/// Where a run starts.
#[derive(Clone, PartialEq, Debug)]
pub enum StartPoint {
    /// What warpStart does: delete the save, set the difficulty, go to `GameDef::new_game_room`.
    NewGame {
        /// Difficulty index.
        difficulty: u8,
    },
    /// Trace cases: the recorder's `--room` jump.
    Room {
        /// The room.
        room: RoomId,
        /// Difficulty index.
        difficulty: u8,
    },
    /// From a save slot (practice, route testing).
    FromSave(SaveSlot),
}

/// Room changes and transitions for the client. Visual only: the sim never reads them back.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RoomOutput {
    /// A room was entered (after room change step 3, before any Create of the new room).
    Entered {
        /// The room left, or `None` for the first room of the game (also after game_restart).
        from: Option<RoomId>,
        /// The room entered.
        to: RoomId,
    },
    /// A built-in set `transition_kind` and `transition_steps` for the next room change. The
    /// client plays it at the next [`RoomOutput::Entered`] and then forgets it (the kind resets
    /// to 0 after one change, Confirmed). Transitions spend no ticks (Inferred, R17b).
    Transition {
        /// GM `transition_kind` (21 is "fade out and in").
        kind: u8,
        /// GM `transition_steps`.
        steps: u32,
    },
    /// `game_restart` ran: every instance and persistent room state is gone.
    GameRestarted,
    /// `game_end` ran: the sim status is now `Ended`.
    GameEnded,
}

/// The event and spawn layer the room sequence runs on. [`EngineHost`] is the real one; tests
/// use a recording host.
pub trait RoomHost {
    /// The instances a phase visits, in dispatch order (`events::dispatch_order`, R2).
    fn dispatch_order(
        &mut self,
        w: &World,
        rules: &TickRules,
        filter: &dyn Fn(&Instance) -> bool,
    ) -> Vec<InstId>;

    /// Runs one event on one instance (`events::dispatch`).
    fn dispatch(&mut self, ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow;

    /// Creates an instance and runs its Create unless `init.run_create` is off (`Ctx::spawn`).
    fn spawn(
        &mut self,
        ctx: &mut Ctx,
        kind: KindId,
        x: Real,
        y: Real,
        init: SpawnInit,
    ) -> Result<InstId, SimError>;

    /// Loads a save slot: creates the kid at the saved spot (`save::load_into`).
    fn load_save(&mut self, ctx: &mut Ctx, slot: &SaveSlot) -> Result<InstId, SimError>;
}

/// The real event and spawn layer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct EngineHost;

impl RoomHost for EngineHost {
    fn dispatch_order(
        &mut self,
        w: &World,
        rules: &TickRules,
        filter: &dyn Fn(&Instance) -> bool,
    ) -> Vec<InstId> {
        events::dispatch_order(w, rules, filter)
    }

    fn dispatch(&mut self, ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow {
        events::dispatch(ctx, me, ev)
    }

    fn spawn(
        &mut self,
        ctx: &mut Ctx,
        kind: KindId,
        x: Real,
        y: Real,
        init: SpawnInit,
    ) -> Result<InstId, SimError> {
        ctx.spawn(kind, x, y, init)
    }

    fn load_save(&mut self, ctx: &mut Ctx, slot: &SaveSlot) -> Result<InstId, SimError> {
        save::load_into(ctx, slot)
    }
}

/// Creates the boot instances, enters the first room, runs Game Start.
///
/// `GameDef::boot` kinds are created first, as persistent instances (the `world` built-in
/// stands in for rInit, Design). Then the start room is entered as the first room of the game:
/// steps 2 to 7, with Game Start. `NewGame` deletes the save, sets the difficulty and enters
/// `GameDef::new_game_room` (what warpStart does). `Room` sets the difficulty and enters the
/// room (the recorder's `--room` jump). `FromSave` stores the slot, sets the difficulty, loads
/// it (the kid is created before the room, so he is carried in) and enters the saved room.
///
/// An error halts the sim (`SimStatus::Halted`) and is returned.
pub fn start_game(ctx: &mut Ctx, start: &StartPoint) -> Result<(), SimError> {
    start_game_with(ctx, start, &mut EngineHost)
}

/// The manual's 7-step room change (Confirmed), plus nested requests (R23).
///
/// Called only by tick row 15 (WP1), which passes the pending request; handlers never change
/// rooms themselves, they call `Ctx::request_room`. `World::pending` is cleared first, so any
/// request found afterwards was raised during the sequence. A halted sim returns its error and
/// does nothing; an ended sim does nothing. An error halts the sim and is returned.
pub fn perform_change(ctx: &mut Ctx, req: RoomRequest) -> Result<(), SimError> {
    perform_change_with(ctx, req, &mut EngineHost)
}

/// [`start_game`] on an explicit host.
pub fn start_game_with<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    start: &StartPoint,
    host: &mut H,
) -> Result<(), SimError> {
    let result = start_inner(ctx, start, host);
    halt_on_err(ctx, result)
}

/// [`perform_change`] on an explicit host.
pub fn perform_change_with<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    req: RoomRequest,
    host: &mut H,
) -> Result<(), SimError> {
    let result = change_inner(ctx, req, host);
    halt_on_err(ctx, result)
}

/// Halts the sim on an error (the first error wins) and passes the result on.
fn halt_on_err(ctx: &mut Ctx, result: Result<(), SimError>) -> Result<(), SimError> {
    if let Err(e) = &result {
        ctx.halt(e.clone());
    }
    result
}

/// `Err` with the halting error when the sim is halted.
fn stop_if_halted(ctx: &Ctx) -> Result<(), SimError> {
    match &ctx.w.status {
        SimStatus::Halted(e) => Err(e.clone()),
        SimStatus::Running | SimStatus::Ended => Ok(()),
    }
}

/// The error behind a `Flow::Halt`.
fn halted_error(ctx: &Ctx) -> SimError {
    match &ctx.w.status {
        SimStatus::Halted(e) => e.clone(),
        SimStatus::Running | SimStatus::Ended => {
            SimError::invariant("a handler returned Flow::Halt but the world is not halted")
        }
    }
}

fn start_inner<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    start: &StartPoint,
    host: &mut H,
) -> Result<(), SimError> {
    stop_if_halted(ctx)?;
    ctx.w.pending = None;
    create_boot(ctx, host)?;
    let target = match start {
        StartPoint::NewGame { difficulty } => {
            ctx.w.save = None;
            ctx.w.engine.run.difficulty = *difficulty;
            ctx.def.new_game_room
        }
        StartPoint::Room { room, difficulty } => {
            ctx.w.engine.run.difficulty = *difficulty;
            *room
        }
        StartPoint::FromSave(slot) => {
            ctx.w.save = Some(slot.clone());
            ctx.w.engine.run.difficulty = slot.difficulty;
            host.load_save(ctx, slot)?;
            stop_if_halted(ctx)?;
            // The saved room is entered right below; a request the load made for it is that
            // same entry.
            ctx.w.pending = None;
            slot.room
        }
    };
    room_def(ctx, target)?;
    enter(ctx, host, None, target)?;
    after_sequence(ctx, host)
}

fn change_inner<H: RoomHost + ?Sized>(ctx: &mut Ctx, req: RoomRequest, host: &mut H) -> Result<(), SimError> {
    stop_if_halted(ctx)?;
    if ctx.w.status == SimStatus::Ended {
        return Ok(());
    }
    ctx.w.pending = None;
    run_request(ctx, host, req)?;
    after_sequence(ctx, host)
}

/// R23: a request raised during the sequence. `ChainAfterSequence` performs it now (bounded);
/// `NextTick` leaves it pending for tick row 15 of the next tick.
fn after_sequence<H: RoomHost + ?Sized>(ctx: &mut Ctx, host: &mut H) -> Result<(), SimError> {
    if ctx.rules.room.nested_request == NestedRequest::NextTick {
        return Ok(());
    }
    let max = ctx.rules.room.max_chained_changes;
    let mut chained: u8 = 0;
    while ctx.w.status == SimStatus::Running {
        let Some(req) = ctx.w.pending.take() else {
            break;
        };
        if chained >= max {
            return Err(SimError::unsupported(format!(
                "more than {max} chained room changes in one tick (R23); last request {req:?}"
            )));
        }
        chained = chained.saturating_add(1);
        run_request(ctx, host, req)?;
    }
    Ok(())
}

/// Runs one request: a room change, game_restart or game_end.
fn run_request<H: RoomHost + ?Sized>(ctx: &mut Ctx, host: &mut H, req: RoomRequest) -> Result<(), SimError> {
    match req {
        RoomRequest::GameEnd => {
            ctx.w.status = SimStatus::Ended;
            emit(ctx, RoomOutput::GameEnded);
            Ok(())
        }
        RoomRequest::GameRestart => game_restart(ctx, host),
        RoomRequest::Goto(_) | RoomRequest::Next | RoomRequest::Previous | RoomRequest::Restart => {
            let target = resolve(ctx, req)?;
            let from = ctx.w.room;
            room_end(ctx, host)?;
            enter(ctx, host, Some(from), target)
        }
    }
}

/// The room a request goes to.
fn resolve(ctx: &Ctx, req: RoomRequest) -> Result<RoomId, SimError> {
    let current = ctx.w.room;
    let target = match req {
        RoomRequest::Goto(room) => room,
        RoomRequest::Next => current
            .0
            .checked_add(1)
            .map(RoomId)
            .filter(|r| ctx.def.room(*r).is_some())
            .ok_or_else(|| SimError::Script {
                message: format!("room_goto_next: no room after room {}", current.0),
            })?,
        RoomRequest::Previous => current
            .0
            .checked_sub(1)
            .map(RoomId)
            .filter(|r| ctx.def.room(*r).is_some())
            .ok_or_else(|| SimError::Script {
                message: format!("room_goto_previous: no room before room {}", current.0),
            })?,
        RoomRequest::Restart => {
            if room_def(ctx, current)?.persistent {
                return Err(SimError::unsupported(
                    "room_restart of a persistent room (rooms UNKNOWN; unused by the 20 games)",
                ));
            }
            current
        }
        RoomRequest::GameRestart | RoomRequest::GameEnd => {
            return Err(SimError::invariant(format!("{req:?} is not a room change")));
        }
    };
    room_def(ctx, target)?;
    Ok(target)
}

/// The definition of `room`; an unknown id is an invariant break (the loader checks rooms).
fn room_def<'d>(ctx: &Ctx<'d>, room: RoomId) -> Result<&'d RoomDef, SimError> {
    let def = ctx.def;
    def.room(room)
        .ok_or_else(|| SimError::invariant(format!("room {} does not exist", room.0)))
}

/// Creates the `GameDef::boot` kinds at (0, 0), as persistent instances.
fn create_boot<H: RoomHost + ?Sized>(ctx: &mut Ctx, host: &mut H) -> Result<(), SimError> {
    let def = ctx.def;
    for &kind in &def.boot {
        let id = host.spawn(ctx, kind, Real::ZERO, Real::ZERO, SpawnInit::default())?;
        stop_if_halted(ctx)?;
        if let Some(inst) = ctx.w.get_mut(id) {
            inst.persistent = true;
        }
    }
    Ok(())
}

/// game_restart (Inferred; R16): optionally Room End, remove every instance (persistent ones
/// too) without events, forget the persistent rooms, clear the globals unless
/// `globals_survive_restart`, then boot again and enter the boot room as the first room. The
/// save (a file) and the sounds (Inferred, S4) are kept.
fn game_restart<H: RoomHost + ?Sized>(ctx: &mut Ctx, host: &mut H) -> Result<(), SimError> {
    if ctx.rules.room.restart_events {
        room_end(ctx, host)?;
    }
    let ids: Vec<InstId> = ctx.w.instances().iter().map(|i| i.id).collect();
    for id in ids {
        if let Some(inst) = ctx.w.get_mut(id) {
            inst.alive = false;
        }
    }
    ctx.w.sweep();
    ctx.w.grid = Grid::default();
    ctx.w.persistent_rooms.clear();
    ctx.w.pending = None;
    if !ctx.rules.room.globals_survive_restart {
        ctx.w.globals.clear();
        ctx.w.engine = EngineGlobals::default();
    }
    emit(ctx, RoomOutput::GameRestarted);
    create_boot(ctx, host)?;
    let def = ctx.def;
    let target = def
        .rooms
        .iter()
        .position(|r| r.role == RoomRole::Boot)
        .and_then(|i| u32::try_from(i).ok())
        .map_or(def.start_room, RoomId);
    room_def(ctx, target)?;
    enter(ctx, host, None, target)
}

/// Step 1: Room End for every alive, active instance that handles it.
fn room_end<H: RoomHost + ?Sized>(ctx: &mut Ctx, host: &mut H) -> Result<(), SimError> {
    let def = ctx.def;
    let handles = |i: &Instance| i.alive && i.active && def.kind(i.kind).is_some_and(|k| k.events.room_end);
    let order = match ctx.rules.room.room_end_order {
        RoomEndOrder::Dispatch => host.dispatch_order(ctx.w, &ctx.rules.tick, &handles),
        RoomEndOrder::Depth => {
            let mut v: Vec<(Real, InstId)> = ctx
                .w
                .instances()
                .iter()
                .filter(|i| handles(i))
                .map(|i| (i.depth, i.id))
                .collect();
            // Depth descending, then creation (id) order. Depth is never NaN (setters refuse
            // non-finite values), so `partial_cmp` always answers.
            v.sort_by(|a, b| {
                b.0.get()
                    .partial_cmp(&a.0.get())
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1.cmp(&b.1))
            });
            v.into_iter().map(|(_, id)| id).collect()
        }
    };
    run_event(ctx, host, &order, &EventKind::RoomEnd)
}

/// Dispatches `ev` to each instance of `order` that is still alive and active at its turn.
/// `AbortTick` does not stop the sequence; `Halt` does.
fn run_event<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    order: &[InstId],
    ev: &EventKind,
) -> Result<(), SimError> {
    for &id in order {
        if !ctx.w.exists(id) {
            continue;
        }
        if host.dispatch(ctx, id, ev) == Flow::Halt {
            return Err(halted_error(ctx));
        }
    }
    Ok(())
}

/// Steps 2 to 7: leave `from` (if any) and enter `to`. `from = None` is the first room of the
/// game, which gets Game Start.
fn enter<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    from: Option<RoomId>,
    to: RoomId,
) -> Result<(), SimError> {
    let room = room_def(ctx, to)?;

    // Steps 2 and 3: split into carried (persistent and active) and left behind.
    let (carried, left): (Vec<Instance>, Vec<Instance>) = ctx
        .w
        .instances()
        .iter()
        .filter(|i| i.alive)
        .cloned()
        .partition(|i| i.persistent && i.active);
    if let Some(old) = from
        && ctx.def.room(old).is_some_and(|r| r.persistent)
    {
        ctx.w.persistent_rooms.insert(old, left);
    }
    let restored = if room.persistent {
        ctx.w.persistent_rooms.remove(&to)
    } else {
        None
    };
    let carried_ids: BTreeSet<InstId> = carried.iter().map(|i| i.id).collect();

    // Rebuild the store: carried plus restored, sorted by id (both lists already are), so the
    // store stays in creation order. Every instance is re-inserted, which marks it in the new,
    // empty grid.
    let ids: Vec<InstId> = ctx.w.instances().iter().map(|i| i.id).collect();
    for id in ids {
        if let Some(inst) = ctx.w.get_mut(id) {
            inst.alive = false;
        }
    }
    ctx.w.sweep();
    ctx.w.grid = Grid::default();
    let restoring = restored.is_some();
    let mut merged = carried;
    merged.extend(restored.unwrap_or_default());
    merged.sort_by_key(|i| i.id);
    for inst in merged {
        ctx.w.insert(inst)?;
    }

    // Room state on entry (R14, views, output).
    ctx.w.room = to;
    ctx.w.room_tick = 0;
    ctx.w.room_speed = Real::new(f64::from(room.file_speed));
    ctx.w.views = views_of(room);
    emit(ctx, RoomOutput::Entered { from, to });

    // Step 4: placed instances in file order, params before Create (inside spawn).
    if !restoring {
        for p in &room.instances {
            let init = SpawnInit {
                params: p.params.clone(),
                variant: None,
                rank: Some(p.rank),
                xscale: p.xscale,
                yscale: p.yscale,
                angle: p.angle,
                name: p.name,
                group: p.group,
                run_create: true,
            };
            host.spawn(ctx, p.kind, p.x, p.y, init)?;
            stop_if_halted(ctx)?;
        }
    }

    // Step 5: Game Start, first room only. No kind declares it (no game in the 20 uses it),
    // so every instance gets it.
    if from.is_none() {
        let order = host.dispatch_order(ctx.w, &ctx.rules.tick, &|i: &Instance| i.alive && i.active);
        run_event(ctx, host, &order, &EventKind::GameStart)?;
    }

    // Step 6: room creation actions.
    for action in &room.on_create {
        run_action(ctx, host, action)?;
    }

    // Step 7: Room Start, snapshot taken now (R10).
    let def = ctx.def;
    let handles = |i: &Instance| i.alive && i.active && def.kind(i.kind).is_some_and(|k| k.events.room_start);
    let order: Vec<InstId> = match ctx.rules.room.room_start_order {
        RoomStartOrder::RoomThenCarried => {
            let in_room = ctx.w.instances().iter().filter(|i| handles(i));
            let (carried, own): (Vec<InstId>, Vec<InstId>) =
                in_room.map(|i| i.id).partition(|id| carried_ids.contains(id));
            own.into_iter().chain(carried).collect()
        }
        RoomStartOrder::Dispatch => host.dispatch_order(ctx.w, &ctx.rules.tick, &handles),
    };
    run_event(ctx, host, &order, &EventKind::RoomStart)
}

/// One room creation action (step 6).
fn run_action<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    action: &RoomAction,
) -> Result<(), SimError> {
    match action {
        RoomAction::SetGlobal { var, value } => {
            ctx.w.globals.insert(*var, value.clone());
        }
        RoomAction::Spawn { kind, x, y, params } => {
            let init = SpawnInit {
                params: params.clone(),
                ..SpawnInit::default()
            };
            host.spawn(ctx, *kind, *x, *y, init)?;
            stop_if_halted(ctx)?;
        }
        RoomAction::LoadGame => {
            // kaenbyou and eat-the-lemon rLoad: loadGame creates the kid at the saved spot and
            // requests the saved room (rooms Confirmed); R23 decides when that change happens.
            let slot =
                ctx.w.save.clone().ok_or_else(|| {
                    SimError::unsupported("LoadGame in room creation code with no save (R23)")
                })?;
            host.load_save(ctx, &slot)?;
            stop_if_halted(ctx)?;
            ctx.w.pending = Some(RoomRequest::Goto(slot.room));
        }
        RoomAction::DestroySelf => {
            return Err(SimError::unsupported(
                "instance_destroy() in room creation code: which instance is self is UNKNOWN (R24)",
            ));
        }
    }
    Ok(())
}

/// The views of a room as its file sets them.
fn views_of(room: &RoomDef) -> [View; 8] {
    room.views.each_ref().map(|d| View {
        enabled: room.views_enabled,
        visible: d.visible,
        x: d.view.0,
        y: d.view.1,
        w: d.view.2,
        h: d.view.3,
        port: ViewPort {
            x: d.port.0,
            y: d.port.1,
            w: d.port.2,
            h: d.port.3,
        },
        follow: d.follow.map(Target::Kind),
        hborder: d.hborder,
        vborder: d.vborder,
        hspeed: d.hspeed,
        vspeed: d.vspeed,
        angle: Real::ZERO,
    })
}

/// Queues a room output for the client (what `Ctx::emit` does).
fn emit(ctx: &mut Ctx, out: RoomOutput) {
    ctx.w.output.push(OutputEvent::Room(out));
}
