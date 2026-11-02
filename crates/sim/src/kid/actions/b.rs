//! Kid actions specific to family B (kid/B.md: the `cur_jumps` jump, the polling Alarm 0 cut;
//! docs/plans/sim-core.md 5.3). WP5b owns it. `slope_b`, `set_view_angle` and the yflag flip are
//! later sub-issues of #14.

use serde::Deserialize;

use super::common::{down, set_sprite, state, state_mut};
use crate::alarm;
use crate::collide::query::{Target, place_meeting};
use crate::def::ClassMask;
use crate::def::physics::{GroundAlso, JumpCancel, WaterAir};
use crate::error::SimError;
use crate::input::Action;
use crate::num::{Real, gm_lt};
use crate::world::{Ctx, Flow, InstId};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BAction {
    /// Alarm 0 with jump cancelling off (`jump.cancel = "poll_alarm"`): while Jump is up and the
    /// kid moves against gravity, `vspeed` times `release_mult` and the alarm stops; else the
    /// alarm is armed again for 1 tick.
    CancelPoll,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &BAction) -> Result<Flow, SimError> {
    match action {
        BAction::CancelPoll => cancel_poll(ctx, me)?,
    }
    Ok(Flow::Continue)
}

/// `playerJump` for `jump.air = "cur_jumps"` (kid/B.md playerJump, kaenbyou version). The caller
/// has checked `frozen` and that Jump was pressed.
///
/// 1. With `jump.blocked_by_pause_press`, a tick with a Pause press does nothing.
/// 2. First jump: a `ground_classes` instance 1 px in the gravity direction, or `onPlatform`
///    (`ground_also`): `vspeed = -yflag * jump[1]`, `curJumps = 2`; with `cancel = "poll_alarm"`
///    Alarm 0 is armed for 1 tick.
/// 3. Otherwise `curJumps` 1 becomes 2 (walking off a ledge spends the first jump); when
///    `curJumps <= maxJumps`: `vspeed = -yflag * jump[curJumps]`, the jump sprite, `curJumps + 1`;
///    else with `infJump`, or water under `water_air = "water_unlimited"`: `vspeed = -yflag *
///    jump[maxJumps]` and `curJumps = maxJumps + 1`.
pub(crate) fn jump(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let p = &def.physics;
    if p.jump.blocked_by_pause_press && ctx.w.input.pressed(Action::Pause) {
        return Ok(());
    }
    let s = state(ctx, me)?;
    let sign = Real::new(f64::from(down(&s)));
    let Some(i) = ctx.w.get(me) else { return Ok(()) };
    let (x, y) = (i.m.x(), i.m.y());
    let below = y + sign;
    let mut first = place_meeting(ctx, me, x, below, Target::Class(p.jump.ground_classes))?;
    for extra in &p.jump.ground_also {
        first |= match extra {
            GroundAlso::PlatformFlag => s.on_platform,
            GroundAlso::LadderJump => s.ladder_jump,
        };
    }
    // `jump[n]` is 1-based in the original: `jump_speeds[n - 1]` here.
    let speed = |n: u8| {
        usize::from(n)
            .checked_sub(1)
            .and_then(|i| s.jump_speeds.get(i))
            .copied()
            .unwrap_or(Real::ZERO)
    };
    let rules = ctx.rules;
    if first {
        if let Some(i) = ctx.w.get_mut(me) {
            i.m.set_vspeed(-(sign * speed(1)))?;
            if p.jump.cancel == JumpCancel::PollAlarm {
                alarm::set(i, 0, Real::ONE, &rules.num)?;
            }
        }
        state_mut(ctx, me)?.cur_jumps = 2;
        return Ok(());
    }
    let max = s.b.max_jumps;
    let mut cur = s.cur_jumps;
    if cur == 1 {
        cur = 2;
    }
    if cur <= max {
        if let Some(i) = ctx.w.get_mut(me) {
            i.m.set_vspeed(-(sign * speed(cur)))?;
            set_sprite(def, &mut i.img, p.sprites.jump);
        }
        state_mut(ctx, me)?.cur_jumps = cur.saturating_add(1);
        return Ok(());
    }
    let in_water = match p.jump.water_air {
        WaterAir::WaterUnlimited => place_meeting(ctx, me, x, below, Target::Class(ClassMask::WATER1))?,
        _ => false,
    };
    if s.b.inf_jump || in_water {
        if let Some(i) = ctx.w.get_mut(me) {
            i.m.set_vspeed(-(sign * speed(max)))?;
        }
        state_mut(ctx, me)?.cur_jumps = max.saturating_add(1);
    } else {
        state_mut(ctx, me)?.cur_jumps = cur;
    }
    Ok(())
}

/// Alarm 0 with jump cancelling off (kid/B.md Alarm 0).
fn cancel_poll(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let n = &ctx.rules.num;
    let s = state(ctx, me)?;
    let held = ctx.w.input.held(Action::Jump);
    let rules = ctx.rules;
    let Some(i) = ctx.w.get_mut(me) else { return Ok(()) };
    let against = gm_lt(Real::new(f64::from(down(&s))) * i.m.vspeed(), Real::ZERO, n);
    if !held && against {
        i.m.set_vspeed(i.m.vspeed() * def.physics.numbers.release_mult)?;
        alarm::set(i, 0, Real::ZERO, &rules.num)?;
    } else {
        alarm::set(i, 0, Real::ONE, &rules.num)?;
    }
    Ok(())
}
