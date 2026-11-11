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
use crate::world::{Ctx, Flow, InstId, Instance, KindState, Value, World};

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

/// Sets or clears the triple-jump flag `tjump` (showdown moreJumping and moreJumping2). Does
/// nothing for an instance that is not a kid.
pub fn set_triple_jump(ctx: &mut Ctx, kid: InstId, on: bool) {
    if let Some(s) = state_mut(ctx.w, kid) {
        s.tjump = on;
    }
}

/// Sets or clears the air jump flag `djump` alone (showdown spring, JUN objwater2, water with
/// `air_jump = "djump_ignoring_flag"`); the family B counter is not touched.
/// Does nothing for an instance that is not a kid.
pub fn set_air_jump(ctx: &mut Ctx, kid: InstId, on: bool) {
    if let Some(s) = state_mut(ctx.w, kid) {
        s.djump = on;
    }
}

/// Sets the family B jump counter `curJumps` (kaenbyou's objWater2 sets 2). Does nothing for an
/// instance that is not a kid.
pub fn set_cur_jumps(ctx: &mut Ctx, kid: InstId, n: u8) {
    if let Some(s) = state_mut(ctx.w, kid) {
        s.cur_jumps = n;
    }
}

/// Runs the kid's variable-jump release (`playerVJump`: an upward `vspeed` times `release_mult`)
/// now, whether or not Jump was released this tick; the caller tests the release (water's
/// same-tick clause). Does nothing while the kid is frozen. Errors for an instance that is not a
/// kid.
pub fn run_jump_release(ctx: &mut Ctx, kid: InstId) -> Result<(), SimError> {
    let s = state(ctx.w, kid).ok_or_else(|| SimError::invariant("run_jump_release: not a kid"))?;
    if s.frozen || ctx.w.engine.kid.frozen {
        return Ok(());
    }
    crate::kid::actions::common::cut_jump(ctx, kid)
}

/// A value trace setup writes to a kid field (plan 9.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KidSetValue {
    /// A flag (`tjump`, `djump`, `onPlatform`), already reduced to its GM truth.
    Flag(bool),
    /// The family B counter `curJumps`.
    Count(u8),
}

