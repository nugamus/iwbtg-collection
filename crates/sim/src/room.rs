//! Room entry and the room change sequence (docs/architecture/sim/rooms.md).
//!
//! WP0 declares; WP4a owns.
//!
//! The sequence follows the manual's 7 steps (CLONE_SPEC rooms, "Room start sequence",
//! Confirmed), run as one [`RoomTransaction`] with a declared stage order (#1174 A2):
//!
//! 1. Room End for every instance of the old room.
//! 2. Non-persistent instances are removed, with no Destroy event. Deactivated persistent
//!    instances count as non-persistent here: they stay behind (Confirmed, manual 403_06). When
//!    the old room is persistent, everything that stays behind is stored with it.
//! 3. Persistent instances move to the new room with their ids.
//! 4. A new or non-persistent room creates its placed instances in file order, each with its
//!    params (instance creation code) applied before its Create. One with a GM8 id (`gm_id`)
//!    gets that id, and is skipped when an instance with that id is already alive (a carried
//!    persistent one; CLONE_SPEC, Instances, "Placed instance ids", #669) or was destroyed
//!    earlier in the same tick and is still pending removal (#670). A persistent room that was
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
//! [`Phase::GameStart`] and [`Phase::RoomStart`], dispatched by this module one event at a time
//! on an order it computes, not by `events::run_phase`: `events::run_phase(ctx, phase)` takes no
//! order, and the Confirmed (recorder) Room Start order (own instances, then carried ones) and
//! the Room End switch need one. The loop is [`RoomHost::run_phase`]'s with a nested-request
//! capture after each event (#1256), so this module calls [`RoomHost::dispatch`] directly.
//! `events::run_phase` therefore never sees these three phases (WP1 need not handle them), and a
//! per-phase hook for these steps (WP8a `--stages`) belongs in the dispatch loop here. Unlike R8
//! in the tick, `Flow::AbortTick` does not stop the sequence here (R23: the sequence always
//! finishes).

use std::collections::{BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::collide::Target;
use crate::collide::grid::Grid;
use crate::def::game::{NestedRequests, RoomStage};
use crate::def::{GlobalIndex, KindId, RoomAction, RoomDef, RoomId, RoomRole, RoomWhen, VarId};
use crate::error::SimError;
use crate::events::{self, EventKind, Phase};
use crate::num::Real;
use crate::output::OutputEvent;
use crate::rules::TickRules;
use crate::rules::room::{NestedRequest, RoomEndOrder, RoomStartOrder};
use crate::save::{self, SaveSlot};
use crate::stripped::PlayMode;
use crate::view::{View, ViewPort};
use crate::world::{Ctx, EngineGlobals, Flow, InstId, Instance, SimStatus, SpawnInit, Value, World};

/// How many destroyed instance ids a [`RoomRequest::GotoAfterRemovals`] carries (#670).
pub const MAX_REMOVED: usize = 16;

/// The instance ids destroyed earlier in a tick, for the room build's pending-removal check
/// (#670). Fixed-size so [`RoomRequest`] stays `Copy`; 16 is above every room's placed-kid
/// count in the 20 games.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RemovedInstances {
    ids: [InstId; MAX_REMOVED],
    len: u8,
}

impl Default for RemovedInstances {
    fn default() -> RemovedInstances {
        RemovedInstances {
            ids: [InstId(0); MAX_REMOVED],
            len: 0,
        }
    }
}

impl RemovedInstances {
    /// The ids, keeping the first [`MAX_REMOVED`].
    pub fn new(ids: &[InstId]) -> RemovedInstances {
        let mut out = RemovedInstances::default();
        let n = ids.len().min(MAX_REMOVED);
        out.ids[..n].copy_from_slice(&ids[..n]);
        out.len = u8::try_from(n).unwrap_or(u8::try_from(MAX_REMOVED).unwrap_or(u8::MAX));
        out
    }

    /// Whether `id` was destroyed earlier this tick.
    pub fn contains(&self, id: InstId) -> bool {
        self.ids[..usize::from(self.len)].contains(&id)
    }

