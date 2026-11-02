//! Kid actions shared by every family: `create_defaults`, `gravity`, `walk`, `air_sprite`,
//! `fall_cap`, `shoot`, `jump`, `release`, `vines` and the other rows of docs/plans/sim-core.md
//! 5.3 marked 5a. WP5a owns the file.
//!
//! Spec: kid/A1.md "Create", "Step" (actions 2-4), "playerJump", "playerVJump", "playerShoot".
//! A named action whose behavior belongs to another work package returns
//! `SimError::Unsupported` naming its row, so a profile that lists it halts instead of silently
//! skipping it.
//!
//! **OPEN (sound).** The jump, double jump, shoot and vine kick sounds are not played: the sound
//! model is WP10 and its `sound::play` is not built. Sounds are output only, so no trace column
//! depends on them.

use serde::Deserialize;

use crate::alarm;
use crate::anim::Image;
use crate::collide::query::{Target, distance_to_object, place_free, place_meeting};
use crate::def::physics::{AirJump, FallCapRule, GroundAlso, VineKeys, VineModel, WalkRead, WaterAir};
use crate::def::room::RoomRole;
use crate::def::{ClassMask, EngineKind, GameDef, SpriteId};
use crate::error::SimError;
use crate::input::Action;
use crate::kid::api::KillCause;
use crate::kid::state::KidState;
use crate::num::{Real, gm_eq, gm_gt, gm_lt, gm_ne};
use crate::save;
use crate::world::{Ctx, Flow, InstId, KindState, SpawnInit};

/// The actions this file implements, by their `physics.toml` names (snake case). Part of
/// `kid::KidAction`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommonAction {
    /// Create step 1-2: the numbers, flags and facing a new kid starts with.
    CreateDefaults,
    /// Create step 3: the world controller if the game has none in the room.
    EnsureWorld,
    /// Create step 6: a bow at the kid's position.
    CreateBow,
    /// Create step 4: when no save exists, deaths and time 0 and a save at the kid's position.
    FirstPlaySave,
    /// Create step 5: alarm 0 = `room_speed`.
    ArmPlayTimer,
    /// Step action 2 step 4: gravity back to its constant unless on a ladder.
    Gravity,
    /// Step action 3 steps 1-4: direction keys, facing, horizontal speed, running and idle sprites.
    Walk,
    /// Step action 3 step 5: jump and fall sprites, and `onPlatform` release.
    AirSprite,
    /// Step action 3 step 6: the speed clamp, by `fall_cap.rule`.
    FallCap,
    /// Step action 3 step 7: shoot on press.
    Shoot,
    /// Step action 3 step 7: `playerJump` on press.
    Jump,
    /// Step action 3 step 7: `playerVJump` on release.
    Release,
    /// Step action 4: vines.
    Vines,
    /// Alarm 0: outside the menu roles `global.time` + 1, then alarm 0 = `room_speed` again.
    PlayTimer,
    /// Key Q pressed (and the killPlayer calls of key rows): the death sequence.
    Kill,
}

/// Runs one action on the kid `me`. `kid::run_action` routes here.
pub fn run(ctx: &mut Ctx, me: InstId, action: &CommonAction) -> Result<Flow, SimError> {
    match action {
        CommonAction::CreateDefaults => create_defaults(ctx, me)?,
        CommonAction::Gravity => gravity(ctx, me)?,
        CommonAction::Walk => walk(ctx, me)?,
        CommonAction::AirSprite => air_sprite(ctx, me)?,
        CommonAction::FallCap => fall_cap(ctx, me)?,
        CommonAction::Shoot => return shoot(ctx, me),
        CommonAction::Jump => jump(ctx, me)?,
        CommonAction::Release => release(ctx, me)?,
        CommonAction::Vines => vines(ctx, me)?,
        CommonAction::EnsureWorld => ensure_world(ctx)?,
        CommonAction::CreateBow => return create_bow(ctx, me),
        CommonAction::FirstPlaySave => first_play_save(ctx, me)?,
        CommonAction::ArmPlayTimer => arm_play_timer(ctx, me)?,
        CommonAction::PlayTimer => play_timer(ctx, me)?,
        CommonAction::Kill => return super::super::death::kill(ctx, me, KillCause::Suicide),
    }
    Ok(Flow::Continue)
}

