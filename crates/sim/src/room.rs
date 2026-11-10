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
//! 6. The room creation actions run, as data. On a persistent-room revisit only when
//!    [`RoomRules::creation_code_on_revisit`](crate::rules::RoomRules::creation_code_on_revisit)
//!    (rooms UNKNOWN; no persistent room of the 20 games has creation code).
//! 7. Room Start for every instance in the room when this step begins (R10): the room's own
//!    instances, then the carried persistent ones (Confirmed (recorder), [`RoomStartOrder`]).
//!
//! A stripped (ranked) run (`World::mode`, #561, `crate::stripped`) changes three things: every
//! entry of a cut room (a request, `game_restart`'s first room, a start point, a save load)
//! enters its `to` room instead; step 4 skips cut kinds and cut placed instances; step 6 skips
//! `spawn` actions of cut kinds. `StartPoint::NewGame` starts at the ranked start point.
//!
//! `room_restart` and `room_goto` of the current room rebuild a non-persistent room (Inferred).
//! For a persistent room both are UNKNOWN (unused by the 20 games) and give
//! `SimError::Unsupported`.
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
//!
//! **Contract note for WP1 and WP8a.** Steps 1, 5 and 7 are the phases [`Phase::RoomEnd`],
//! [`Phase::GameStart`] and [`Phase::RoomStart`], run by [`RoomHost::run_phase`] on an order
//! this module computes, not by `events::run_phase`: `events::run_phase(ctx, phase)` takes no
//! order, and the Confirmed (recorder) Room Start order (own instances, then carried ones) and
//! the Room End switch need one. `events::run_phase` therefore never sees these three phases
//! (WP1 need not handle them), and a per-phase hook for these steps (WP8a `--stages`) belongs
//! in `RoomHost::run_phase`. Unlike R8 in the tick, `Flow::AbortTick` does not stop the
//! sequence here (R23: the sequence always finishes).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::collide::Target;
use crate::collide::grid::Grid;
use crate::def::{KindId, RoomAction, RoomDef, RoomId};
use crate::error::SimError;
use crate::events::{self, EventKind, Phase};
use crate::num::Real;
use crate::output::OutputEvent;
use crate::rules::TickRules;
use crate::rules::room::{NestedRequest, RoomEndOrder, RoomStartOrder};
use crate::save::{self, SaveSlot};
use crate::stripped::PlayMode;
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

    /// Runs one phase of the room sequence ([`Phase::RoomEnd`], [`Phase::GameStart`] or
    /// [`Phase::RoomStart`]) on `order`, the snapshot taken when the step begins: each instance
    /// that still exists at its turn gets the phase's event. `Halt` stops at once; `AbortTick`
    /// does not stop the phase (R23). Any other phase is an invariant break.
    fn run_phase(&mut self, ctx: &mut Ctx, phase: Phase, order: &[InstId]) -> Flow {
        let ev = match phase {
            Phase::RoomEnd => EventKind::RoomEnd,
            Phase::GameStart => EventKind::GameStart,
            Phase::RoomStart => EventKind::RoomStart,
            _ => {
                return ctx.halt(SimError::invariant(format!(
                    "{phase:?} is not a room sequence phase"
                )));
            }
        };
        for &id in order {
            if !ctx.w.exists(id) {
                continue;
            }
            if self.dispatch(ctx, id, &ev) == Flow::Halt {
                return Flow::Halt;
            }
        }
        Flow::Continue
    }

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
/// Before the load, `room`, `room_speed` (the saved room's file speed, R14) and the views are
/// already the saved room's, so the kid's Create (alarm[0] = room_speed, room-role checks) sees
/// that room (Design: the original loads in its boot room after world's Room Start; FromSave
/// has no boot room).
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
    if matches!(start, StartPoint::Room { .. }) {
        age_boot(ctx)?;
    }
    let target = match start {
        StartPoint::NewGame { difficulty } => {
            ctx.w.save = None;
            ctx.w.engine.run.difficulty = *difficulty;
            match ctx.w.mode {
                PlayMode::Original => ctx.def.new_game_room,
                PlayMode::Stripped => ranked_start(ctx),
            }
        }
        StartPoint::Room { room, difficulty } => {
            ctx.w.engine.run.difficulty = *difficulty;
            redirect(ctx, *room)
        }
        StartPoint::FromSave(slot) => {
            ctx.w.save = Some(slot.clone());
            let target = redirect(ctx, slot.room);
            let room = room_def(ctx, target)?;
            set_room_state(ctx, target, room);
            load_slot(ctx, host, slot)?;
            // The saved room is entered right below; a request the load made for it is that
            // same entry.
            ctx.w.pending = None;
            target
        }
    };
    room_def(ctx, target)?;
    enter(ctx, host, None, target)?;
    after_sequence(ctx, host)
}

