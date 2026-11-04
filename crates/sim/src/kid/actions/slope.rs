//! Slope actions: `slope_jump` and `downhill_snap` (docs/plans/sim-core.md 5.3). WP5c owns it.
//! Spec: kid/A1.md (poronkusema, get-the-rainbow-feather, rori, fairy-tale).

use serde::Deserialize;

use super::common::{set_sprite, state, state_mut};
use crate::collide::move_contact_solid;
use crate::collide::query::{Target, place_free, place_meeting};
use crate::def::ClassMask;
use crate::def::physics::Downhill;
use crate::error::SimError;
use crate::input::Action;
use crate::num::{Real, gm_ge};
use crate::world::{Ctx, Flow, InstId};

const BOTH: ClassMask = ClassMask(ClassMask::SLOPE.0 | ClassMask::SLOPE_MIRROR.0);

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlopeAction {
    /// The end of `playerJump` (rainbow, rori, fairy-tale): on a Jump press, with a `slope` or
    /// `slope_mirror` within `[slope] jump_probe` px below, a single jump is forced: `vspeed` is
    /// `-numbers.jumps[0]` and `djump` is set. It runs after the normal jump, so it can override a
    /// double jump of the same press.
    SlopeJump,
    /// End Step, `[slope] downhill` (see `Downhill`).
    DownhillSnap,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &SlopeAction) -> Result<Flow, SimError> {
    match action {
        SlopeAction::SlopeJump => slope_jump(ctx, me)?,
        SlopeAction::DownhillSnap => downhill_snap(ctx, me)?,
    }
    Ok(Flow::Continue)
}

fn missing() -> SimError {
    SimError::invariant("slope action: kid vanished")
}

fn slope_jump(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let probe = ctx.def.physics.slope.jump_probe;
    let s = state(ctx, me)?;
    if s.frozen || ctx.w.engine.kid.frozen || !ctx.w.input.pressed(Action::Jump) {
        return Ok(());
    }
    let i = ctx.w.get(me).ok_or_else(missing)?;
    let (x, y) = (i.m.x(), i.m.y());
    // gm-compare: ok (`probe` is a profile param, 0 turns the rule off; not a GML comparison)
    if probe == Real::ZERO || !place_meeting(ctx, me, x, y + probe, Target::Class(BOTH))? {
        return Ok(());
    }
    let jump0 = s.jump_speeds.first().copied().unwrap_or(Real::ZERO);
    ctx.w.get_mut(me).ok_or_else(missing)?.m.set_vspeed(-jump0)?;
    state_mut(ctx, me)?.djump = true;
    Ok(())
}

fn snap(ctx: &mut Ctx, me: InstId, max: f64) -> Result<(), SimError> {
    move_contact_solid(ctx, me, Real::new(270.0), Real::new(max))
}

fn downhill_snap(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let n = &ctx.rules.num;
    let i = ctx.w.get(me).ok_or_else(missing)?;
    let (x, y, vs, xp) = (i.m.x(), i.m.y(), i.m.vspeed(), i.m.xprevious());
    match def.physics.slope.downhill {
        Downhill::None => {}
        Downhill::Por => {
            if gm_ge(vs, Real::ZERO, n)
                && place_free(ctx, me, x, y + Real::ONE)?
                && state(ctx, me)?.on_slope_last
                && place_meeting(ctx, me, x, y + Real::new(12.0), Target::Class(ClassMask::SLOPE))?
            {
                snap(ctx, me, 12.0)?;
                let moving = ctx.w.input.held(Action::Left) || ctx.w.input.held(Action::Right);
                let spr = if moving {
                    def.physics.sprites.running
                } else {
                    def.physics.sprites.idle
                };
                if let Some(i) = ctx.w.get_mut(me) {
                    set_sprite(def, &mut i.img, spr);
                }
            }
            let i = ctx.w.get(me).ok_or_else(missing)?;
            let (x, y) = (i.m.x(), i.m.y());
            let on = place_meeting(ctx, me, x, y + Real::ONE, Target::Class(ClassMask::SLOPE))?;
            state_mut(ctx, me)?.on_slope_last = on;
        }
        Downhill::Rainbow => {
            for class in [ClassMask::SLOPE, ClassMask::SLOPE_MIRROR] {
                let i = ctx.w.get(me).ok_or_else(missing)?;
                let (x, y) = (i.m.x(), i.m.y());
                if place_meeting(ctx, me, x, y + Real::new(5.0), Target::Class(class))? {
                    snap(ctx, me, 10.0)?;
                }
            }
        }
        Downhill::FairyTale => {
            if !place_meeting(ctx, me, x, y + Real::ONE, Target::Class(ClassMask::BLOCK))?
                && place_meeting(ctx, me, xp, y + Real::new(3.0), Target::Class(BOTH))?
            {
                snap(ctx, me, 10.0)?;
            }
        }
    }
    Ok(())
}
