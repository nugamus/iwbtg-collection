//! The retry and reset lifecycle events (docs/architecture/sim/rooms.md, R16, R17, R26, R27;
//! decision 0005, #1149).
//!
//! The policies are [`crate::def::game::Lifecycle`] values. The engines' init rooms and world
//! objects are not ported as GM code: the retry's delay is the engine-owned [`InitPhase`]. The
//! `world` built-in (WP6a) calls these at the world's rank, in the phase
//! [`crate::def::game::RetryPolicy::trigger`] names; this module does not look at the trigger.
//! WP0 declared; WP4b owns.
//!
//! Spec: CLONE_SPEC rooms, "game_restart" and "Retry timeline"; kid/A3.md showdown "Restart".
//!
//! Kid-side steps (`RestartModel::kid_steps`): B's +2 to the play timer and saveDeathTime, clearing
//! `frozen` and `frozen2`, and showdown's jump-value reset after the load, through `kid::api`.
//! Showdown's loadGame also resets the trigger arrays (`kid_steps.clear_triggers`). What is **not**
//! here: its laser (reiza), move (idou), shift, rihuto and taru globals, which are game code
//! (the package's own variables, #245 follow-up), and the dormant `global.tjump` (never set to 1,
//! kid/A3.md).

use crate::def::{ClassMask, DeathOnRestart, EngineKind, ResetKind, RestartKind, RoomId, RoomRole};
use crate::error::SimError;
use crate::num::Real;
use crate::room::{RemovedInstances, RoomRequest};
use crate::run_globals::{BootLoad, RunGlobals};
use crate::save::{self, SaveSlot};
use crate::world::{Ctx, Flow, InstId};

/// Runs `GameDef::restart` (the R key), called by the world instance `world`.
///
/// Nothing happens in a room whose role is in `excluded_roles` or that sets `no_restart`. Then by kind:
///
/// - `GameRestartViaBoot` (groups A, B, C): count the death and write the death-only save when
///   `death_on_restart` says so; the kid, if one lives, is moved to the saved spot (on the R tick
///   the old kid already stands there, Confirmed (recorder) rows 42-44), else one is created
///   there; then `game_restart` is requested (it wins over any earlier request, R13) and the
///   engine-owned [`InitPhase`] starts with the retry policy's delay. The game then sits in the
///   Boot-role room (the gm8 profile's shape) for `delay_ticks()` ticks; [`boot_step`] on the
///   last one creates the kid at the saved spot and requests the saved room. With no save it is
///   a plain `game_restart`. A delay of 0 is `Unsupported` (the spec does not say whether the
///   init tick exists at all, R17).
/// - `RoomGotoSaved` (kaenbyou, eat-the-lemon, go-the-spook-house): destroy the kid and the bow,
///   create the kid at the saved spot, request the saved room (skipped in a Title or Menu role,
///   spook's saveExe).
/// - `ShowdownLoad`: the same without the bow, never a `game_restart` (R26, settled: the
///   recorder shows the R tick makes a new kid and the next tick is the reloaded room).
/// - `RecreateInPlace` (keep-celebrating-for-xia): destroy the kid, create it at the saved spot
///   in the current room, then request the saved room.
///
/// `Ok(Flow::AbortTick)` when a room change was requested (R8), else `Continue`.
pub fn run_restart(ctx: &mut Ctx, world: InstId) -> Result<Flow, SimError> {
    let lifecycle = ctx.def.lifecycle();
    let model = lifecycle.retry.clone();
    if excluded(ctx, model.excluded_roles)? || ctx.def.room(ctx.w.room).is_some_and(|r| r.no_restart) {
        return Ok(Flow::Continue);
    }
    let _ = world;
    match model.death_on_restart {
        DeathOnRestart::Nothing => {}
        DeathOnRestart::SaveCounters => {
            ctx.w.engine.run.add_deaths(1)?;
            save::save_counters(ctx)?;
        }
        DeathOnRestart::SaveCountersPerKid => {
            let kids = ctx
                .w
                .iter_alive()
                .filter(|i| i.classes.intersects(ClassMask::KID | ClassMask::KID_FLIPPED))
                .count();
            if kids > 0 {
                let n = u32::try_from(kids).map_err(|_| SimError::overflow("live kids"))?;
                ctx.w.engine.run.add_deaths(n)?;
                save::save_counters(ctx)?;
            }
        }
    }
    kid_steps_before(ctx, model.kid_steps)?;
    let slot = ctx.w.save.clone();
    match model.kind {
        RestartKind::GameRestartViaBoot => {
            let Some(slot) = slot else {
                ctx.request_room(RoomRequest::GameRestart);
                return Ok(Flow::AbortTick);
            };
            if model.delay_ticks() == 0 {
                return Err(SimError::unsupported(
                    "restart with retry_delay_ticks = 0: the init tick's existence is UNKNOWN (R17) and no game sets 0; permanent until a trace (#225)",
                ));
            }
            place_or_create_kid(ctx, &slot, model.flipped_load_dy)?;
            InitPhase::begin(model.delay_ticks()).store(ctx);
            ctx.request_room(RoomRequest::GameRestart);
        }
        RestartKind::RoomGotoSaved | RestartKind::ShowdownLoad | RestartKind::RecreateInPlace => {
            let Some(slot) = slot else {
                // kaenbyou and eat-the-lemon with no save: destroy the kid, restart the room.
                let removed = destroy_kids(ctx, model.kind == RestartKind::RoomGotoSaved)?;
                let room = ctx.w.room;
                ctx.request_room(RoomRequest::GotoAfterRemovals {
                    room,
                    removed: RemovedInstances::new(&removed)?,
                });
                return Ok(Flow::AbortTick);
            };
            // The destroyed kids are pending removal in the original, so the room restart does
            // not create its placed instances with their ids again (#670).
            let removed = destroy_kids(ctx, model.kind == RestartKind::RoomGotoSaved)?;
            let kid = save::load_into(ctx, &slot)?;
            if model.kid_steps.reset_jump_values {
                crate::kid::api::reset_jump_values(ctx, kid)?;
            }
            if model.kid_steps.clear_triggers {
                ctx.w.engine.mech.clear_triggers();
            }
            let role = room_role(ctx, ctx.w.room);
            let skip = model.kind == RestartKind::RoomGotoSaved
                && matches!(role, Some(RoomRole::Title | RoomRole::Menu));
            if !skip {
                ctx.request_room(RoomRequest::GotoAfterRemovals {
                    room: slot.room,
                    removed: RemovedInstances::new(&removed)?,
                });
            }
        }
    }
    Ok(Flow::AbortTick)
}