/// A stripped run's start (#561): sets `[stripped].start`'s globals (and the engine difficulty
/// from a `difficulty` global) after the boot instances, in place of what the cut menus set, and
/// returns its room (default `GameDef::new_game_room`).
fn ranked_start(ctx: &mut Ctx) -> RoomId {
    let start = &ctx.def.stripped.start;
    for (var, value) in &start.globals {
        ctx.w.globals.insert(*var, value.clone());
    }
    if let Some(d) = start.difficulty {
        ctx.w.engine.run.difficulty = d;
    }
    start.room.unwrap_or(ctx.def.new_game_room)
}

/// Where an entry of `room` goes: a stripped run's cut room goes to its `to` room (#561).
fn redirect(ctx: &Ctx, room: RoomId) -> RoomId {
    ctx.def.stripped.redirect(ctx.w.mode, room)
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

/// Loads `slot` as loadGame does: the saved difficulty (kaenbyou tr-room-12 row 137: 0 from the
/// save, 1 before), then the kid at the saved spot (`RoomHost::load_save`).
fn load_slot<H: RoomHost + ?Sized>(ctx: &mut Ctx, host: &mut H, slot: &SaveSlot) -> Result<(), SimError> {
    ctx.w.engine.run.difficulty = slot.difficulty;
    host.load_save(ctx, slot)?;
    stop_if_halted(ctx)
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
        RoomRequest::Restart => current,
        RoomRequest::GameRestart | RoomRequest::GameEnd => {
            return Err(SimError::invariant(format!("{req:?} is not a room change")));
        }
    };
    let target = redirect(ctx, target);
    // room_restart and room_goto(current) rebuild a non-persistent room (Inferred); for a
    // persistent one both are UNKNOWN.
    if room_def(ctx, target)?.persistent && target == current {
        return Err(SimError::unsupported(format!(
            "{req:?} of the current persistent room {} (rooms UNKNOWN; unused by the 20 games)",
            current.0
        )));
    }
    Ok(target)
}

/// The definition of `room`; an unknown id is an invariant break (the loader checks rooms).
fn room_def<'d>(ctx: &Ctx<'d>, room: RoomId) -> Result<&'d RoomDef, SimError> {
    let def = ctx.def;
    def.room(room)
        .ok_or_else(|| SimError::invariant(format!("room {} does not exist", room.0)))
}