    /// No instance was destroyed.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

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
    /// A room change requested in the same tick that `removed` instances were destroyed (#670).
    ///
    /// In the original, an instance destroyed earlier in the frame is still pending removal when
    /// the room starts, so a placed instance with the same room-file id is not created again.
    /// The retry that destroys the room's placed kids records their ids here; the new room's
    /// build skips those placed instances. An ordinary request carries no removals.
    GotoAfterRemovals {
        /// The room to enter.
        room: RoomId,
        /// The instance ids destroyed earlier this tick.
        removed: RemovedInstances,
    },
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
    ///
    /// The room transaction runs the same loop itself ([`RoomHost::dispatch`] per id) so it can
    /// capture a nested request after each event (#1256); this method is the equivalent phase
    /// entry point for a host that drives a phase outside the transaction.
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
    let mut queue = RoomQueue::for_ctx(ctx);
    create_boot(ctx, host, &mut queue)?;
    if matches!(start, StartPoint::Room { .. }) {
        age_boot(ctx)?;
    }
    let target = match start {
        StartPoint::NewGame { difficulty } => {
            // `new_game` (0005, #1149): reset scope `run` drops the save and the run counters;
            // the spawn policy is `room_start`, and the definition names that room.
            let new_game = ctx.def.lifecycle().new_game;
            if new_game.reset_scope == crate::def::game::ResetScope::Run {
                ctx.w.save = None;
            }
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
    enter(ctx, host, None, target, &mut queue, RemovedInstances::default())?;
    after_sequence(ctx, host, &mut queue)
}

/// A stripped run's start (#561): sets `[stripped].start`'s globals (and the engine difficulty
/// from a `difficulty` global) after the boot instances, in place of what the cut menus set, and
/// returns its room (default `GameDef::new_game_room`).
fn ranked_start(ctx: &mut Ctx) -> RoomId {
    let start = ctx.def.map_start(ctx.w.map);
    for (var, value) in &start.globals {
        // A global gets its own value: an array is copied (GML 8 rule).
        ctx.w.globals.insert(*var, value.assigned());
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

/// Requests raised while a room transaction runs (R13/R23; #1256).
///
/// `NestedRequests::LastWins` (gm8) leaves the world's one pending slot alone: `capture` does
/// nothing and `next` takes that slot, exactly as before. `NestedRequests::Queue` takes each
/// request out of the slot right after the event or creation action that raised it and appends
/// it, so `next` returns them in raise order. The queue lives for this tick's chain only: the
/// world store is identity-save's, so `NextTick` keeps the one-slot behavior.
struct RoomQueue {
    requests: VecDeque<RoomRequest>,
    fifo: bool,
}

impl RoomQueue {
    fn for_ctx(ctx: &Ctx) -> RoomQueue {
        RoomQueue::with_policy(ctx.def.lifecycle().enter_room, ctx.rules)
    }

    fn with_policy(
        policy: crate::def::game::EnterRoomPolicy,
        rules: &crate::rules::RunnerRules,
    ) -> RoomQueue {
        let fifo = policy.nested_requests == NestedRequests::Queue
            && rules.room.nested_request == NestedRequest::ChainAfterSequence;
        RoomQueue {
            requests: VecDeque::new(),
            fifo,
        }
    }

    /// Takes a request the event or action just raised.
    fn capture(&mut self, ctx: &mut Ctx) {
        if self.fifo
            && let Some(req) = ctx.w.pending.take()
        {
            self.requests.push_back(req);
        }
    }

    /// The next request to run.
    fn next(&mut self, ctx: &mut Ctx) -> Option<RoomRequest> {
        if self.fifo {
            self.requests.pop_front()
        } else {
            ctx.w.pending.take()
        }
    }
}

fn change_inner<H: RoomHost + ?Sized>(ctx: &mut Ctx, req: RoomRequest, host: &mut H) -> Result<(), SimError> {
    let policy = ctx.def.lifecycle().enter_room;
    change_inner_with(ctx, req, host, policy)
}

/// [`change_inner`] with an explicit enter-room policy. The ruleset/profile layer (#1160) will
/// select the policy; the public [`perform_change`] uses the definition's default.
fn change_inner_with<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    req: RoomRequest,
    host: &mut H,
    policy: crate::def::game::EnterRoomPolicy,
) -> Result<(), SimError> {
    stop_if_halted(ctx)?;
    if ctx.w.status == SimStatus::Ended {
        return Ok(());
    }
    ctx.w.pending = None;
    let mut queue = RoomQueue::with_policy(policy, ctx.rules);
    run_request(ctx, host, req, &mut queue)?;
    after_sequence(ctx, host, &mut queue)
}

/// R23: a request raised during the sequence. `ChainAfterSequence` performs it now (bounded,
/// FIFO under [`NestedRequests::Queue`]); `NextTick` leaves it pending for tick row 15 of the
/// next tick.
fn after_sequence<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    queue: &mut RoomQueue,
) -> Result<(), SimError> {
    if ctx.rules.room.nested_request == NestedRequest::NextTick {
        return Ok(());
    }
    let max = ctx.rules.room.max_chained_changes;
    let mut chained: u8 = 0;
    while ctx.w.status == SimStatus::Running {
        let Some(req) = queue.next(ctx) else {
            break;
        };
        if chained >= max {
            return Err(SimError::unsupported(format!(
                "more than {max} chained room changes in one tick (R23); last request {req:?}"
            )));
        }
        chained = chained.saturating_add(1);
        run_request(ctx, host, req, queue)?;
    }
    Ok(())
}

/// Runs one request: a room change, game_restart or game_end.
fn run_request<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    req: RoomRequest,
    queue: &mut RoomQueue,
) -> Result<(), SimError> {
    match req {
        RoomRequest::GameEnd => {
            ctx.w.status = SimStatus::Ended;
            emit(ctx, RoomOutput::GameEnded);
            Ok(())
        }
        RoomRequest::GameRestart => game_restart(ctx, host, queue),
        RoomRequest::Goto(_)
        | RoomRequest::Next
        | RoomRequest::Previous
        | RoomRequest::Restart
        | RoomRequest::GotoAfterRemovals { .. } => {
            let removed = match &req {
                RoomRequest::GotoAfterRemovals { removed, .. } => *removed,
                _ => RemovedInstances::default(),
            };
            let target = resolve(ctx, req)?;
            let from = ctx.w.room;
            enter(ctx, host, Some(from), target, queue, removed)
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
        RoomRequest::Goto(room) | RoomRequest::GotoAfterRemovals { room, .. } => room,
        RoomRequest::Next => neighbour(ctx, current, true)?,
        RoomRequest::Previous => neighbour(ctx, current, false)?,
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

/// The next or previous room in room order. Skips the Boot-role room when the lifecycle's
/// enter-room policy says so (#1149): the Boot room is engine init, not a stage, so
/// `room_goto_next` and `room_goto_previous` never land on it.
fn neighbour(ctx: &Ctx, current: RoomId, forward: bool) -> Result<RoomId, SimError> {
    let skip_boot = ctx.def.lifecycle().enter_room.skip_boot_rooms;
    let step = if forward { 1 } else { -1 };
    let mut next = i64::from(current.0) + step;
    while let Ok(raw) = u32::try_from(next) {
        let Ok(index) = usize::try_from(raw) else {
            break;
        };
        let Some(room) = ctx.def.rooms.get(index) else {
            break;
        };
        if !(skip_boot && room.role == RoomRole::Boot) {
            return Ok(RoomId(raw));
        }
        next += step;
    }
    Err(SimError::Script {
        message: if forward {
            format!("room_goto_next: no room after room {}", current.0)
        } else {
            format!("room_goto_previous: no room before room {}", current.0)
        },
    })
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
fn create_boot<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    queue: &mut RoomQueue,
) -> Result<(), SimError> {
    let def = ctx.def;
    for &kind in &def.boot {
        let id = host.spawn(ctx, kind, Real::ZERO, Real::ZERO, SpawnInit::default())?;
        queue.capture(ctx);
        stop_if_halted(ctx)?;
        if let Some(inst) = ctx.w.get_mut(id) {
            inst.persistent = true;
        }
    }
    Ok(())
}

/// The room `game_restart` enters: the Boot-role room (engine init), else room order index 0.
///
/// The loader marks the engine init room with `RoomRole::Boot`; packages that predate that role
/// keep the gm8 shape where rInit is the first room, so index 0 is the fallback (#1149). The id
/// is an implementation detail of the profile, never a save or replay identity.
pub(crate) fn restart_room(def: &crate::def::GameDef) -> RoomId {
    def.rooms
        .iter()
        .position(|r| r.role == RoomRole::Boot)
        .and_then(|i| u32::try_from(i).ok())
        .map_or(RoomId(0), RoomId)
}

/// game_restart (Inferred; R16): optionally Room End, remove every instance (persistent ones
/// too) without events, forget the persistent rooms, clear the globals unless
/// `globals_survive_restart`, then boot again and enter the Boot-role room ([`restart_room`];
/// rInit in 19 of the 20 games) as the first room. The save (a file) and the sounds (Inferred,
/// S4: `SoundRules::keep_on_restart`) are kept.
fn game_restart<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    queue: &mut RoomQueue,
) -> Result<(), SimError> {
    if ctx.rules.room.restart_events {
        room_end(ctx, host, queue)?;
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
    create_boot(ctx, host, queue)?;
    let first = redirect(ctx, restart_room(ctx.def));
    room_def(ctx, first)?;
    enter(ctx, host, None, first, queue, RemovedInstances::default())
}

/// Step 1: Room End for every alive, active instance that handles it.
fn room_end<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    queue: &mut RoomQueue,
) -> Result<(), SimError> {
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
    run_room_phase(ctx, host, Phase::RoomEnd, &order, queue)
}

/// Runs a room sequence phase on `order`, one event at a time, and captures a request each event
/// raises (so the FIFO queue sees them in raise order). `Halt` becomes the halting error; an
/// `AbortTick` does not stop the phase (R23). This is [`RoomHost::run_phase`]'s loop with the
/// queue capture in it.
fn run_room_phase<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    phase: Phase,
    order: &[InstId],
    queue: &mut RoomQueue,
) -> Result<(), SimError> {
    let ev = match phase {
        Phase::RoomEnd => EventKind::RoomEnd,
        Phase::GameStart => EventKind::GameStart,
        Phase::RoomStart => EventKind::RoomStart,
        _ => {
            return Err(SimError::invariant(format!(
                "{phase:?} is not a room sequence phase"
            )));
        }
    };
    for &id in order {
        if !ctx.w.exists(id) {
            continue;
        }
        if host.dispatch(ctx, id, &ev) == Flow::Halt {
            return Err(halted_error(ctx));
        }
        queue.capture(ctx);
    }
    Ok(())
}

/// The room state on entry: room, room tick, `room_speed` = the file speed (R14), the views.
fn set_room_state(ctx: &mut Ctx, to: RoomId, room: &RoomDef) {
    ctx.w.room = to;
    ctx.w.room_tick = 0;
    ctx.w.room_speed = Real::new(f64::from(room.file_speed));
    ctx.w.views = views_of(room);
}

/// Steps 2 to 7: leave `from` (if any) and enter `to` as one room transaction. `from = None` is
/// the first room of the game, which gets Game Start.
fn enter<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    from: Option<RoomId>,
    to: RoomId,
    queue: &mut RoomQueue,
    removed: RemovedInstances,
) -> Result<(), SimError> {
    RoomTransaction::new(ctx, from, to, removed)?.run(ctx, host, queue)
}

/// One engine-owned room transaction (decision 0005; #1174 A2).
///
/// A room change is prepared, committed once and then built in the order
/// [`EnterRoomPolicy::stages`](crate::def::game::EnterRoomPolicy::stages) declares. `prepare`
/// only reads (the two rooms and the carried instance set); `commit` is the single point that
/// changes the instance store and the room state and reports the entry; the build stages then
/// create and start the new room. A nested request raised in any stage still goes to
/// `World::pending` and is chained or deferred by [`after_sequence`] (R23), so the transaction
/// always finishes before the next change is resolved.
struct RoomTransaction {
    /// The room left, or `None` for the first room of the game.
    from: Option<RoomId>,
    /// The room entered.
    to: RoomId,
    /// The declared stage order ([`RoomStage`]).
    stages: [RoomStage; 7],
    /// Persistent, active instances that move to the new room; staged by `Prepare`.
    carried: Vec<Instance>,
    /// Instances the old persistent room keeps; staged by `Prepare`.
    left: Vec<Instance>,
    /// The carried ids, for the Room Start order.
    carried_ids: BTreeSet<InstId>,
    /// The target room had stored instances: the build restores them instead of creating
    /// placed ones.
    restoring: bool,
    /// The placed instances this transaction created, in file order. The creation code's
    /// `DestroySelf` names it explicitly (there is no calling instance; R24, #1256).
    built: Vec<InstId>,
    /// Instances destroyed earlier this tick, from a [`RoomRequest::GotoAfterRemovals`]: their
    /// room-file ids are still pending removal, so a placed instance with one is not created
    /// again (#670).
    removed: RemovedInstances,
    /// `Prepare` ran.
    prepared: bool,
}

impl RoomTransaction {
    fn new(
        ctx: &Ctx,
        from: Option<RoomId>,
        to: RoomId,
        removed: RemovedInstances,
    ) -> Result<RoomTransaction, SimError> {
        room_def(ctx, to)?;
        Ok(RoomTransaction {
            from,
            to,
            stages: ctx.def.lifecycle().enter_room.stages,
            carried: Vec::new(),
            left: Vec::new(),
            carried_ids: BTreeSet::new(),
            restoring: false,
            built: Vec::new(),
            removed,
            prepared: false,
        })
    }