/// Create step 3: spawns the game's `world` kind when no live instance of it exists.
fn ensure_world(ctx: &mut Ctx) -> Result<(), SimError> {
    let Some(kind) = ctx.def.engine_kind(EngineKind::World) else {
        return Ok(());
    };
    if ctx.w.iter_alive().any(|i| i.kind == kind) {
        return Ok(());
    }
    ctx.spawn(kind, Real::ZERO, Real::ZERO, SpawnInit::default())?;
    Ok(())
}

/// Create step 6: a bow at the kid's position (the game may have none).
fn create_bow(ctx: &mut Ctx, me: InstId) -> Result<Flow, SimError> {
    let Some(i) = ctx.w.get(me) else {
        return Ok(Flow::Continue);
    };
    let (x, y) = (i.m.x(), i.m.y());
    ctx.spawn_engine(EngineKind::Bow, x, y, SpawnInit::default())?;
    Ok(Flow::Continue)
}

/// Create step 4: with no save, `global.death` and `global.time` become 0 and the position is
/// saved with the game's model (`[save]`).
fn first_play_save(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    if ctx.w.save.is_some() {
        return Ok(());
    }
    let rules = &ctx.def.physics.save;
    ctx.w.engine.run.deaths = 0;
    ctx.w.engine.run.time = 0;
    let fixed = rules.fixed.map(|[x, y]| (x, y));
    save::save_now(ctx, me, rules.position.model(), fixed)
}

/// Create step 5: `alarm[0] = room_speed`.
fn arm_play_timer(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let speed = ctx.w.room_speed;
    let rules = ctx.rules;
    match ctx.w.get_mut(me) {
        Some(i) => alarm::set(i, 0, speed, &rules.num),
        None => Ok(()),
    }
}

/// Alarm 0: `global.time` + 1 unless the room is the init, title, menu or select room (checked),
/// then alarm 0 again. Time stops counting while no kid exists (only this alarm counts it).
fn play_timer(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let counts = ctx
        .w
        .room
        .index()
        .and_then(|i| ctx.def.rooms.get(i))
        .is_none_or(|r| r.role == RoomRole::Play);
    if counts {
        let run = &mut ctx.w.engine.run;
        run.time = run
            .time
            .checked_add(1)
            .ok_or_else(|| SimError::overflow("global.time"))?;
    }
    arm_play_timer(ctx, me)
}

/// The kid's state, or a broken invariant when `me` is not a kid.
pub(crate) fn state_mut<'a>(ctx: &'a mut Ctx, me: InstId) -> Result<&'a mut KidState, SimError> {
    match ctx.w.get_mut(me).map(|i| &mut i.state) {
        Some(KindState::Kid(s)) => Ok(s),
        _ => Err(SimError::invariant(format!(
            "kid action on instance {} that is not a kid",
            me.0
        ))),
    }
}

/// A copy of the kid's state.
pub(crate) fn state(ctx: &Ctx, me: InstId) -> Result<KidState, SimError> {
    match ctx.w.get(me).map(|i| &i.state) {
        Some(KindState::Kid(s)) => Ok((**s).clone()),
        _ => Err(SimError::invariant(format!(
            "kid action on instance {} that is not a kid",
            me.0
        ))),
    }
}

/// `sprite_index = s` with the recorder's index reset (`Image::change_sprite`). A role the game
/// has no sprite for leaves the sprite alone.
pub(crate) fn set_sprite(def: &GameDef, img: &mut Image, s: Option<SpriteId>) {
    let Some(id) = s else { return };
    let frames = id
        .index()
        .and_then(|i| def.sprites.get(i))
        .map_or(0, |d| d.frames);
    img.change_sprite(Some(id), frames);
}

/// `n` as a `Real`.
fn real_i(n: i32) -> Real {
    Real::new(f64::from(n))
}

