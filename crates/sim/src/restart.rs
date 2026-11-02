//! Restart and reset models (docs/plans/sim-core.md 4.3, R16, R17, R26, R27).
//!
//! The engines' init rooms and world objects are not ported as GM code. The `world` built-in
//! (WP6a) calls these at the world's rank, in the phase `RestartModel::trigger` names; this
//! module does not look at the trigger. WP0 declared; WP4b owns.
//!
//! Spec: CLONE_SPEC rooms, "game_restart" and "Retry timeline"; kid/A3.md showdown "Restart".
//!
//! Kid-side steps (`RestartModel::kid_steps`): B's +2 to the play timer and saveDeathTime, clearing
//! `frozen` and `frozen2`, and showdown's jump-value reset after the load, through `kid::api`.
//! What is **not** here: showdown's resets of the trigger, laser, move, shift, rihuto, tjump and
//! taru globals (the mechanics' state, WP6; filed as an issue), and sounds on F2 (WP10).

use crate::def::{ClassMask, DeathOnRestart, EngineKind, ResetKind, RestartKind, RoomId, RoomRole};
use crate::error::SimError;
use crate::room::RoomRequest;
use crate::run_globals::{BootLoad, RunGlobals};
use crate::save::{self, SaveSlot};
use crate::world::{Ctx, Flow, InstId};

/// Runs `GameDef::restart` (the R key), called by the world instance `world`.
///
/// Nothing happens in a room whose role is in `excluded_roles`. Then by kind:
///
/// - `GameRestartViaBoot` (groups A, B, C): count the death and write the death-only save when
///   `death_on_restart` says so; the kid, if one lives, is moved to the saved spot (on the R tick
///   the old kid already stands there, Confirmed (recorder) rows 42-44), else one is created
///   there; then `game_restart` is requested (it wins over any earlier request, R13) and the
///   boot marker is set. The game then sits in the boot room for `boot_ticks` ticks;
///   [`boot_step`] on the last one creates the kid at the saved spot and requests the saved
///   room. With no save it is a plain `game_restart`. `boot_ticks == 0` is `Unsupported` (the
///   spec does not say whether the boot tick exists at all, R17).
/// - `RoomGotoSaved` (kaenbyou, eat-the-lemon, go-the-spook-house): destroy the kid and the bow,
///   create the kid at the saved spot, request the saved room (skipped in a Title or Menu role,
///   spook's saveExe).
/// - `ShowdownLoad`: the same without the bow, never a `game_restart`. R26: the rooms section
///   lists showdown in Group A (loadGame then game_restart), but kid/A3.md read showdown's
///   scripts in full and says "There is no game_restart"; this follows kid/A3.md. Reported to
///   research.
/// - `RecreateInPlace` (keep-celebrating-for-xia): destroy the kid, create it at the saved spot
///   in the current room, then request the saved room.
///
/// `Ok(Flow::AbortTick)` when a room change was requested (R8), else `Continue`.
pub fn run_restart(ctx: &mut Ctx, world: InstId) -> Result<Flow, SimError> {
    let model = ctx.def.restart.clone();
    if excluded(ctx, model.excluded_roles)? {
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
            if model.boot_ticks == 0 {
                return Err(SimError::unsupported(
                    "restart with boot_ticks = 0: whether the boot tick exists is UNKNOWN (R17)",
                ));
            }
            place_or_create_kid(ctx, &slot)?;
            ctx.w.engine.run.boot_load = Some(BootLoad {
                ticks_left: model.boot_ticks,
            });
            ctx.request_room(RoomRequest::GameRestart);
        }
        RestartKind::RoomGotoSaved | RestartKind::ShowdownLoad | RestartKind::RecreateInPlace => {
            let Some(slot) = slot else {
                // kaenbyou and eat-the-lemon with no save: destroy the kid, restart the room.
                destroy_kids(ctx, model.kind == RestartKind::RoomGotoSaved)?;
                ctx.request_room(RoomRequest::Restart);
                return Ok(Flow::AbortTick);
            };
            destroy_kids(ctx, model.kind == RestartKind::RoomGotoSaved)?;
            let kid = save::load_into(ctx, &slot)?;
            if model.kid_steps.reset_jump_values {
                crate::kid::api::reset_jump_values(ctx, kid)?;
            }
            let role = room_role(ctx, ctx.w.room);
            let skip = model.kind == RestartKind::RoomGotoSaved
                && matches!(role, Some(RoomRole::Title | RoomRole::Menu));
            if !skip {
                ctx.request_room(RoomRequest::Goto(slot.room));
            }
        }
    }
    Ok(Flow::AbortTick)
}

/// Runs `GameDef::reset` (F2), called by the world instance `world`.
///
/// `DeleteTempAndRestart` (groups A, B, C): delete the temp marker and request `game_restart`
/// (the save is kept; the globals are cleared by the restart). `GameReset` (kaenbyou,
/// eat-the-lemon gameReset): reset the run globals and go to the boot room (the first room with
/// the `Boot` role, else room 0). OPEN for `GameReset`: which other globals the original
/// resets (the script-level globals stay).
pub fn run_reset(ctx: &mut Ctx, world: InstId) -> Result<Flow, SimError> {
    let model = ctx.def.reset.clone();
    if excluded(ctx, model.excluded_roles)? {
        return Ok(Flow::Continue);
    }
    let _ = world;
    ctx.w.engine.run.boot_load = None;
    match model.kind {
        ResetKind::DeleteTempAndRestart => ctx.request_room(RoomRequest::GameRestart),
        ResetKind::GameReset => {
            ctx.w.engine.run = RunGlobals::default();
            let boot = ctx
                .def
                .rooms
                .iter()
                .position(|r| r.role == RoomRole::Boot)
                .and_then(|i| u32::try_from(i).ok())
                .unwrap_or(0);
            ctx.request_room(RoomRequest::Goto(RoomId(boot)));
        }
    }
    Ok(Flow::AbortTick)
}

/// One boot-room tick of a retry, called by the `world` built-in's Step (the init object's Step
/// in the original, which checks for the temp file there, not in its Create). Does nothing
/// unless a `GameRestartViaBoot` retry is waiting. The last boot tick loads the save (kid at
/// the saved spot, no motion) and requests the saved room. Returns `AbortTick` when it did.
pub fn boot_step(ctx: &mut Ctx) -> Result<Flow, SimError> {
    let Some(mut boot) = ctx.w.engine.run.boot_load else {
        return Ok(Flow::Continue);
    };
    boot.ticks_left = boot.ticks_left.saturating_sub(1);
    if boot.ticks_left > 0 {
        ctx.w.engine.run.boot_load = Some(boot);
        return Ok(Flow::Continue);
    }
    ctx.w.engine.run.boot_load = None;
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
fn place_or_create_kid(ctx: &mut Ctx, slot: &SaveSlot) -> Result<(), SimError> {
    let rules = ctx.rules;
    match ctx.w.kid(&rules.tick) {
        Some(kid) => {
            let dy = if slot.flipped {
                ctx.def.restart.flipped_load_dy
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

/// Destroys every kid (both kinds) and, when `with_bow`, the bow.
fn destroy_kids(ctx: &mut Ctx, with_bow: bool) -> Result<(), SimError> {
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
    for id in ids {
        if ctx.destroy(id).is_halt() {
            return Err(ctx.halted_error());
        }
    }
    Ok(())
}