    fn run<H: RoomHost + ?Sized>(
        mut self,
        ctx: &mut Ctx,
        host: &mut H,
        queue: &mut RoomQueue,
    ) -> Result<(), SimError> {
        for stage in self.stages {
            match stage {
                RoomStage::RoomEnd => {
                    if self.from.is_some() {
                        room_end(ctx, host, queue)?;
                    }
                }
                RoomStage::Prepare => self.prepare(ctx)?,
                RoomStage::Commit => self.commit(ctx)?,
                RoomStage::Placed => self.build_placed(ctx, host, queue)?,
                RoomStage::GameStart => self.game_start(ctx, host, queue)?,
                RoomStage::CreationCode => self.creation_code(ctx, host, queue)?,
                RoomStage::RoomStart => self.room_start(ctx, host, queue)?,
            }
        }
        queue.capture(ctx);
        Ok(())
    }

    /// Steps 2 and 3, read-only: split the old room's instances into carried (persistent and
    /// active) and left behind, and note whether the target room has stored instances to
    /// restore. No world state changes here.
    fn prepare(&mut self, ctx: &Ctx) -> Result<(), SimError> {
        let room = room_def(ctx, self.to)?;
        let (carried, left): (Vec<Instance>, Vec<Instance>) = ctx
            .w
            .instances()
            .iter()
            .filter(|i| i.alive)
            .cloned()
            .partition(|i| i.persistent && i.active);
        self.carried_ids = carried.iter().map(|i| i.id).collect();
        self.restoring = room.persistent && ctx.w.persistent_rooms.contains_key(&self.to);
        self.carried = carried;
        self.left = left;
        self.prepared = true;
        Ok(())
    }

