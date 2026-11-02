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
    if p.take != PlatformTake::Vspeed {
        return Err(SimError::unsupported(format!(
            "kid collision platform_standard: platform.take {:?} (5.3 platform row, WP5b/WP5c)",
            p.take
        )));
    }
    let missing = || SimError::invariant("platform_standard: kid or platform vanished");
    let (ky, kvs) = ctx
        .w
        .get(me)
        .map(|i| (i.m.y(), i.m.vspeed()))
        .ok_or_else(missing)?;
    let (py, pvs) = ctx
        .w
        .get(other)
        .map(|i| (i.m.y(), i.m.vspeed()))
        .ok_or_else(missing)?;
    if gm_le(ky - kvs * Real::new(0.5), py, n) {
        if gm_ge(pvs, Real::ZERO, n) {
            let i = ctx.w.get_mut(me).ok_or_else(missing)?;
            i.m.set_y(py - p.snap)?;
            i.m.set_vspeed(pvs)?;
        }
        set_on_platform(ctx, me, true);
        refresh_air_jump(ctx, me);
    }
    Ok(())
}