/// Runs the lifecycle's reset policy (F2), called by the world instance `world`.
///
/// `DeleteTempAndRestart` (groups A, B, C, go-the-spook-house): delete the temp marker, stop every
/// sound (F2 feedback; CLONE_SPEC rooms, kid/A4.md) and request `game_restart` (the save is kept;
/// the globals are cleared by the restart). `GameReset` (kaenbyou, eat-the-lemon gameReset):
/// reset the run globals and go to the Boot-role room (`crate::room::restart_room`). OPEN for
/// `GameReset`: which other globals the original resets, and whether it stops sounds (the
/// script-level globals stay; trace request #225).
pub fn run_reset(ctx: &mut Ctx, world: InstId) -> Result<Flow, SimError> {
    let lifecycle = ctx.def.lifecycle();
    let model = lifecycle.reset.clone();
    if excluded(ctx, model.excluded_roles)? {
        return Ok(Flow::Continue);
    }
    let _ = world;
    ctx.w.engine.run.boot_load = None;
    if model.sounds() == crate::def::game::Persistence::Stop {
        crate::sound::stop_all(ctx);
    }
    match model.kind {
        ResetKind::DeleteTempAndRestart => ctx.request_room(RoomRequest::GameRestart),
        ResetKind::GameReset => {
            ctx.w.engine.run = RunGlobals::default();
            ctx.request_room(RoomRequest::Goto(crate::room::restart_room(ctx.def)));
        }
    }
    Ok(Flow::AbortTick)
}

/// The engine-owned init phase of a retry (decision 0005; #1149).
///
/// The retry's delay is a lifecycle state, not a package room: [`run_restart`] enters the phase
/// on the R tick, [`boot_step`] counts it down one tick at a time, and the tick that reaches
/// zero loads the checkpoint and returns control. The original's temp file is why the phase
/// survives `game_restart`; the sim has no file, so the phase is the engine's stand-in.
///
/// The gm8 compatibility profile still enters the package's Boot room while the phase runs
/// ([`crate::room`] enters the Boot-role room on `game_restart`), because an old replay's state
/// hash observes that room. The phase is not tied to that room: `standard` runs the same phase
/// without a package-authored room once the stable room key lands (#1153).
///
/// The original's temp file also holds the slot number; the sim's slot is
/// [`crate::run_globals::RunGlobals::slot`], which `game_restart` resets. Carrying the slot in
/// the phase needs the run-globals field and its hash entry, which the identity-save seat owns
/// (follow-up of #1149).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InitPhase {
    /// No retry is waiting.
    Idle,
    /// `ticks_left` init ticks remain, counting the tick that loads.
    Waiting {
        /// Init ticks left.
        ticks_left: u8,
    },
}

impl InitPhase {
    /// The phase a retry with `ticks` leaves behind.
    pub fn begin(ticks: u8) -> InitPhase {
        InitPhase::Waiting { ticks_left: ticks }
    }

    /// The phase the world is in.
    pub fn current(ctx: &Ctx) -> InitPhase {
        ctx.w
            .engine
            .run
            .boot_load
            .map_or(InitPhase::Idle, |boot| InitPhase::Waiting {
                ticks_left: boot.ticks_left,
            })
    }

