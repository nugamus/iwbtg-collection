//! The `platform_standard` collision run (docs/plans/sim-core.md 5.3, `[platform]`). WP5a owns
//! it.
//!
//! Spec: kid/A1.md "Collision with platform (platform is not solid)". With
//! `platform.rule = "half_vspeed"`: when `y - vspeed / 2 <= platform.y` (the kid was at or above
//! the top half a step ago) the kid lands: if the platform's `vspeed >= 0`, the kid's `y` becomes
//! `platform.y - snap` and its `vspeed` the platform's; `onPlatform` and `djump` are set either
//! way. Otherwise nothing, so a kid coming from below passes through.

use serde::Deserialize;

use crate::def::physics::{PlatformRule, PlatformTake};
use crate::error::SimError;
use crate::kid::actions::common::{state, yflag};
use crate::kid::api::{refresh_air_jump, set_on_platform};
use crate::num::{Real, gm_ge, gm_le};
use crate::world::{Ctx, Flow, InstId};

/// The collision runs this file implements, by their `physics.toml` names (snake case). Part
/// of `kid::KidCollisionRun`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformRun {
    /// Kid A1 platform contact.
    PlatformStandard,
}

/// Runs one collision run for the kid `me` against `other`. `kid::run_collision` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, other: InstId, run: &PlatformRun) -> Result<Flow, SimError> {
    match run {
        PlatformRun::PlatformStandard => platform_standard(ctx, me, other)?,
    }
    Ok(Flow::Continue)
}

fn platform_standard(ctx: &mut Ctx, me: InstId, other: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let n = &ctx.rules.num;
    let p = &def.physics.platform;
    let PlatformRule::HalfVspeed = p.rule;
    if p.take == PlatformTake::YspeedFlipped {
        return Err(SimError::unsupported(
            "kid collision platform_standard: platform.take yspeed_flipped (5.3 platform row, WP5c)",
        ));
    }
    let missing = || SimError::invariant("platform_standard: kid or platform vanished");
    let (ky, kvs) = ctx
        .w
        .get(me)
        .map(|i| (i.m.y(), i.m.vspeed()))
        .ok_or_else(missing)?;
    let (py, pvs, pyprev) = ctx
        .w
        .get(other)
        .map(|i| (i.m.y(), i.m.vspeed(), i.m.yprevious()))
        .ok_or_else(missing)?;
    // `vspeed` takes the platform's `vspeed` (A1) or its movement this tick (family B).
    let taken = match p.take {
        PlatformTake::DeltaY => py - pyprev,
        _ => pvs,
    };
    // The reversed kid (family B `yflag`) lands from below, the mirror rule.
    let reversed = yflag(ctx, &state(ctx, me)?) < 0;
    let half = ky - kvs * Real::new(0.5);
    let (lands, snap_y) = if reversed {
        (gm_ge(half, py + p.reversed_above, n), py + p.reversed_snap)
    } else {
        (gm_le(half, py, n), py - p.snap)
    };
    if lands {
        if gm_ge(pvs, Real::ZERO, n) {
            let i = ctx.w.get_mut(me).ok_or_else(missing)?;
            i.m.set_y(snap_y)?;
            i.m.set_vspeed(taken)?;
        }
        set_on_platform(ctx, me, true);
        refresh_air_jump(ctx, me);
    }
    Ok(())
}