/// The recorder's `--room` jump happens on frame 1, so the boot instances (rInit's world) have
/// already had one animation update when the first sampled tick (frame 2) runs: the world, which
/// has no sprite, reads `image_index` 2.0 on frame 2 (traces/kaenbyou/t5-input-restart-latch-b.csv
/// and audio-supersound-deathfade-c.csv; rely-the-needle-4-2/t-room-1.csv: 0.0 on frame 0, then
/// one per tick). `StartPoint::Room` gives the boot instances that one update.
fn age_boot(ctx: &mut Ctx) -> Result<(), SimError> {
    let ids: Vec<_> = ctx
        .w
        .iter_alive()
        .filter(|i| i.persistent)
        .map(|i| i.id)
        .collect();
    for id in ids {
        let frames = ctx.w.get(id).and_then(|i| i.img.sprite()).map_or(0, |s| {
            s.index()
                .and_then(|n| ctx.def.sprites.get(n))
                .map_or(0, |d| d.frames)
        });
        if let Some(inst) = ctx.w.get_mut(id) {
            crate::anim::advance(&mut inst.img, frames, &ctx.rules.tick)?;
        }
    }
    Ok(())
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
/// `globals_survive_restart`, then boot again and enter `room_first` (room order index 0, the
/// order `room_goto_next` follows; rInit in 19 of the 20 games) as the first room. The save (a
/// file) and the sounds (Inferred, S4: `SoundRules::keep_on_restart`) are kept.
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
    if !ctx.rules.sound.keep_on_restart {
        crate::sound::stop_all(ctx);
    }
    if !ctx.rules.room.globals_survive_restart {
        ctx.w.globals.clear();
        // The temp-file marker of a retry survives: it is a file, not a global (WP4b).
        let boot_load = ctx.w.engine.run.boot_load;
        ctx.w.engine = EngineGlobals::default();
        ctx.w.engine.run.boot_load = boot_load;
    }
    emit(ctx, RoomOutput::GameRestarted);
    create_boot(ctx, host)?;
    let first = redirect(ctx, RoomId(0));
    room_def(ctx, first)?;
    enter(ctx, host, None, first)
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
            // non-finite values), so `partial_cmp` always answers. This tie-break does not
            // reproduce showdown tr-room-14 (see `RoomEndOrder::Depth`).
            v.sort_by(|a, b| {
                b.0.get()
                    .partial_cmp(&a.0.get())
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1.cmp(&b.1))
            });
            v.into_iter().map(|(_, id)| id).collect()
        }
    };
    run_room_phase(ctx, host, Phase::RoomEnd, &order)
}

/// Runs a room sequence phase on `order` through [`RoomHost::run_phase`]; `Halt` becomes the
/// halting error.
fn run_room_phase<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    phase: Phase,
    order: &[InstId],
) -> Result<(), SimError> {
    match host.run_phase(ctx, phase, order) {
        Flow::Halt => Err(halted_error(ctx)),
        Flow::Continue | Flow::AbortTick => Ok(()),
    }
}

/// The room state on entry: room, room tick, `room_speed` = the file speed (R14), the views.
fn set_room_state(ctx: &mut Ctx, to: RoomId, room: &RoomDef) {
    ctx.w.room = to;
    ctx.w.room_tick = 0;
    ctx.w.room_speed = Real::new(f64::from(room.file_speed));
    ctx.w.views = views_of(room);
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
    set_room_state(ctx, to, room);
    emit(ctx, RoomOutput::Entered { from, to });

    // Step 4: placed instances in file order, params before Create (inside spawn).
    if !restoring {
        let stripped = &ctx.def.stripped;
        for p in &room.instances {
            if stripped.kind_cut(ctx.w.mode, p.kind) || stripped.instance_cut(ctx.w.mode, to, p.name) {
                continue;
            }
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
        run_room_phase(ctx, host, Phase::GameStart, &order)?;
    }

    // Step 6: room creation actions; on a revisit per the switch (rooms UNKNOWN).
    if !restoring || ctx.rules.room.creation_code_on_revisit {
        for action in &room.on_create {
            run_action(ctx, host, action)?;
        }
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
    run_room_phase(ctx, host, Phase::RoomStart, &order)
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
        RoomAction::Spawn { kind, .. } if ctx.def.stripped.kind_cut(ctx.w.mode, *kind) => {}
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
            load_slot(ctx, host, &slot)?;
            ctx.w.pending = Some(RoomRequest::Goto(slot.room));
        }
        // R24 (Confirmed, recorder): instance_destroy() in room creation code destroys nothing
        // and the rest of the code runs (showdown a3-t18-a rows 42-43, a3-t20, tr-room-14).
        RoomAction::DestroySelf => {}
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