/// A distance query result as the vine rules read it: `None` (no such instance) is far.
fn far(d: Option<Real>) -> Real {
    d.unwrap_or(Real::new(f64::MAX))
}

/// +1 normally, -1 for the flipped kid (gravity points up).
fn down(s: &KidState) -> i32 {
    if s.flipped { -1 } else { 1 }
}

/// Create steps 1 and 2 (A1): numbers from the profile, flags cleared, `djump` true,
/// `image_alpha` 0 when the game fades the kid in, and facing left only when the previous kid
/// handed over `global.image_xscale = -1`.
fn create_defaults(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let p = &def.physics;
    let handed_over_left = ctx.w.engine.kid.image_xscale == -1;
    {
        let s = state_mut(ctx, me)?;
        s.jump_speeds.clone_from(&p.numbers.jumps);
        s.run_speed = p.numbers.run_speed;
        s.max_vspeed = p.numbers.max_vspeed;
        s.max_fall_speed = p.numbers.max_fall_speed;
        s.frozen = false;
        s.frozen2 = false;
        s.djump = true;
        s.cur_jumps = 1;
        s.invincible = false;
        s.facing = if handed_over_left { -1 } else { 1 };
    }
    let Some(inst) = ctx.w.get_mut(me) else {
        return Err(SimError::invariant("create_defaults: kid vanished"));
    };
    inst.m.set_gravity(p.numbers.gravity)?;
    inst.img.set_speed(p.numbers.image_speed_idle)?;
    if p.misc.fade_in {
        inst.img.set_alpha(Real::ZERO)?;
    }
    if handed_over_left {
        inst.img.set_xscale(real_i(-1))?;
    }
    Ok(())
}

/// Step action 2 step 4: gravity back to its constant unless on a ladder (`gravity.reapply =
/// "step"`). Other models are other work packages.
fn gravity(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    use crate::def::physics::GravityReapply;
    let def = ctx.def;
    if def.physics.gravity.reapply != GravityReapply::Step {
        return Err(SimError::unsupported(
            "kid action gravity: gravity.reapply other than \"step\" (5.3 gravity row, WP5b/WP5c)",
        ));
    }
    if state(ctx, me)?.ladder {
        return Ok(());
    }
    let g = def.physics.numbers.gravity;
    match ctx.w.get_mut(me) {
        Some(i) => i.m.set_gravity(g),
        None => Ok(()),
    }
}

/// Step action 3, steps 1-4 (A1; slip blocks are `[ice]`, WP5c).
///
/// `h` is right, else minus left, both read (right first) the way `input.walk` says; frozen
/// forces 0. Facing changes to `h` unless the right vine distance is exactly 1 or the left vine
/// distance is 1 or less; `hspeed` is set only when the direction is not blocked by a vine
/// (else it keeps its old value and the sprite stays).
fn walk(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let p = &def.physics;
    let s = state(ctx, me)?;
    if s.frozen2 || ctx.w.engine.kid.frozen2 {
        return Ok(());
    }
    let (right, left) = match p.input.walk {
        WalkRead::DirectLatch => {
            let r = ctx.w.input.direct(Action::Right);
            let l = ctx.w.input.direct(Action::Left);
            (r, l)
        }
        WalkRead::Sampled => (ctx.w.input.held(Action::Right), ctx.w.input.held(Action::Left)),
    };
    let mut h: i32 = if right {
        1
    } else if left {
        -1
    } else {
        0
    };
    if s.frozen || ctx.w.engine.kid.frozen {
        h = 0;
    }
    let (dr, dl) = vine_distances(ctx, me)?;
    let n = &ctx.rules.num;
    let run = s.run_speed;
    if h != 0 {
        if gm_ne(dr, Real::ONE, n) && gm_gt(dl, Real::ONE, n) {
            state_mut(ctx, me)?.facing = if h > 0 { 1 } else { -1 };
            if let Some(i) = ctx.w.get_mut(me) {
                i.img.set_xscale(real_i(h))?;
            }
        }
        let open = (h == -1 && gm_ne(dr, Real::ONE, n)) || (h == 1 && gm_gt(dl, Real::ONE, n));
        if let (true, Some(i)) = (open, ctx.w.get_mut(me)) {
            i.m.set_hspeed(run * real_i(h))?;
            set_sprite(def, &mut i.img, p.sprites.running);
            i.img.set_speed(p.numbers.image_speed_run)?;
        }
    } else if let Some(i) = ctx.w.get_mut(me) {
        i.m.set_hspeed(Real::ZERO)?;
        set_sprite(def, &mut i.img, p.sprites.idle);
        i.img.set_speed(p.numbers.image_speed_idle)?;
    }
    Ok(())
}

