//! Kid access for other WPs (docs/plans/sim-core.md 1.6).
//!
//! Built-ins never touch `KidState` fields directly; they call these. WP0 declared the
//! signatures; WP5a owns the bodies.
//!
//! Functions that cannot return an error do nothing, or give a neutral value, for an instance
//! that is not a live kid (documented on each); the ones that return `Result` report a broken
//! invariant.

use super::state::KidState;
use crate::def::{ParamValue, Params};
use crate::error::SimError;
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId, KindState, Value, World};

/// A kid parameter zone's settings (rainbow, showdown; `kid_param_zone` built-in).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ParamZone {
    /// The kid fields the zone sets, by profile name.
    pub params: Params,
}

/// What the bow copies from the kid each Step (1-tick lag, Inferred).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BowSnapshot {
    /// Kid x.
    pub x: Real,
    /// Kid y.
    pub y: Real,
    /// Kid `image_xscale`.
    pub xscale: Real,
    /// Kid `image_yscale`.
    pub yscale: Real,
    /// Kid `image_alpha`.
    pub alpha: Real,
}

/// Why the kid died.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KillCause {
    /// A killer instance.
    Killer(InstId),
    /// The suicide key (Q).
    Suicide,
    /// Left the room or crossed its boundary.
    Boundary,
    /// A built-in or script killed it (`killPlayer`).
    Script,
}

/// A kid field by its GM name, for the probe registry (plan 9.3): `djump`, `curJumps`,
/// `jump[1]` is `{ name: "jump", index: Some(1) }`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KidField {
    /// GM variable name.
    pub name: String,
    /// Array index, if any.
    pub index: Option<u32>,
}

fn state(w: &World, kid: InstId) -> Option<&KidState> {
    match &w.get(kid)?.state {
        KindState::Kid(s) => Some(s),
        _ => None,
    }
}

fn state_mut(w: &mut World, kid: InstId) -> Option<&mut KidState> {
    match &mut w.get_mut(kid)?.state {
        KindState::Kid(s) => Some(s),
        _ => None,
    }
}

fn flag(b: bool) -> Value {
    Value::Real(if b { Real::ONE } else { Real::ZERO })
}

/// The kid's facing, -1 or 1. An instance that is not a kid faces right (1).
pub fn facing(w: &World, kid: InstId) -> i8 {
    state(w, kid).map_or(1, |s| s.facing)
}

/// Refreshes the air jump (water, refreshers, platforms, landing): `djump` true and the family B
/// counter back to 1. A family reads only the one its `jump.air` names, so both are set. The
/// triple jump flag (`tjump`) is not touched: showdown's refresher sets it on its own terms.
/// Does nothing for an instance that is not a kid.
pub fn refresh_air_jump(ctx: &mut Ctx, kid: InstId) {
    if let Some(s) = state_mut(ctx.w, kid) {
        s.djump = true;
        s.cur_jumps = 1;
    }
}

/// Sets the on-platform flag. Does nothing for an instance that is not a kid.
pub fn set_on_platform(ctx: &mut Ctx, kid: InstId, on: bool) {
    if let Some(s) = state_mut(ctx.w, kid) {
        s.on_platform = on;
    }
}

/// Which number of the kid a zone parameter names.
enum NumberSlot {
    RunSpeed,
    MaxVspeed,
    MaxFallSpeed,
    Jump(usize),
}

/// A zone parameter's slot, by profile name or GM name.
fn number_slot(name: &str) -> Option<NumberSlot> {
    Some(match name {
        "run_speed" | "maxSpeed" => NumberSlot::RunSpeed,
        "max_vspeed" | "maxVspeed" => NumberSlot::MaxVspeed,
        "max_fall_speed" | "maxFallSpeed" => NumberSlot::MaxFallSpeed,
        "jump" | "jump1" => NumberSlot::Jump(0),
        "jump2" => NumberSlot::Jump(1),
        "jump3" => NumberSlot::Jump(2),
        _ => return None,
    })
}