    /// Stores the phase in the world (the hash observes `RunGlobals::boot_load`).
    pub fn store(self, ctx: &mut Ctx) {
        ctx.w.engine.run.boot_load = match self {
            InitPhase::Idle => None,
            InitPhase::Waiting { ticks_left } => Some(BootLoad { ticks_left }),
        };
    }

    /// Counts one tick: the next phase, and whether this tick is the one that loads. The phase
    /// becomes [`InitPhase::Idle`] on the loading tick, so a retry loads exactly once.
    pub fn advance(self) -> (InitPhase, bool) {
        match self {
            InitPhase::Idle => (InitPhase::Idle, false),
            InitPhase::Waiting { ticks_left } => match ticks_left.saturating_sub(1) {
                0 => (InitPhase::Idle, true),
                left => (InitPhase::Waiting { ticks_left: left }, false),
            },
        }
    }
}

/// One tick of the engine-owned init phase, called by the `world` built-in's Step (the init
/// object's Step in the original, which checks for the temp file there, not in its Create).
/// Does nothing unless a `GameRestartViaBoot` retry is waiting. The last init tick loads the
/// checkpoint (kid at the saved spot, no motion) and requests the saved room. Returns
/// `AbortTick` when it did.
pub fn boot_step(ctx: &mut Ctx) -> Result<Flow, SimError> {
    let (next, load) = InitPhase::current(ctx).advance();
    next.store(ctx);
    if !load {
        return Ok(Flow::Continue);
    }
    let Some(slot) = ctx.w.save.clone() else {
        return Ok(Flow::Continue);
    };
    save::load_into(ctx, &slot)?;
    ctx.request_room(RoomRequest::Goto(slot.room));
    Ok(Flow::AbortTick)
}

/// The kid-side steps that come before the kid is destroyed (`RestartModel::kid_steps`): B's +2
/// to the play timer and saveDeathTime (kid/B.md Restart 3.1), and clearing `frozen` and
/// `frozen2` (B 3.3, showdown 1). The new kid's Create then sees whatever `reverse` the globals
/// hold, because the load creates the kid before it reads the save (kid/B.md Restart, "stale
/// `reverse`"); nothing here touches it.
fn kid_steps_before(ctx: &mut Ctx, steps: crate::def::RetryKidSteps) -> Result<(), SimError> {
    if steps.release_death_sound {
        crate::builtins::death_sound::release(ctx)?;
    }
    if steps.add_time > 0 {
        ctx.w.engine.run.add_time(steps.add_time)?;
    }
    if steps.save_death_time {
        save::save_counters(ctx)?;
    }
    if steps.clear_frozen {
        crate::kid::api::clear_frozen(ctx);
    }
    Ok(())
}

fn excluded(ctx: &Ctx, roles: crate::def::RoleSet) -> Result<bool, SimError> {
    Ok(room_role(ctx, ctx.w.room).is_some_and(|r| roles.contains(r)))
}

fn room_role(ctx: &Ctx, room: RoomId) -> Option<RoomRole> {
    ctx.def.room(room).map(|r| r.role)
}

/// On the R tick the live kid stands at the saved spot already; with no kid one is created
/// there (loadGame creates one at (0, 0) and saveExe moves it).
fn place_or_create_kid(ctx: &mut Ctx, slot: &SaveSlot, flipped_load_dy: Real) -> Result<(), SimError> {
    let rules = ctx.rules;
    match ctx.w.kid(&rules.tick) {
        Some(kid) => {
            let dy = if save::flip_by_kind(ctx, slot) {
                flipped_load_dy
            } else {
                crate::num::Real::ZERO
            };
            if let Some(inst) = ctx.w.get_mut(kid) {
                inst.m.set_x(slot.x)?;
                inst.m.set_y(slot.y + dy)?;
            }
        }
        None => {
            save::load_into(ctx, slot)?;
        }
    }
    Ok(())
}

/// Destroys every kid (both kinds) and, when `with_bow`, the bow. Returns the destroyed ids:
/// the room change in the same tick must not create placed instances with those ids (#670).
fn destroy_kids(ctx: &mut Ctx, with_bow: bool) -> Result<Vec<InstId>, SimError> {
    let bow = ctx.def.engine_kind(EngineKind::Bow);
    let ids: Vec<InstId> = ctx
        .w
        .iter_alive()
        .filter(|i| {
            i.classes
                .intersects(crate::def::ClassMask::KID | crate::def::ClassMask::KID_FLIPPED)
                || (with_bow && Some(i.kind) == bow)
        })
        .map(|i| i.id)
        .collect();
    for &id in &ids {
        if ctx.destroy(id).is_halt() {
            return Err(ctx.halted_error());
        }
    }
    Ok(ids)
}