/// Distances to the nearest right-side vine and left-side vine; far when there is none.
fn vine_distances(ctx: &Ctx, me: InstId) -> Result<(Real, Real), SimError> {
    let dr = far(distance_to_object(ctx, me, Target::Class(ClassMask::VINE_R))?);
    let dl = far(distance_to_object(ctx, me, Target::Class(ClassMask::VINE_L))?);
    Ok((dr, dl))
}

/// Step action 3 step 5: off a platform, `vspeed` beyond the threshold picks the jump or fall
/// sprite; on one, `onPlatform` clears when no platform lies `platform.hold_probe` pixels below.
fn air_sprite(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let n = &ctx.rules.num;
    let p = &def.physics;
    let s = state(ctx, me)?;
    if s.on_platform {
        let Some(i) = ctx.w.get(me) else { return Ok(()) };
        let (x, y) = (i.m.x(), i.m.y());
        let below = y + p.platform.hold_probe * real_i(down(&s));
        if !place_meeting(ctx, me, x, below, Target::Class(ClassMask::PLATFORM))? {
            state_mut(ctx, me)?.on_platform = false;
        }
        return Ok(());
    }
    let Some(i) = ctx.w.get_mut(me) else { return Ok(()) };
    let v = i.m.vspeed();
    let t = p.numbers.sprite_threshold;
    if gm_lt(v, -t, n) {
        set_sprite(def, &mut i.img, p.sprites.jump);
    } else if gm_gt(v, t, n) {
        set_sprite(def, &mut i.img, p.sprites.fall);
    }
    Ok(())
}

/// Step action 3 step 6: `fall_cap.rule = "clamp_abs"`: `abs(vspeed) > max_vspeed` gives
/// `sign(vspeed) * max_vspeed`. The other rules are WP5b and WP5c.
fn fall_cap(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let rule = ctx.def.physics.fall_cap.rule;
    if rule != FallCapRule::ClampAbs {
        return Err(SimError::unsupported(format!(
            "kid action fall_cap: rule {rule:?} (5.3 fall cap row, WP5b/WP5c)"
        )));
    }
    let n = &ctx.rules.num;
    let max = state(ctx, me)?.max_vspeed;
    let Some(i) = ctx.w.get_mut(me) else { return Ok(()) };
    let v = i.m.vspeed();
    if gm_gt(Real::new(v.get().abs()), max, n) {
        i.m.set_vspeed(if gm_lt(v, Real::ZERO, n) { -max } else { max })?;
    }
    Ok(())
}

/// Step action 3 step 7, `playerShoot`: when not frozen and Shoot was pressed this tick, and
/// fewer than `bullet.max` bullets exist, a bullet appears at the kid's position moving
/// `bullet.speed * facing`. (The bullet's own Create, life and the sound are `kid_bullet`, WP6a.)
fn shoot(ctx: &mut Ctx, me: InstId) -> Result<Flow, SimError> {
    let def = ctx.def;
    let s = state(ctx, me)?;
    if s.frozen || ctx.w.engine.kid.frozen || !ctx.w.input.pressed(Action::Shoot) {
        return Ok(Flow::Continue);
    }
    if ctx.w.count_of(ClassMask::KID_BULLET) >= def.physics.bullet.max {
        return Ok(Flow::Continue);
    }
    let Some(i) = ctx.w.get(me) else {
        return Ok(Flow::Continue);
    };
    let (x, y) = (i.m.x(), i.m.y());
    let Some(b) = ctx.spawn_engine(EngineKind::KidBullet, x, y, SpawnInit::default())? else {
        return Err(SimError::invariant("shoot: the game has no kid_bullet kind"));
    };
    if let Some(bi) = ctx.w.get_mut(b) {
        bi.m.set_hspeed(def.physics.bullet.speed * real_i(i32::from(s.facing)))?;
    }
    Ok(if ctx.w.status == crate::world::SimStatus::Running {
        Flow::Continue
    } else {
        Flow::Halt
    })
}

