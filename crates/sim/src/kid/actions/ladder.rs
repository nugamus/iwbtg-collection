//! The `ladder`, `ladder_sprites` and `ladder_jump_reset` actions (docs/plans/sim-core.md 5.3,
//! kid/A1.md Step action 2 and End Step). WP5c owns it. The ladder object itself is the
//! `ladder` built-in (WP6c); the kid only needs its class.

use serde::Deserialize;

use super::common::{set_sprite, state, state_mut};
use crate::collide::query::{Target, place_meeting};
use crate::def::ClassMask;
use crate::def::physics::LadderEntry;
use crate::error::SimError;
use crate::input::Action;
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LadderAction {
    /// Step action 2, instead of `gravity` (it includes the gravity reset, so the two are never
    /// listed together): `ladderud` clears; Up or Down (`[ladder] entry`) held, off a ladder and
    /// overlapping one: on the ladder with `djump`; on a ladder with none overlapping any more:
    /// off; otherwise Up moves `y` up by the run speed, Down moves it down, each with gravity and
    /// `vspeed` 0 and `ladderud` 1, and Jump held leaves with `ladderjump`. Off the ladder
    /// gravity returns to `numbers.gravity`. The keys are direct reads.
    Ladder,
    /// End Step: on a ladder the `back` sprite (`image_speed` 0.1), moving on it the
    /// `ladder_up_down` sprite (0.2).
    LadderSprites,
    /// End Step: `ladderjump` clears (rori has only this part).
    LadderJumpReset,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &LadderAction) -> Result<Flow, SimError> {
    match action {
        LadderAction::Ladder => ladder(ctx, me)?,
        LadderAction::LadderSprites => ladder_sprites(ctx, me)?,
        LadderAction::LadderJumpReset => state_mut(ctx, me)?.ladder_jump = false,
    }
    Ok(Flow::Continue)
}

fn ladder(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let p = &def.physics;
    state_mut(ctx, me)?.ladder_ud = false;
    let up = ctx.w.input.direct(Action::Up);
    let down = ctx.w.input.direct(Action::Down);
    let jump = ctx.w.input.direct(Action::Jump);
    let enter = match p.ladder.entry {
        LadderEntry::UpOrDown => up || down,
        LadderEntry::UpOnly => up,
    };
    let Some(i) = ctx.w.get(me) else { return Ok(()) };
    let (x, y) = (i.m.x(), i.m.y());
    let overlap = place_meeting(ctx, me, x, y, Target::Class(ClassMask::LADDER))?;
    if enter && !state(ctx, me)?.ladder && overlap {
        let s = state_mut(ctx, me)?;
        s.ladder = true;
        s.djump = true;
    }
    if state(ctx, me)?.ladder {
        if !overlap {
            state_mut(ctx, me)?.ladder = false;
        } else {
            let step = state(ctx, me)?.run_speed;
            if up {
                move_on_ladder(ctx, me, -step)?;
            }
            if down {
                move_on_ladder(ctx, me, step)?;
            }
            if up || down {
                state_mut(ctx, me)?.ladder_ud = true;
            }
            if jump {
                let s = state_mut(ctx, me)?;
                s.ladder_jump = true;
                s.ladder = false;
            }
        }
    }
    if !state(ctx, me)?.ladder
        && let Some(i) = ctx.w.get_mut(me)
    {
        i.m.set_gravity(p.numbers.gravity)?;
    }
    Ok(())
}

fn move_on_ladder(ctx: &mut Ctx, me: InstId, dy: Real) -> Result<(), SimError> {
    if let Some(i) = ctx.w.get_mut(me) {
        i.m.set_y(i.m.y() + dy)?;
        i.m.set_gravity(Real::ZERO)?;
        i.m.set_vspeed(Real::ZERO)?;
    }
    Ok(())
}

fn ladder_sprites(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let s = state(ctx, me)?;
    let Some(i) = ctx.w.get_mut(me) else { return Ok(()) };
    if s.ladder {
        set_sprite(def, &mut i.img, def.physics.sprites.back);
        i.img.set_speed(Real::new(0.1))?;
    }
    if s.ladder_ud {
        set_sprite(def, &mut i.img, def.physics.sprites.ladder_up_down);
        i.img.set_speed(Real::new(0.2))?;
    }
    Ok(())
}