    /// The transaction's single commit point: store the old persistent room's instances,
    /// take the target room's stored ones, rebuild the instance store (carried plus restored,
    /// sorted by id), set the room state and report the entry.
    fn commit(&mut self, ctx: &mut Ctx) -> Result<(), SimError> {
        if !self.prepared {
            return Err(SimError::invariant(
                "room transaction committed before it was prepared",
            ));
        }
        let room = room_def(ctx, self.to)?;
        if let Some(old) = self.from
            && ctx.def.room(old).is_some_and(|r| r.persistent)
        {
            ctx.w.persistent_rooms.insert(old, std::mem::take(&mut self.left));
        }
        let restored = if room.persistent {
            ctx.w.persistent_rooms.remove(&self.to)
        } else {
            None
        };
        // Rebuild the store: carried plus restored, sorted by id (both lists already are), as
        // the store keeps them. Every instance is re-inserted, which marks it in the new,
        // empty grid.
        let ids: Vec<InstId> = ctx.w.instances().iter().map(|i| i.id).collect();
        for id in ids {
            if let Some(inst) = ctx.w.get_mut(id) {
                inst.alive = false;
            }
        }
        ctx.w.sweep();
        ctx.w.grid = Grid::default();
        let mut merged = std::mem::take(&mut self.carried);
        merged.extend(restored.unwrap_or_default());
        merged.sort_by_key(|i| i.id);
        for inst in merged {
            ctx.w.insert(inst)?;
        }

        // Room state on entry (R14, views, output).
        set_room_state(ctx, self.to, room);
        emit(
            ctx,
            RoomOutput::Entered {
                from: self.from,
                to: self.to,
            },
        );
        Ok(())
    }