/// Step action 3 step 7, `playerJump`: on Jump press, when not frozen. A ground jump (a ground
/// class one pixel below, or a `ground_also` condition) sets `vspeed = -jump[0]` and `djump`; else
/// an air jump when `djump` is set or a water 2 lies one pixel below sets `vspeed = -jump[1]`,
/// the jump sprite and `djump` false; else nothing. Only the 1 px test matters, never `vspeed`.
fn jump(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let p = &def.physics;
    let s = state(ctx, me)?;
    if s.frozen || ctx.w.engine.kid.frozen || !ctx.w.input.pressed(Action::Jump) {
        return Ok(());
    }
    if p.jump.air != AirJump::Djump {
        return Err(SimError::unsupported(format!(
            "kid action jump: jump.air {:?} (5.3 jump rows, WP5b/WP5c)",
            p.jump.air
        )));
    }
    let sign = real_i(down(&s));
    let Some(i) = ctx.w.get(me) else { return Ok(()) };
    let (x, y) = (i.m.x(), i.m.y());
    let below = y + sign;
    let mut ground = place_meeting(ctx, me, x, below, Target::Class(p.jump.ground_classes))?;
    for extra in &p.jump.ground_also {
        ground |= match extra {
            GroundAlso::PlatformFlag => s.on_platform,
            GroundAlso::LadderJump => s.ladder_jump,
        };
    }
    let jump0 = s.jump_speeds.first().copied().unwrap_or(Real::ZERO);
    let jump1 = s.jump_speeds.get(1).copied().unwrap_or(Real::ZERO);
    if ground {
        if let Some(i) = ctx.w.get_mut(me) {
            i.m.set_vspeed(-(sign * jump0))?;
        }
        if p.jump.ground_sets_djump {
            state_mut(ctx, me)?.djump = true;
        }
        return Ok(());
    }
    let water2 = match p.jump.water_air {
        WaterAir::None => false,
        WaterAir::Water2WithoutFlag => place_meeting(ctx, me, x, below, Target::Class(ClassMask::WATER2))?,
        other => {
            return Err(SimError::unsupported(format!(
                "kid action jump: jump.water_air {other:?} (5.3 jump rows, WP5b/WP5c)"
            )));
        }
    };
    if s.djump || water2 {
        if let Some(i) = ctx.w.get_mut(me) {
            i.m.set_vspeed(-(sign * jump1))?;
            set_sprite(def, &mut i.img, p.sprites.jump);
        }
        state_mut(ctx, me)?.djump = false;
    }
    Ok(())
}

/// Step action 3 step 7, `playerVJump`: on Jump release, when not frozen, an upward `vspeed`
/// (against gravity) is multiplied by `release_mult`. It runs on every release, so a jump can be
/// cut more than once.
fn release(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let n = &ctx.rules.num;
    let s = state(ctx, me)?;
    if s.frozen || ctx.w.engine.kid.frozen || !ctx.w.input.released(Action::Jump) {
        return Ok(());
    }
    let sign = real_i(down(&s));
    let Some(i) = ctx.w.get_mut(me) else { return Ok(()) };
    let v = i.m.vspeed();
    if gm_lt(sign * v, Real::ZERO, n) {
        i.m.set_vspeed(v * def.physics.numbers.release_mult)?;
    }
    Ok(())
}

