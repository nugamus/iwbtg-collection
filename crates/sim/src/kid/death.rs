//! Kid death per family: sound, blood emitter, destroy, deaths +1, death-only save, GAMEOVER
//! (docs/plans/sim-core.md 5.3 "Death"). Called through `kid::api::kill`. WP5a owns it.
//!
//! Spec: kid/A1.md "Death (killPlayer)".
//!
//! 1. In a `Select` room (`death.select_room_reset`): request the same room again and destroy
//!    the kid; nothing else happens, and the tick aborts for the room change.
//! 2. While `muteki` is set, nothing happens.
//! 3. Otherwise the steps of `death.order` run: the sound (`[sounds] death`), a blood
//!    emitter at the kid's position (its `flipped` param says whether the kid was the flipped
//!    one), destroy the kid (`destroy_all_kids`: every kid; `first_kid`: the effects use the
//!    first kid's position and gravity, family B), GAMEOVER (it places itself at view 0 plus its own offset),
//!    `global.death + 1` (checked), the death-only save.
//!
//! A kind the game does not have (no blood emitter, no GAMEOVER) is skipped.

use serde::Deserialize;

use super::api::KillCause;
use crate::def::physics::DeathStep;
use crate::def::room::RoomRole;
use crate::def::{ClassMask, EngineKind, ParamValue};
use crate::error::SimError;
use crate::room::RoomRequest;
use crate::save;
use crate::world::{Ctx, Flow, InstId, KindState, SpawnInit};

/// The collision and key runs of this file, by their `physics.toml` names. Part of
/// `kid::KidCollisionRun`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeathRun {
    /// `{ with = "killer", run = "kill" }`: the other instance kills the kid.
    Kill,
}

/// Runs a death collision run: the kid `me` meets the killer `other`.
pub fn run(ctx: &mut Ctx, me: InstId, other: InstId, run: &DeathRun) -> Result<Flow, SimError> {
    match run {
        DeathRun::Kill => kill(ctx, me, KillCause::Killer(other)),
    }
}

/// Kills the kid `kid` by the profile's `[death]` rules. `Flow::AbortTick` when the Select-room
/// reset requested a room; `Flow::Halt` when a destroy halted the sim; an error when `kid` is not
/// a kid. A kid that no longer exists is left alone.
pub fn kill(ctx: &mut Ctx, kid: InstId, _cause: KillCause) -> Result<Flow, SimError> {
    let def = ctx.def;
    let rules = &def.physics.death;
    let Some(inst) = ctx.w.get(kid).filter(|i| i.alive) else {
        return Ok(Flow::Continue);
    };
    let KindState::Kid(s) = &inst.state else {
        return Err(SimError::invariant(format!(
            "kid::death::kill: instance {} is not a kid",
            kid.0
        )));
    };
    let invincible = s.invincible;
    let mut flipped = s.flipped || inst.classes.intersects(ClassMask::KID_FLIPPED);
    let (mut x, mut y) = (inst.m.x(), inst.m.y());
    let in_select = ctx
        .w
        .room
        .index()
        .and_then(|i| def.rooms.get(i))
        .is_some_and(|r| r.role == RoomRole::Select);
    if rules.select_room_reset && in_select {
        ctx.request_room(RoomRequest::Goto(ctx.w.room));
        let flow = ctx.destroy(kid);
        return Ok(if flow.is_halt() { flow } else { Flow::AbortTick });
    }
    if invincible {
        return Ok(Flow::Continue);
    }
    if rules.stop_music {
        return Err(SimError::unsupported(
            "kid death: death.stop_music (WP10 sound model)",
        ));
    }
    // Family B with several kids: the effects sit on the first kid, not on the one that died.
    if rules.first_kid
        && let Some(first) = ctx.w.kid(&ctx.rules.tick)
        && let Some(i) = ctx.w.get(first)
    {
        (x, y) = (i.m.x(), i.m.y());
        flipped = matches!(&i.state, KindState::Kid(s) if s.flipped)
            || i.classes.intersects(ClassMask::KID_FLIPPED);
    }
    for step in &rules.order {
        match step {
            DeathStep::Sound => {
                super::actions::common::play_sound(ctx, ctx.def.physics.sounds.death)?;
            }
            DeathStep::BloodEmitter => {
                let mut init = SpawnInit::default();
                init.params.0.insert("flipped".into(), ParamValue::Bool(flipped));
                ctx.spawn_engine(EngineKind::BloodEmitter, x, y, init)?;
            }
            DeathStep::DestroyAllKids => {
                let ids: Vec<InstId> = ctx
                    .w
                    .iter_alive()
                    .filter(|i| matches!(i.state, KindState::Kid(_)))
                    .map(|i| i.id)
                    .collect();
                for id in ids {
                    let flow = ctx.destroy(id);
                    if flow.is_halt() {
                        return Ok(flow);
                    }
                }
            }
            DeathStep::DestroyKid => {
                let flow = ctx.destroy(kid);
                if flow.is_halt() {
                    return Ok(flow);
                }
            }
            DeathStep::GameOver => {
                ctx.spawn_engine(EngineKind::GameOver, x, y, SpawnInit::default())?;
            }
            DeathStep::DeathsPlusOne => {
                let run = &mut ctx.w.engine.run;
                run.deaths = run
                    .deaths
                    .checked_add(1)
                    .ok_or_else(|| SimError::overflow("global.death"))?;
            }
            DeathStep::SaveCounters => save::save_counters(ctx)?,
        }
    }
    Ok(Flow::Continue)
}
