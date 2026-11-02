//! Kid actions specific to family A3 (kid/A3.md; docs/plans/sim-core.md 5.3). WP5c owns it.
//! Showdown's ice (`ice_temp_hspeed`) is `kid/actions/ice.rs`.

use serde::Deserialize;

use super::common::state;
use crate::def::room::RoomRole;
use crate::error::SimError;
use crate::input::Action;
use crate::kid::api::{KillCause, kill};
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum A3Action {
    /// Step action 4 step 1 (wanna, showdown, rori, fairy-tale): the game's own key flags. Left,
    /// Right and Jump flags become 1 on the key's press tick and 0 on its release tick; they are
    /// not the key's state. With a press and a release in one tick the flag ends 0 (OPEN). The
    /// flags live in `KidGlobalsExt`, so they outlive the kid.
    KeyFlags,
    /// Step action 5 (wanna): the Suicide key held (not a press) outside a Boot, Title, Menu or
    /// Select room runs `killPlayer` every tick.
    SuicideCheck,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &A3Action) -> Result<Flow, SimError> {
    match action {
        A3Action::KeyFlags => {
            key_flags(ctx);
            Ok(Flow::Continue)
        }
        A3Action::SuicideCheck => suicide_check(ctx, me),
    }
}

fn key_flags(ctx: &mut Ctx) {
    let input = &ctx.w.input;
    let update = |flag: &mut bool, a: Action| {
        if input.pressed(a) {
            *flag = true;
        }
        if input.released(a) {
            *flag = false;
        }
    };
    let ext = &mut ctx.w.engine.kid.ext;
    update(&mut ext.key_left, Action::Left);
    update(&mut ext.key_right, Action::Right);
    update(&mut ext.key_jump, Action::Jump);
}

fn suicide_check(ctx: &mut Ctx, me: InstId) -> Result<Flow, SimError> {
    if !ctx.w.input.held(Action::Suicide) {
        return Ok(Flow::Continue);
    }
    let role = ctx
        .w
        .room
        .index()
        .and_then(|i| ctx.def.rooms.get(i))
        .map(|r| r.role);
    if matches!(
        role,
        Some(RoomRole::Boot | RoomRole::Title | RoomRole::Menu | RoomRole::Select)
    ) {
        return Ok(Flow::Continue);
    }
    let _ = state(ctx, me)?;
    kill(ctx, me, KillCause::Suicide)
}