/// Step action 4, vines (`vine.model = "distance_asym"`): a left vine closer than `left_below`
/// with free space one pixel below grabs: slide speed, `maxFallSpeed` 2, sliding sprite, facing
/// right; a Right press with Jump held kicks off, without it drops away. A right vine at
/// exactly `right_equal` mirrors it. Distances above 1 with free space below set `maxFallSpeed`
/// back to its constant (no effect in A1; `shift_then_clamp` reads it).
fn vines(ctx: &mut Ctx, me: InstId) -> Result<(), SimError> {
    let def = ctx.def;
    let n = &ctx.rules.num;
    let p = &def.physics;
    match p.vine.model {
        VineModel::None => return Ok(()),
        VineModel::DistanceAsym => {}
        VineModel::DistanceSymEarly => {
            return Err(SimError::unsupported(
                "kid action vines: vine.model distance_sym_early (5.3 vine row C, WP5c)",
            ));
        }
    }
    if p.input.vine_keys != VineKeys::Edges {
        return Err(SimError::unsupported(
            "kid action vines: input.vine_keys latched_flags (5.3 vine keys row, WP5c)",
        ));
    }
    let Some(i) = ctx.w.get(me) else { return Ok(()) };
    let (x, y) = (i.m.x(), i.m.y());
    let s = state(ctx, me)?;
    let (dr, dl) = vine_distances(ctx, me)?;
    let below_free = place_free(ctx, me, x, y + real_i(down(&s)))?;
    let (right_pressed, left_pressed, jump_held) = (
        ctx.w.input.pressed(Action::Right),
        ctx.w.input.pressed(Action::Left),
        ctx.w.input.held(Action::Jump),
    );
    let max_fall = p.numbers.max_fall_speed;
    let slide_fall = Real::new(2.0);
    let (kick_h, kick_v) = (p.vine.kick[0], p.vine.kick[1]);

    // Left vine: face right; kick or drop on a Right press.
    if gm_lt(dl, p.vine.left_below, n) && below_free {
        state_mut(ctx, me)?.max_fall_speed = slide_fall;
        state_mut(ctx, me)?.facing = 1;
        if let Some(i) = ctx.w.get_mut(me) {
            i.m.set_vspeed(p.vine.slide)?;
            set_sprite(def, &mut i.img, p.sprites.sliding);
            i.img.set_xscale(Real::ONE)?;
            i.img.set_speed(p.numbers.image_speed_run)?;
            if right_pressed {
                if jump_held {
                    set_sprite(def, &mut i.img, p.sprites.jump);
                    i.m.set_vspeed(kick_v)?;
                    i.m.set_hspeed(kick_h)?;
                } else {
                    i.m.set_hspeed(p.vine.drop)?;
                    set_sprite(def, &mut i.img, p.sprites.fall);
                }
            }
        }
    }
    if gm_gt(dl, Real::ONE, n) && below_free {
        state_mut(ctx, me)?.max_fall_speed = max_fall;
    }
    // Right vine: the mirror, at exactly `right_equal`.
    if gm_eq(dr, p.vine.right_equal, n) && below_free {
        state_mut(ctx, me)?.max_fall_speed = slide_fall;
        state_mut(ctx, me)?.facing = -1;
        if let Some(i) = ctx.w.get_mut(me) {
            i.m.set_vspeed(p.vine.slide)?;
            set_sprite(def, &mut i.img, p.sprites.sliding);
            i.img.set_xscale(real_i(-1))?;
            i.img.set_speed(p.numbers.image_speed_run)?;
            if left_pressed {
                if jump_held {
                    set_sprite(def, &mut i.img, p.sprites.jump);
                    i.m.set_vspeed(kick_v)?;
                    i.m.set_hspeed(-kick_h)?;
                } else {
                    i.m.set_hspeed(-p.vine.drop)?;
                    set_sprite(def, &mut i.img, p.sprites.fall);
                }
            }
        }
    }
    if gm_gt(dr, Real::ONE, n) && below_free {
        state_mut(ctx, me)?.max_fall_speed = max_fall;
    }
    Ok(())
}