    /// Step 4: placed instances in file order, params before Create (inside spawn).
    fn build_placed<H: RoomHost + ?Sized>(
        &mut self,
        ctx: &mut Ctx,
        host: &mut H,
        queue: &mut RoomQueue,
    ) -> Result<(), SimError> {
        let policy = ctx.def.lifecycle().enter_room;
        if self.restoring || !policy.rebuild_placed {
            return Ok(());
        }
        let room = room_def(ctx, self.to)?;
        let stripped = &ctx.def.stripped;
        for p in &room.instances {
            if stripped.kind_cut(ctx.w.mode, p.kind) || stripped.instance_cut(ctx.w.mode, self.to, p.name) {
                continue;
            }
            // A placed instance whose GM8 id is already in use (a persistent instance carried
            // in, possibly from another room that shares ids) is not created again (CLONE_SPEC,
            // Instances, "Placed instance ids"). An instance destroyed earlier this tick is still
            // pending removal in the original, so its id counts too (#670; the retry records
            // those ids in `removed`).
            let gm_id = p.gm_id.map(|id| InstId(u64::from(id)));
            if gm_id.is_some_and(|id| ctx.w.get(id).is_some() || self.removed.contains(id)) {
                continue;
            }
            let init = SpawnInit {
                params: p.params.clone(),
                variant: None,
                rank: Some(p.rank),
                xscale: p.xscale,
                yscale: p.yscale,
                angle: p.angle,
                visible: p.visible,
                depth: p.depth,
                name: p.name,
                group: p.group,
                run_create: true,
                gm_id,
            };
            let id = host.spawn(ctx, p.kind, p.x, p.y, init)?;
            self.built.push(id);
            queue.capture(ctx);
            stop_if_halted(ctx)?;
        }
        Ok(())
    }

    /// Step 5: Game Start, first room only. No kind declares it (no game in the 20 uses it),
    /// so every instance gets it.
    fn game_start<H: RoomHost + ?Sized>(
        &self,
        ctx: &mut Ctx,
        host: &mut H,
        queue: &mut RoomQueue,
    ) -> Result<(), SimError> {
        if self.from.is_some() {
            return Ok(());
        }
        let order = host.dispatch_order(ctx.w, &ctx.rules.tick, &|i: &Instance| i.alive && i.active);
        run_room_phase(ctx, host, Phase::GameStart, &order, queue)
    }

    /// Step 6: room creation actions; on a revisit per the switch (rooms UNKNOWN).
    fn creation_code<H: RoomHost + ?Sized>(
        &self,
        ctx: &mut Ctx,
        host: &mut H,
        queue: &mut RoomQueue,
    ) -> Result<(), SimError> {
        let room = room_def(ctx, self.to)?;
        if self.restoring && !ctx.rules.room.creation_code_on_revisit {
            return Ok(());
        }
        let context = CreationContext {
            room: self.to,
            built: &self.built,
        };
        for action in &room.on_create {
            run_action(ctx, host, action, &context)?;
            queue.capture(ctx);
        }
        Ok(())
    }

    /// Step 7: Room Start, snapshot taken now (R10).
    fn room_start<H: RoomHost + ?Sized>(
        &self,
        ctx: &mut Ctx,
        host: &mut H,
        queue: &mut RoomQueue,
    ) -> Result<(), SimError> {
        let def = ctx.def;
        let handles =
            |i: &Instance| i.alive && i.active && def.kind(i.kind).is_some_and(|k| k.events.room_start);
        let order: Vec<InstId> = match ctx.rules.room.room_start_order {
            RoomStartOrder::RoomThenCarried => {
                let in_room = ctx.w.instances().iter().filter(|i| handles(i));
                let (carried, own): (Vec<InstId>, Vec<InstId>) = in_room
                    .map(|i| i.id)
                    .partition(|id| self.carried_ids.contains(id));
                own.into_iter().chain(carried).collect()
            }
            RoomStartOrder::Dispatch => host.dispatch_order(ctx.w, &ctx.rules.tick, &handles),
        };
        run_room_phase(ctx, host, Phase::RoomStart, &order, queue)
    }
}

/// What a room creation action can act on (#1256).
///
/// Room creation code belongs to the room, not to an instance, so `instance_destroy()` has no
/// calling instance to destroy: the original's recorder confirms it destroys nothing (R24).
/// `built` names the placed instances this transaction created, in build order, so a standard
/// model can replace the bare `instance_destroy()` with an explicit target once A1's identity
/// predicate lands (#1174).
struct CreationContext<'a> {
    room: RoomId,
    built: &'a [InstId],
}