/// Writes a kid field by its GM name, for trace setup (plan 9.2): the fields [`read_field`]
/// reads that have a setter here, `tjump` ([`set_triple_jump`]), `djump` ([`set_air_jump`]),
/// `onPlatform` ([`set_on_platform`]) as flags and `curJumps` ([`set_cur_jumps`]) as a count.
/// Writes the field alone, like those setters. Errors for an instance that is not a kid, for any
/// other name (no setter yet), and for a value of the wrong shape (a count for a flag).
pub fn set_field(inst: &mut Instance, f: &KidField, v: KidSetValue) -> Result<(), SimError> {
    let KindState::Kid(s) = &mut inst.state else {
        return Err(SimError::unsupported(format!(
            "setup of kid field `{}`: instance {} is not a kid",
            f.name, inst.id.0
        )));
    };
    match (f.name.as_str(), f.index, v) {
        ("tjump", None, KidSetValue::Flag(b)) => s.tjump = b,
        ("djump", None, KidSetValue::Flag(b)) => s.djump = b,
        ("onPlatform", None, KidSetValue::Flag(b)) => s.on_platform = b,
        ("curJumps", None, KidSetValue::Count(n)) => s.cur_jumps = n,
        _ => {
            return Err(SimError::unsupported(format!(
                "setup of kid field `{}`{}: kid::api has no setter for it",
                f.name,
                f.index.map(|i| format!("[{i}]")).unwrap_or_default()
            )));
        }
    }
    Ok(())
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
/// `maxFallSpeed`, `muteki`, `scale`, `maxJumps`, `infJump`, and `jump[n]` (1-based like family B's
/// array: `jump[1]` is the ground jump; `jump` alone is the same; `jump2` and `jump3` are `jump[2]` and
/// `jump[3]`). WP5b and WP5c add the names of their own fields.
pub fn read_field(w: &World, kid: InstId, f: &KidField) -> Option<Value> {
    let s = state(w, kid)?;
    let real = |r: Real| Some(Value::Real(r));
    if f.name == "jump" {
        // Family B's `jump[1]`, `jump[2]` count from 1; plain `jump` is the ground jump.
        let i = match f.index {
            Some(n) => usize::try_from(n).ok()?.checked_sub(1)?,
            None => 0,
        };
        return s.jump_speeds.get(i).copied().and_then(real);
    }
    if f.index.is_some() {
        return None;
    }
    match f.name.as_str() {
        "maxJumps" => real(Real::new(f64::from(s.b.max_jumps))),
        "infJump" => Some(flag(s.b.inf_jump)),
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

/// Kid fields that come from the game's profile, not from the kid's state; `None` for any other
/// name. `grav`: family B declares `grav` 0.4 in Create and never reads it (kid/B.md), so a game
/// that flips by `yflag` reads its profile gravity. Other families use `grav` as a sign, which the
/// kid state does not hold as a field.
pub fn read_field_def(def: &crate::def::GameDef, f: &KidField) -> Option<Value> {
    let by_flag = def.physics.flip.model == crate::def::physics::FlipKind::Yflag;
    (f.name == "grav" && f.index.is_none() && by_flag).then_some(Value::Real(def.physics.numbers.gravity))
}

/// Whether the kid is the flipped one: the flipped kid's own state flag, or the flipped class
/// (the games that flip by swapping to a second kid kind). The save reads this, not the class
/// alone, because B and C flip by a flag on one kind.
pub fn flipped(w: &World, kid: InstId) -> bool {
    let Some(i) = w.get(kid) else { return false };
    i.classes.intersects(crate::def::ClassMask::KID_FLIPPED) || state(w, kid).is_some_and(|s| s.flipped)
}

/// The facing a save stores, -1 or 1: the kid's `facing`, and for an instance with no kid state
/// (a test stand-in) the sign of its `image_xscale`.
pub fn saved_facing(w: &World, kid: InstId) -> i8 {
    match state(w, kid) {
        Some(s) => s.facing,
        None => match w.get(kid) {
            // gm-compare: ok (sign of the stand-in's scale, set by the test to -1 or 1; not a GML comparison)
            Some(i) if i.img.xscale().get() < 0.0 => -1,
            _ => 1,
        },
    }
}

/// Puts a loaded kid in the saved orientation: `facing` (and `image_xscale`).
///
/// For a game that flips by flag (`[flip] model = "yflag"`, family B) the saved flipped state is
/// `global.reverse`: it becomes the global, and the kid keeps the `yflag` its Create gave it, which
/// is stale until its first Step fixes it (kid/B.md Restart). For the other models the kid's
/// `flipped` flag and `yflag` follow the save. The flipped kid kind and the `+dy` offset are the
/// loader's (`save::load_into`).
pub fn restore_orientation(ctx: &mut Ctx, kid: InstId, flipped: bool, facing: i8) -> Result<(), SimError> {
    let by_flag = ctx.def.physics.flip.model == crate::def::physics::FlipKind::Yflag;
    if by_flag {
        ctx.w.engine.kid.b.reverse = flipped;
    }
    let Some(i) = ctx.w.get_mut(kid) else {
        return Err(SimError::invariant(format!(
            "kid::api::restore_orientation: no instance {}",
            kid.0
        )));
    };
    i.img.set_xscale(Real::new(f64::from(facing)))?;
    if let KindState::Kid(s) = &mut i.state {
        s.facing = facing;
        if !by_flag {
            s.flipped = flipped;
            s.yflag = if flipped { -1 } else { 1 };
        }
    }
    Ok(())
}

/// Clears `frozen` and `frozen2`: the globals and the flags of every live kid, both kinds
/// (B Restart step 3, showdown Restart step 1).
pub fn clear_frozen(ctx: &mut Ctx) {
    ctx.w.engine.kid.frozen = false;
    ctx.w.engine.kid.frozen2 = false;
    let ids: Vec<InstId> = ctx
        .w
        .instances()
        .iter()
        .filter(|i| matches!(i.state, KindState::Kid(_)))
        .map(|i| i.id)
        .collect();
    for id in ids {
        if let Some(s) = state_mut(ctx.w, id) {
            s.frozen = false;
            s.frozen2 = false;
        }
    }
}

/// Resets the kid's jump values to the profile's, both speeds to 0 and the jump flags
/// (`djump` true; `tjump` true when the game has a third jump): showdown's loadGame step.
pub fn reset_jump_values(ctx: &mut Ctx, kid: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let triple = def.physics.jump.air == crate::def::physics::AirJump::DjumpThenTjump;
    let Some(i) = ctx.w.get_mut(kid) else {
        return Err(SimError::invariant(format!(
            "kid::api::reset_jump_values: no instance {}",
            kid.0
        )));
    };
    let KindState::Kid(s) = &mut i.state else {
        return Err(SimError::invariant(format!(
            "kid::api::reset_jump_values: instance {} is not a kid",
            kid.0
        )));
    };
    s.jump_speeds.clone_from(&def.physics.numbers.jumps);
    s.djump = true;
    s.tjump = triple;
    i.m.set_hspeed(Real::ZERO)?;
    i.m.set_vspeed(Real::ZERO)?;
    Ok(())
}

/// `playerReverse` on `kid` (family B, `flip.model = "yflag"`): `global.reverse` toggles, `yflag`
/// negates, `y` moves 4 times the new `yflag`, `curJumps` 1, `vspeed` 0. The gravity arrows call
/// this from their Alarm 1.
pub fn player_reverse(ctx: &mut Ctx, kid: InstId) -> Result<(), SimError> {
    super::actions::b::player_reverse(ctx, kid)
}

/// `scrFlipGrav` on `kid` (family C, `flip.model = "grav_sign"`): `grav` negates, `djump` 1,
/// `vspeed` 0, gravity and `image_yscale` take the new sign and `y` moves 4 times it. The gravity
/// arrows call this on contact.
pub fn flip_grav(ctx: &mut Ctx, kid: InstId) -> Result<(), SimError> {
    super::actions::flip::flip_grav(ctx, kid)
}