/// Applies a parameter zone: writes each named kid number (`run_speed`, `max_vspeed`,
/// `max_fall_speed`, `jump`, `jump2`, `jump3`; the GM names `maxSpeed`, `maxVspeed` and
/// `maxFallSpeed` work too). The values persist until another zone or a new kid's Create changes
/// them (mechanics Inferred). Errors, with nothing applied, when a value is not a real, is not
/// finite (N15), names no kid number, or `kid` is not a kid.
pub fn apply_param_zone(ctx: &mut Ctx, kid: InstId, z: &ParamZone) -> Result<(), SimError> {
    let mut writes = Vec::with_capacity(z.params.0.len());
    for (name, value) in &z.params.0 {
        let slot = number_slot(name)
            .ok_or_else(|| SimError::unsupported(format!("kid param zone field `{name}`")))?;
        let ParamValue::Real(v) = value else {
            return Err(SimError::unsupported(format!(
                "kid param zone field `{name}` is not a real"
            )));
        };
        if !v.is_finite() {
            return Err(SimError::non_finite(format!("kid param zone `{name}`")));
        }
        writes.push((slot, *v));
    }
    let s = state_mut(ctx.w, kid).ok_or_else(|| {
        SimError::invariant(format!(
            "kid::api::apply_param_zone: instance {} is not a kid",
            kid.0
        ))
    })?;
    for (slot, v) in writes {
        match slot {
            NumberSlot::RunSpeed => s.run_speed = v,
            NumberSlot::MaxVspeed => s.max_vspeed = v,
            NumberSlot::MaxFallSpeed => s.max_fall_speed = v,
            NumberSlot::Jump(i) => {
                if s.jump_speeds.len() <= i {
                    s.jump_speeds.resize(i + 1, Real::ZERO);
                }
                s.jump_speeds[i] = v;
            }
        }
    }
    Ok(())
}

/// What the bow copies from the kid: position and image variables as they are now. For an
/// instance that does not exist the snapshot is the origin with scales and alpha 1.
pub fn bow_snapshot(w: &World, kid: InstId) -> BowSnapshot {
    match w.get(kid) {
        Some(i) => BowSnapshot {
            x: i.m.x(),
            y: i.m.y(),
            xscale: i.img.xscale(),
            yscale: i.img.yscale(),
            alpha: i.img.alpha(),
        },
        None => BowSnapshot {
            x: Real::ZERO,
            y: Real::ZERO,
            xscale: Real::ONE,
            yscale: Real::ONE,
            alpha: Real::ONE,
        },
    }
}

/// Kills the kid per the family's `[death]` order (`death::kill`). `Ok(Flow::AbortTick)` when the
/// death requested a room change (rely in the Select role); an `Err` halts the sim.
pub fn kill(ctx: &mut Ctx, kid: InstId, cause: KillCause) -> Result<Flow, SimError> {
    super::death::kill(ctx, kid, cause)
}

/// A kid field for the probe, by its GM name; `None` when this kid has no such field. Flags read
/// as 1 or 0. Names: `djump`, `curJumps`, `tjump`, `onPlatform`, `ladder`, `ladderud`,
/// `ladderjump`, `frozen`, `frozen2`, `yflag`, `facing`, `tempHspeed`, `maxSpeed`, `maxVspeed`,
/// `maxFallSpeed`, `muteki`, `scale`, and `jump[i]` (`jump` alone is `jump[0]`; `jump2` and
/// `jump3` are `jump[1]` and `jump[2]`). WP5b and WP5c add the names of their own fields.
pub fn read_field(w: &World, kid: InstId, f: &KidField) -> Option<Value> {
    let s = state(w, kid)?;
    let real = |r: Real| Some(Value::Real(r));
    if f.name == "jump" {
        let i = usize::try_from(f.index.unwrap_or(0)).ok()?;
        return s.jump_speeds.get(i).copied().and_then(real);
    }
    if f.index.is_some() {
        return None;
    }
    match f.name.as_str() {
        "jump2" => s.jump_speeds.get(1).copied().and_then(real),
        "jump3" => s.jump_speeds.get(2).copied().and_then(real),
        "djump" => Some(flag(s.djump)),
        "curJumps" => real(Real::new(f64::from(s.cur_jumps))),
        "tjump" => Some(flag(s.tjump)),
        "onPlatform" => Some(flag(s.on_platform)),
        "ladder" => Some(flag(s.ladder)),
        "ladderud" => Some(flag(s.ladder_ud)),
        "ladderjump" => Some(flag(s.ladder_jump)),
        "frozen" => Some(flag(s.frozen)),
        "frozen2" => Some(flag(s.frozen2)),
        "yflag" => real(Real::new(f64::from(s.yflag))),
        "facing" => real(Real::new(f64::from(s.facing))),
        "tempHspeed" => real(s.temp_hspeed),
        "maxSpeed" => real(s.run_speed),
        "maxVspeed" => real(s.max_vspeed),
        "maxFallSpeed" => real(s.max_fall_speed),
        "muteki" => Some(flag(s.invincible)),
        "scale" => real(s.scale),
        _ => None,
    }
}