/// One room creation action (step 6).
fn run_action<H: RoomHost + ?Sized>(
    ctx: &mut Ctx,
    host: &mut H,
    action: &RoomAction,
    context: &CreationContext<'_>,
) -> Result<(), SimError> {
    match action {
        RoomAction::SetGlobal {
            var,
            value,
            difficulty,
        } => {
            ctx.w.globals.insert(*var, value.assigned());
            if let Some(d) = difficulty {
                ctx.w.engine.run.difficulty = *d;
            }
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
        // There is no calling instance: `context` names the room and the instances this
        // transaction built, which is the explicit target a standard model would use (A1).
        RoomAction::DestroySelf => {
            let _ = (context.room, context.built);
        }
        RoomAction::SetGlobalIndexed { var, index, value } => {
            let i = index_value(ctx, *index)?;
            // GM8 aliases a global and its element 0 (research#130): `global.a[0] = v` writes
            // `global.a`, not a separate `a[0]` variable; elements 1 and up are their own.
            let id = if i == 0 {
                run_time_var(ctx, var)?
            } else {
                run_time_var(ctx, &format!("{var}[{i}]"))?
            };
            ctx.w.globals.insert(id, value.assigned());
        }
        RoomAction::SaveGame => save_game(ctx)?,
        RoomAction::SaveDeathTime => save::save_counters(ctx)?,
        // Confirmed (recorder, creation-code.md): moves the kid when one exists (showdown
        // a3-t18-a rows 42-43), no visible effect without one (a3-t20-a).
        RoomAction::MoveKid { x, y } => {
            if let Some(kid) = ctx.w.kid(&ctx.rules.tick)
                && let Some(inst) = ctx.w.get_mut(kid)
            {
                inst.m.set_x(*x)?;
                inst.m.set_y(*y)?;
            }
        }
        RoomAction::Destroy { kind } => {
            // Event order, as a snapshot (Inferred: the order of `with` is not in the spec; it
            // only decides the order of the Destroy events).
            let mut ids: Vec<(u16, InstId)> = ctx
                .w
                .iter_alive()
                .filter(|i| i.active && i.kind == *kind)
                .map(|i| (i.rank, i.id))
                .collect();
            ids.sort_unstable();
            for (_, id) in ids {
                if ctx.destroy(id).is_halt() {
                    return Err(halted_error(ctx));
                }
            }
        }
        RoomAction::PlaySound { sound, looping } => crate::sound::play(ctx, *sound, *looping)?,
        RoomAction::StopSound { sound } => crate::sound::stop(ctx, *sound),
        RoomAction::When { when, action } => {
            if holds(ctx, when) {
                run_action(ctx, host, action, context)?;
            }
        }
    }
    Ok(())
}

/// Whether a row's `when` holds now (#629).
fn holds(ctx: &Ctx, when: &RoomWhen) -> bool {
    let below = when.difficulty_below.is_none_or(|n| {
        crate::num::gm_lt(
            Real::new(f64::from(ctx.w.engine.run.difficulty)),
            n,
            &ctx.rules.num,
        )
    });
    let kid = when
        .kid_exists
        .is_none_or(|want| ctx.w.kid(&ctx.rules.tick).is_some() == want);
    let absent = when
        .kind_absent
        .is_none_or(|k| !ctx.w.iter_alive().any(|i| i.active && i.kind == k));
    below && kid && absent
}

/// The index of an indexed `set_global`: a whole number from 0 (OPEN: what GM does with a
/// fractional or negative index is not in the spec, so the sim stops there).
fn index_value(ctx: &Ctx, index: GlobalIndex) -> Result<u32, SimError> {
    let v = match index {
        GlobalIndex::Const(i) => return Ok(i),
        GlobalIndex::Savenum => return Ok(u32::from(ctx.w.engine.run.slot)),
        GlobalIndex::Global(var) => match ctx.w.globals.get(&var) {
            Some(Value::Real(r)) => r.get(),
            other => {
                return Err(SimError::unsupported(format!(
                    "set_global index: the index global holds {other:?}, not a number"
                )));
            }
        },
    };
    if v.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&v) {
        // Checked just above: a whole number in 0..=u32::MAX.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        return Ok(v as u32);
    }
    Err(SimError::unsupported(format!(
        "set_global index {v}: not a whole number from 0 (OPEN)"
    )))
}

/// The id of a variable name first seen at run time (docs/architecture/sim/world.md): the load-time id if the
/// package has one, else the world's, else a new one after every id in use.
fn run_time_var(ctx: &mut Ctx, name: &str) -> Result<VarId, SimError> {
    if let Some(v) = ctx.def.var_names.get(name).or_else(|| ctx.w.var_names.get(name)) {
        return Ok(*v);
    }
    let next = ctx
        .def
        .var_names
        .values()
        .chain(ctx.w.var_names.values())
        .map(|v| v.0)
        .max()
        .map_or(Some(0), |m| m.checked_add(1))
        .ok_or_else(|| SimError::invariant("run-time variable ids overflow"))?;
    ctx.w.var_names.insert(name.to_owned(), VarId(next));
    Ok(VarId(next))
}

/// `save_game` in room creation code: the save point's save from the first kid with the
/// game's `[save]` model; with no kid, x 0 and y 0 and no error (Confirmed (recorder),
/// kid/A3.md "Saves", showdown a3-t20-a f3-4).
fn save_game(ctx: &mut Ctx) -> Result<(), SimError> {
    let rules = &ctx.physics().save;
    let model = rules.position.model();
    let fixed = rules.fixed.map(|[x, y]| (x, y));
    match ctx.w.kid(&ctx.rules.tick) {
        Some(kid) => save::save_now(ctx, kid, model, fixed),
        None => {
            save::save_without_kid(ctx);
            Ok(())
        }
    }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::Sim;
    use crate::SimOptions;
    use crate::def::game::{EnterRoomPolicy, NestedRequests};
    use crate::def::testing::GameDefBuilder;
    use crate::def::{Builtin, KindDef, KindId};
    use crate::sound::DllMode;

    /// A one-room game whose creation code is `rows`, with `stage` interned at load as id 0.
    fn alias_sim(rows: Vec<RoomAction>) -> Sim {
        let mut b = GameDefBuilder::new("alias");
        let mut room = RoomDef::new("stage", 800, 608);
        room.on_create = rows;
        let stage = b.room(room);
        b.start_room(stage);
        let mut def = b.build().unwrap();
        def.var_names.insert("stage".to_owned(), VarId(0));
        Sim::new(
            Arc::new(def),
            SimOptions {
                start: StartPoint::Room {
                    room: stage,
                    difficulty: 0,
                },
                seed: 0,
                dll_mode: DllMode::Model,
                mode: PlayMode::Original,
                map: None,
            },
        )
        .unwrap()
    }

    fn indexed(index: u32, value: f64) -> RoomAction {
        RoomAction::SetGlobalIndexed {
            var: "stage".to_owned(),
            index: GlobalIndex::Const(index),
            value: Value::Real(Real::new(value)),
        }
    }

    fn bare(value: f64) -> RoomAction {
        RoomAction::SetGlobal {
            var: VarId(0),
            value: Value::Real(Real::new(value)),
            difficulty: None,
        }
    }

    /// GM8 aliases a global and its element 0 (research#130): `global.stage[0] = 7` is
    /// `global.stage = 7`, and no `stage[0]` variable appears.
    #[test]
    fn writing_index_zero_writes_the_global() {
        let sim = alias_sim(vec![indexed(0, 7.0)]);
        let w = sim.world();
        assert_eq!(w.globals.get(&VarId(0)), Some(&Value::Real(Real::new(7.0))));
        assert!(!w.var_names.contains_key("stage[0]"), "{:?}", w.var_names);
    }

    /// The reverse holds too: a bare write is the element 0 that index 0 overwrites, while
    /// element 1 stays its own run-time variable.
    #[test]
    fn writing_the_global_is_writing_index_zero() {
        let sim = alias_sim(vec![bare(5.0), indexed(0, 9.0), indexed(1, 8.0)]);
        let w = sim.world();
        assert_eq!(w.globals.get(&VarId(0)), Some(&Value::Real(Real::new(9.0))));
        let one = *w.var_names.get("stage[1]").expect("element 1 is a variable");
        assert_eq!(w.globals.get(&one), Some(&Value::Real(Real::new(8.0))));
        assert!(!w.var_names.contains_key("stage[0]"), "{:?}", w.var_names);
    }

    // ---- Nested room requests and the creation-code destroy context (#1256) ----

    /// A host that raises a room request when a named kind is built (after its Create) or
    /// reaches Room Start.
    #[derive(Default)]
    struct RequestingHost {
        on_create: Vec<(&'static str, RoomRequest)>,
        on_room_start: Vec<(&'static str, RoomRequest)>,
    }

    fn request_in(list: &[(&'static str, RoomRequest)], ctx: &Ctx, me: InstId) -> Option<RoomRequest> {
        let name = ctx
            .w
            .get(me)
            .and_then(|i| ctx.def.kind(i.kind))
            .map(|k| k.name.as_str())?;
        list.iter().find(|(n, _)| *n == name).map(|(_, req)| *req)
    }

    impl RoomHost for RequestingHost {
        fn dispatch_order(
            &mut self,
            w: &World,
            _rules: &TickRules,
            filter: &dyn Fn(&Instance) -> bool,
        ) -> Vec<InstId> {
            let mut v: Vec<(u16, InstId)> = w
                .instances()
                .iter()
                .filter(|i| filter(i))
                .map(|i| (i.rank, i.id))
                .collect();
            v.sort();
            v.into_iter().map(|(_, id)| id).collect()
        }

        fn dispatch(&mut self, ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow {
            let request = matches!(ev, EventKind::RoomStart)
                .then(|| request_in(&self.on_room_start, ctx, me))
                .flatten();
            if let Some(req) = request {
                ctx.request_room(req);
            }
            Flow::Continue
        }

        fn spawn(
            &mut self,
            ctx: &mut Ctx,
            kind: KindId,
            x: Real,
            y: Real,
            init: SpawnInit,
        ) -> Result<InstId, SimError> {
            let id = ctx.spawn(kind, x, y, init)?;
            let request = request_in(&self.on_create, ctx, id);
            if let Some(req) = request {
                ctx.request_room(req);
            }
            Ok(id)
        }

        fn load_save(&mut self, ctx: &mut Ctx, slot: &SaveSlot) -> Result<InstId, SimError> {
            crate::save::load_into(ctx, slot)
        }
    }

    fn with_ctx<T>(
        def: &crate::def::GameDef,
        rules: &crate::rules::RunnerRules,
        w: &mut World,
        f: impl FnOnce(&mut Ctx) -> T,
    ) -> T {
        let scripts = crate::script::ScriptRuntime::default();
        let mut ctx = Ctx {
            def,
            rules,
            w,
            scripts: &scripts,
            dll_mode: DllMode::Stub,
        };
        f(&mut ctx)
    }

    /// The rooms entered this tick, in order.
    fn entered(w: &World) -> Vec<RoomId> {
        w.output
            .iter()
            .filter_map(|e| match e {
                OutputEvent::Room(RoomOutput::Entered { to, .. }) => Some(*to),
                _ => None,
            })
            .collect()
    }

    /// A neutral start room, then `from`, `b` and `c`. `from` places `first`, which requests
    /// `b` in Create, and `second`, which requests `c` in Room Start.
    fn request_game() -> (crate::def::GameDef, [RoomId; 4], crate::rules::RunnerRules) {
        let mut b = GameDefBuilder::new("nested");
        let first = b.kind(KindDef::new("first", Builtin::Inert));
        let mut second = KindDef::new("second", Builtin::Inert);
        second.events.room_start = true;
        let second = b.kind(second);
        let start = b.room(RoomDef::new("start", 800, 608));
        let from = b.room(RoomDef::new("from", 800, 608));
        let to_b = b.room(RoomDef::new("b", 800, 608));
        let to_c = b.room(RoomDef::new("c", 800, 608));
        b.start_room(start);
        b.place(from, first, 0.0, 0.0).unwrap();
        b.place(from, second, 2.0, 0.0).unwrap();
        (
            b.build().unwrap(),
            [start, from, to_b, to_c],
            crate::rules::RunnerRules::default(),
        )
    }

    fn run_change(
        def: &crate::def::GameDef,
        rules: &crate::rules::RunnerRules,
        w: &mut World,
        rooms: [RoomId; 4],
        host: &mut RequestingHost,
        policy: EnterRoomPolicy,
    ) {
        let [start, from, _, _] = rooms;
        with_ctx(def, rules, w, |ctx| {
            start_game_with(
                ctx,
                &StartPoint::Room {
                    room: start,
                    difficulty: 0,
                },
                host,
            )
            .unwrap();
        });
        w.output.clear();
        with_ctx(def, rules, w, |ctx| {
            change_inner_with(ctx, RoomRequest::Goto(from), host, policy).unwrap();
        });
    }

    /// `Queue`: the request from `first`'s Create and the one from `second`'s Room Start both
    /// run, in raise order.
    #[test]
    fn queued_nested_requests_run_in_raise_order() {
        let (def, rooms, rules) = request_game();
        let [_, from, to_b, to_c] = rooms;
        let mut w = World::new(rooms[0], 1);
        let mut host = RequestingHost {
            on_create: vec![("first", RoomRequest::Goto(to_b))],
            on_room_start: vec![("second", RoomRequest::Goto(to_c))],
        };
        let policy = EnterRoomPolicy {
            nested_requests: NestedRequests::Queue,
            ..Default::default()
        };
        run_change(&def, &rules, &mut w, rooms, &mut host, policy);
        assert_eq!(entered(&w), [from, to_b, to_c]);
    }

    /// `LastWins` (gm8): the world's one pending slot holds the last request, so only `c` runs.
    #[test]
    fn last_wins_runs_only_the_last_request() {
        let (def, rooms, rules) = request_game();
        let [_, from, _, to_c] = rooms;
        let mut w = World::new(rooms[0], 1);
        let mut host = RequestingHost {
            on_create: vec![("first", RoomRequest::Goto(rooms[2]))],
            on_room_start: vec![("second", RoomRequest::Goto(to_c))],
        };
        run_change(&def, &rules, &mut w, rooms, &mut host, EnterRoomPolicy::default());
        assert_eq!(entered(&w), [from, to_c]);
    }

    /// The creation code's `instance_destroy()` has no calling instance, so the explicit context
    /// still destroys nothing (R24); the room's placed instance survives.
    #[test]
    fn creation_code_destroy_has_no_target() {
        let mut b = GameDefBuilder::new("destroy-self");
        let keep = b.kind(KindDef::new("keep", Builtin::Inert));
        let mut room = RoomDef::new("r", 800, 608);
        room.on_create = vec![RoomAction::DestroySelf];
        let room = b.room(room);
        b.start_room(room);
        b.place(room, keep, 0.0, 0.0).unwrap();
        let def = b.build().unwrap();
        let rules = crate::rules::RunnerRules::default();
        let mut w = World::new(room, 1);
        let mut host = RequestingHost::default();
        with_ctx(&def, &rules, &mut w, |ctx| {
            start_game_with(ctx, &StartPoint::Room { room, difficulty: 0 }, &mut host).unwrap();
        });
        assert_eq!(
            w.instances().iter().filter(|i| i.alive).count(),
            1,
            "no calling instance, no target (R24)"
        );
    }
}
