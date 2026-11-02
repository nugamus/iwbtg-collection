//! WP5a.2: family A1 kid actions on hand-built worlds (docs/plans/sim-core.md 5.3, kid/A1.md).
//!
//! The profile is built by hand with the rely numbers; presets are WP5a.4. Kid mask 11x21 with
//! origin (5, 12), so the box is x-5..x+5 and y-12..y+8 (A1 "Object properties").

use iwbtg_sim::def::physics::{AirJump, FallCapRule, GroundAlso, VineKeys, VineModel, WalkRead, WaterAir};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, PhysicsProfile, RoomDef, SpriteId,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::a1::A1Action;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::{self, KidAction};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Flow, InstId, KindState, SpawnInit};
use iwbtg_sim::{Ctx, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
    block: KindId,
    platform: KindId,
    water2: KindId,
    vine_l: KindId,
    vine_r: KindId,
    bullet: KindId,
}

struct Sprites {
    idle: SpriteId,
    running: SpriteId,
    jump: SpriteId,
    fall: SpriteId,
    sliding: SpriteId,
}

/// The rely numbers (A1 "Numeric constants", plan 5.2).
fn a1_profile(s: &Sprites) -> PhysicsProfile {
    let mut p = PhysicsProfile::default();
    p.numbers.jumps = vec![r(8.5), r(7.0)];
    p.numbers.gravity = r(0.4);
    p.numbers.run_speed = r(3.0);
    p.numbers.max_vspeed = r(9.0);
    p.numbers.max_fall_speed = r(8.0);
    p.numbers.release_mult = r(0.45);
    p.numbers.sprite_threshold = r(0.05);
    p.numbers.image_speed_idle = r(0.2);
    p.numbers.image_speed_run = r(0.5);
    p.input.walk = WalkRead::DirectLatch;
    p.input.vine_keys = VineKeys::Edges;
    p.jump.ground_classes = ClassMask::BLOCK | ClassMask::SOLIDBLOCK | ClassMask::WATER1;
    p.jump.ground_also = vec![GroundAlso::PlatformFlag, GroundAlso::LadderJump];
    p.jump.ground_sets_djump = true;
    p.jump.air = AirJump::Djump;
    p.jump.water_air = WaterAir::Water2WithoutFlag;
    p.fall_cap.rule = FallCapRule::ClampAbs;
    p.platform.hold_probe = r(4.0);
    p.vine.model = VineModel::DistanceAsym;
    p.vine.left_below = r(2.0);
    p.vine.right_equal = r(1.0);
    p.vine.slide = r(2.0);
    p.vine.kick = [r(15.0), r(-9.0)];
    p.vine.drop = r(3.0);
    p.bullet.max = 4;
    p.bullet.speed = r(16.0);
    p.misc.fade_in = true;
    p.misc.fade_step = r(0.1);
    p.sprites.idle = Some(s.idle);
    p.sprites.running = Some(s.running);
    p.sprites.jump = Some(s.jump);
    p.sprites.fall = Some(s.fall);
    p.sprites.sliding = Some(s.sliding);
    p
}

fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds, Sprites) {
    let mut b = GameDefBuilder::new("wp5a-actions");
    let mask = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let sprites = Sprites {
        idle: b.sprite_rect((11, 21), (5, 12)),
        running: b.sprite_rect((11, 21), (5, 12)),
        jump: b.sprite_rect((11, 21), (5, 12)),
        fall: b.sprite_rect((11, 21), (5, 12)),
        sliding: b.sprite_rect((11, 21), (5, 12)),
    };
    let mut kid = KindDef::new("kid", Builtin::Kid);
    kid.sprite = Some(sprites.idle);
    kid.mask = None;
    kid.classes = ClassMask::KID;
    let _ = mask;
    let kid = b.kind(kid);
    let solid = |name: &str, classes: ClassMask, is_solid: bool, b: &mut GameDefBuilder| {
        let mut k = KindDef::new(name, Builtin::Inert);
        k.sprite = Some(tile);
        k.classes = classes;
        k.solid = is_solid;
        b.kind(k)
    };
    let block = solid("block", ClassMask::BLOCK, true, &mut b);
    let platform = solid("platform", ClassMask::PLATFORM | ClassMask::BLOCK, false, &mut b);
    let water2 = solid("water2", ClassMask::WATER2, false, &mut b);
    let vine_l = solid("vine_l", ClassMask::VINE_L, false, &mut b);
    let vine_r = solid("vine_r", ClassMask::VINE_R, false, &mut b);
    let mut bk = KindDef::new("bullet", Builtin::Inert);
    bk.classes = ClassMask::KID_BULLET;
    let bullet = b.kind(bk);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::KidBullet, bullet);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = a1_profile(&sprites);
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds {
            kid,
            block,
            platform,
            water2,
            vine_l,
            vine_r,
            bullet,
        },
        sprites,
    )
}

/// Runs `f` with a ctx, a kid at (100, 100) after `create_defaults`, and the kinds.
fn with_kid<T>(
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds, &Sprites) -> T,
) -> T {
    let (def, k, s) = game(tweak);
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let mut w = World::new(def.start_room, 1);
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    let kid = ctx
        .spawn(k.kid, r(100.0), r(100.0), SpawnInit::default())
        .expect("kid");
    run(&mut ctx, kid, CommonAction::CreateDefaults);
    f(&mut ctx, kid, &k, &s)
}

fn run(ctx: &mut Ctx, kid: InstId, a: CommonAction) {
    assert_eq!(
        kid::run_action(ctx, kid, &KidAction::Common(a)).expect("action"),
        Flow::Continue
    );
}

fn run_a1(ctx: &mut Ctx, kid: InstId, a: A1Action) {
    assert_eq!(
        kid::run_action(ctx, kid, &KidAction::A1(a)).expect("action"),
        Flow::Continue
    );
}

/// This tick's input: `held` and `pressed`/`released` given as lists of actions.
fn input(ctx: &mut Ctx, held: &[Action], pressed: &[Action], released: &[Action]) {
    let bits = |l: &[Action]| l.iter().fold(0u32, |a, x| a | x.bit());
    ctx.w.input.latch(InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: bits(released),
    });
}

fn spawn(ctx: &mut Ctx, kind: KindId, x: f64, y: f64) -> InstId {
    ctx.spawn(kind, r(x), r(y), SpawnInit::default()).expect("spawn")
}

fn set_v(ctx: &mut Ctx, id: InstId, hs: f64, vs: f64) {
    let i = ctx.w.get_mut(id).unwrap();
    i.m.set_hspeed(r(hs)).unwrap();
    i.m.set_vspeed(r(vs)).unwrap();
}

fn hs(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.hspeed().get()
}

fn vs(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.vspeed().get()
}

fn sprite(ctx: &Ctx, id: InstId) -> Option<SpriteId> {
    ctx.w.get(id).unwrap().img.sprite()
}

fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut iwbtg_sim::kid::KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

// ---- Create ----

#[test]
fn create_defaults_sets_the_a1_numbers() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            let s = kstate(ctx, kid).clone();
            assert_eq!(s.jump_speeds, vec![r(8.5), r(7.0)]);
            assert_eq!((s.run_speed, s.max_vspeed), (r(3.0), r(9.0)));
            assert!(s.djump && !s.frozen && !s.frozen2 && !s.invincible && !s.on_platform);
            assert_eq!(s.facing, 1);
            let i = ctx.w.get(kid).unwrap();
            assert_eq!(i.m.gravity(), r(0.4));
            assert_eq!(i.img.speed(), r(0.2));
            assert_eq!(i.img.alpha(), Real::ZERO, "rely fades in from 0");
        },
    );
}

#[test]
fn create_faces_left_only_when_the_global_says_so() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            ctx.w.engine.kid.image_xscale = -1;
            run(ctx, kid, CommonAction::CreateDefaults);
            assert_eq!(kstate(ctx, kid).facing, -1);
            assert_eq!(ctx.w.get(kid).unwrap().img.xscale(), r(-1.0));
            ctx.w.engine.kid.image_xscale = 0;
            run(ctx, kid, CommonAction::CreateDefaults);
            assert_eq!(kstate(ctx, kid).facing, 1, "a loaded kid faces right");
        },
    );
}

#[test]
fn create_without_fade_in_keeps_alpha() {
    with_kid(
        |p| p.misc.fade_in = false,
        |ctx, kid, _, _| {
            assert_eq!(ctx.w.get(kid).unwrap().img.alpha(), Real::ONE);
        },
    );
}

// ---- Walk ----

#[test]
fn walk_right_left_both_none() {
    with_kid(
        |_| {},
        |ctx, kid, _, s| {
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            run(ctx, kid, CommonAction::Walk);
            assert_eq!(hs(ctx, kid), 3.0);
            assert_eq!(sprite(ctx, kid), Some(s.running));
            assert_eq!(ctx.w.get(kid).unwrap().img.speed(), r(0.5));
            assert_eq!(kstate(ctx, kid).facing, 1);

            input(ctx, &[Action::Left], &[Action::Left], &[Action::Right]);
            run(ctx, kid, CommonAction::Walk);
            assert_eq!(hs(ctx, kid), -3.0);
            assert_eq!(kstate(ctx, kid).facing, -1);
            assert_eq!(ctx.w.get(kid).unwrap().img.xscale(), r(-1.0));

            input(ctx, &[Action::Left, Action::Right], &[], &[]);
            run(ctx, kid, CommonAction::Walk);
            assert_eq!(hs(ctx, kid), 3.0, "right wins when both are held");

            input(ctx, &[], &[], &[Action::Left, Action::Right]);
            run(ctx, kid, CommonAction::Walk);
            assert_eq!(hs(ctx, kid), 0.0);
            assert_eq!(sprite(ctx, kid), Some(s.idle));
            assert_eq!(ctx.w.get(kid).unwrap().img.speed(), r(0.2));
        },
    );
}

#[test]
fn walk_frozen_forces_zero() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            set_v(ctx, kid, 3.0, 0.0);
            kstate(ctx, kid).frozen = true;
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            run(ctx, kid, CommonAction::Walk);
            assert_eq!(hs(ctx, kid), 0.0);
        },
    );
}

#[test]
fn walk_skipped_while_frozen2() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            set_v(ctx, kid, 2.0, 0.0);
            kstate(ctx, kid).frozen2 = true;
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            run(ctx, kid, CommonAction::Walk);
            assert_eq!(hs(ctx, kid), 2.0);
        },
    );
}

#[test]
fn walk_direct_latch_sees_a_tap_but_sampled_does_not() {
    let tap = |ctx: &mut Ctx| {
        // Pressed and released inside the tick: not held at its end.
        ctx.w.input.latch(InputFrame {
            held: 0,
            pressed: Action::Right.bit(),
            released: Action::Right.bit(),
        });
    };
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            tap(ctx);
            run(ctx, kid, CommonAction::Walk);
            assert_eq!(hs(ctx, kid), 3.0, "the latch carries the tap");
        },
    );
    with_kid(
        |p| p.input.walk = WalkRead::Sampled,
        |ctx, kid, _, _| {
            tap(ctx);
            run(ctx, kid, CommonAction::Walk);
            assert_eq!(hs(ctx, kid), 0.0);
        },
    );
}

#[test]
fn walk_into_a_right_vine_keeps_hspeed_and_facing() {
    with_kid(
        |_| {},
        |ctx, kid, _, s| {
            // Flush right vine: distance exactly 1 (kid box ends at x+5 = 105; vine starts at 106).
            spawn(ctx, ctx_kind(ctx, "vine_r"), 106.0, 80.0);
            set_v(ctx, kid, 1.5, 0.0);
            input(ctx, &[Action::Left], &[Action::Left], &[]);
            run(ctx, kid, CommonAction::Walk);
            assert_eq!(hs(ctx, kid), 1.5, "left is blocked: hspeed keeps its old value");
            assert_eq!(
                kstate(ctx, kid).facing,
                1,
                "facing does not change next to the vine"
            );
            assert_ne!(sprite(ctx, kid), Some(s.running));
            // Walking away from the vine (right) is not blocked by it.
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            run(ctx, kid, CommonAction::Walk);
            assert_eq!(hs(ctx, kid), 3.0);
        },
    );
}

fn ctx_kind(ctx: &Ctx, name: &str) -> KindId {
    let i = ctx.def.kinds.iter().position(|k| k.name == name).expect("kind");
    KindId(u32::try_from(i).unwrap())
}

// ---- Air sprite ----

#[test]
fn air_sprite_thresholds() {
    with_kid(
        |_| {},
        |ctx, kid, _, s| {
            set_v(ctx, kid, 0.0, -1.0);
            run(ctx, kid, CommonAction::AirSprite);
            assert_eq!(sprite(ctx, kid), Some(s.jump));
            set_v(ctx, kid, 0.0, 1.0);
            run(ctx, kid, CommonAction::AirSprite);
            assert_eq!(sprite(ctx, kid), Some(s.fall));
            set_v(ctx, kid, 0.0, -0.04);
            run(ctx, kid, CommonAction::AirSprite);
            assert_eq!(sprite(ctx, kid), Some(s.fall), "inside the threshold: unchanged");
        },
    );
}

#[test]
fn air_sprite_on_platform_keeps_flag_only_with_a_platform_below() {
    with_kid(
        |_| {},
        |ctx, kid, k, s| {
            kstate(ctx, kid).on_platform = true;
            set_v(ctx, kid, 0.0, -1.0);
            // Platform 4 px below the kid's box: box bottom 108, platform top 112.
            let p = spawn(ctx, k.platform, 84.0, 112.0);
            run(ctx, kid, CommonAction::AirSprite);
            assert!(kstate(ctx, kid).on_platform);
            assert_ne!(
                sprite(ctx, kid),
                Some(s.jump),
                "sprite choice is skipped on a platform"
            );
            ctx.w.get_mut(p).unwrap().m.set_y(r(113.0)).unwrap();
            run(ctx, kid, CommonAction::AirSprite);
            assert!(!kstate(ctx, kid).on_platform, "5 px away: released");
        },
    );
}

// ---- Fall cap ----

#[test]
fn fall_cap_clamps_abs() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            for (before, after) in [(12.0, 9.0), (-12.0, -9.0), (8.0, 8.0), (-9.0, -9.0)] {
                set_v(ctx, kid, 0.0, before);
                run(ctx, kid, CommonAction::FallCap);
                assert_eq!(vs(ctx, kid), after);
            }
        },
    );
}

#[test]
fn fall_cap_other_rules_are_unsupported() {
    with_kid(
        |p| p.fall_cap.rule = FallCapRule::ShiftThenClamp,
        |ctx, kid, _, _| {
            assert!(kid::run_action(ctx, kid, &KidAction::Common(CommonAction::FallCap)).is_err());
        },
    );
}

// ---- Shoot ----

#[test]
fn shoot_spawns_a_bullet_moving_with_facing() {
    with_kid(
        |_| {},
        |ctx, kid, k, _| {
            input(ctx, &[Action::Shoot], &[Action::Shoot], &[]);
            run(ctx, kid, CommonAction::Shoot);
            let b = ctx
                .w
                .first_of(ClassMask::KID_BULLET, &ctx.rules.tick)
                .expect("bullet");
            assert_eq!(ctx.w.get(b).unwrap().kind, k.bullet);
            assert_eq!(hs(ctx, b), 16.0);
            assert_eq!(
                (ctx.w.get(b).unwrap().m.x(), ctx.w.get(b).unwrap().m.y()),
                (r(100.0), r(100.0))
            );
            kstate(ctx, kid).facing = -1;
            run(ctx, kid, CommonAction::Shoot);
            assert_eq!(ctx.w.count_of(ClassMask::KID_BULLET), 2);
            let last = ctx
                .w
                .iter_alive()
                .filter(|i| i.kind == k.bullet)
                .last()
                .unwrap()
                .id;
            assert_eq!(hs(ctx, last), -16.0);
        },
    );
}

#[test]
fn shoot_stops_at_the_bullet_limit_and_needs_a_press_and_no_freeze() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            input(ctx, &[Action::Shoot], &[], &[]);
            run(ctx, kid, CommonAction::Shoot);
            assert_eq!(ctx.w.count_of(ClassMask::KID_BULLET), 0, "held is not pressed");
            input(ctx, &[Action::Shoot], &[Action::Shoot], &[]);
            for _ in 0..6 {
                run(ctx, kid, CommonAction::Shoot);
            }
            assert_eq!(ctx.w.count_of(ClassMask::KID_BULLET), 4, "fewer than 4 may exist");
        },
    );
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            kstate(ctx, kid).frozen = true;
            input(ctx, &[Action::Shoot], &[Action::Shoot], &[]);
            run(ctx, kid, CommonAction::Shoot);
            assert_eq!(ctx.w.count_of(ClassMask::KID_BULLET), 0);
        },
    );
}

// ---- Jump ----

#[test]
fn ground_jump_from_a_block_one_pixel_below() {
    with_kid(
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.block, 80.0, 109.0);
            kstate(ctx, kid).djump = false;
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), -8.5);
            assert!(kstate(ctx, kid).djump, "a ground jump refreshes the double jump");
        },
    );
}

#[test]
fn ground_jump_ignores_vspeed() {
    with_kid(
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.block, 80.0, 109.0);
            set_v(ctx, kid, 0.0, 5.0);
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), -8.5);
        },
    );
}

#[test]
fn a_block_two_pixels_below_is_air() {
    with_kid(
        |_| {},
        |ctx, kid, k, s| {
            spawn(ctx, k.block, 80.0, 110.0);
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), -7.0, "air jump");
            assert!(!kstate(ctx, kid).djump);
            assert_eq!(sprite(ctx, kid), Some(s.jump));
        },
    );
}

#[test]
fn air_jump_needs_djump_unless_water2_is_below() {
    with_kid(
        |_| {},
        |ctx, kid, k, _| {
            kstate(ctx, kid).djump = false;
            set_v(ctx, kid, 0.0, 2.0);
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), 2.0, "no djump: nothing");
            spawn(ctx, k.water2, 80.0, 109.0);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), -7.0, "water 2 below allows it without djump");
            assert!(!kstate(ctx, kid).djump);
        },
    );
}

#[test]
fn on_platform_counts_as_ground_and_a_platform_is_also_a_block() {
    with_kid(
        |_| {},
        |ctx, kid, k, _| {
            kstate(ctx, kid).on_platform = true;
            kstate(ctx, kid).djump = false;
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), -8.5);
            assert!(kstate(ctx, kid).djump);
            // A platform one pixel below counts as a block through its classes.
            kstate(ctx, kid).on_platform = false;
            spawn(ctx, k.platform, 80.0, 109.0);
            set_v(ctx, kid, 0.0, 0.0);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), -8.5);
        },
    );
}

#[test]
fn jump_needs_a_press_and_no_freeze() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            input(ctx, &[Action::Jump], &[], &[]);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), 0.0, "held is not pressed");
            kstate(ctx, kid).frozen = true;
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), 0.0);
        },
    );
}

#[test]
fn other_water_air_models_are_unsupported() {
    with_kid(
        |p| p.jump.water_air = WaterAir::Water3Keeps,
        |ctx, kid, _, _| {
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            assert!(kid::run_action(ctx, kid, &KidAction::Common(CommonAction::Jump)).is_err());
        },
    );
}

// ---- Release ----

#[test]
fn release_cuts_upward_speed_every_time() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            set_v(ctx, kid, 0.0, -8.0);
            input(ctx, &[], &[], &[Action::Jump]);
            run(ctx, kid, CommonAction::Release);
            assert_eq!(vs(ctx, kid), (r(-8.0) * r(0.45)).get());
            run(ctx, kid, CommonAction::Release);
            assert_eq!(
                vs(ctx, kid),
                (r(-8.0) * r(0.45) * r(0.45)).get(),
                "a second release cuts again"
            );
            set_v(ctx, kid, 0.0, 2.0);
            run(ctx, kid, CommonAction::Release);
            assert_eq!(vs(ctx, kid), 2.0, "falling speed is untouched");
        },
    );
}

#[test]
fn release_needs_the_release_edge_and_no_freeze() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            set_v(ctx, kid, 0.0, -8.0);
            input(ctx, &[Action::Jump], &[], &[]);
            run(ctx, kid, CommonAction::Release);
            assert_eq!(vs(ctx, kid), -8.0);
            kstate(ctx, kid).frozen = true;
            input(ctx, &[], &[], &[Action::Jump]);
            run(ctx, kid, CommonAction::Release);
            assert_eq!(vs(ctx, kid), -8.0);
        },
    );
}

// ---- Vines ----

#[test]
fn left_vine_grabs_slides_and_faces_right() {
    with_kid(
        |_| {},
        |ctx, kid, k, s| {
            // Flush left vine: kid box starts at 95, vine ends at 94.
            spawn(ctx, k.vine_l, 63.0, 80.0);
            input(ctx, &[], &[], &[]);
            run(ctx, kid, CommonAction::Vines);
            assert_eq!(vs(ctx, kid), 2.0);
            assert_eq!(sprite(ctx, kid), Some(s.sliding));
            assert_eq!(kstate(ctx, kid).facing, 1);
            // Action 4 step 4 then sets maxFallSpeed back to 8 (no right vine is near); it is never read in A1.
            assert_eq!(kstate(ctx, kid).max_fall_speed, r(8.0));
        },
    );
}

#[test]
fn left_vine_kick_and_drop() {
    with_kid(
        |_| {},
        |ctx, kid, k, s| {
            spawn(ctx, k.vine_l, 63.0, 80.0);
            input(ctx, &[Action::Right, Action::Jump], &[Action::Right], &[]);
            run(ctx, kid, CommonAction::Vines);
            assert_eq!((hs(ctx, kid), vs(ctx, kid)), (15.0, -9.0));
            assert_eq!(sprite(ctx, kid), Some(s.jump));
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            run(ctx, kid, CommonAction::Vines);
            assert_eq!(hs(ctx, kid), 3.0, "no Jump held: drop away from the vine");
            assert_eq!(sprite(ctx, kid), Some(s.fall));
        },
    );
}

#[test]
fn right_vine_needs_distance_exactly_one() {
    with_kid(
        |_| {},
        |ctx, kid, k, s| {
            spawn(ctx, k.vine_r, 106.0, 80.0);
            input(ctx, &[Action::Left, Action::Jump], &[Action::Left], &[]);
            run(ctx, kid, CommonAction::Vines);
            assert_eq!((hs(ctx, kid), vs(ctx, kid)), (-15.0, -9.0));
            assert_eq!(kstate(ctx, kid).facing, -1);
            assert_eq!(ctx.w.get(kid).unwrap().img.xscale(), r(-1.0));
            assert_eq!(sprite(ctx, kid), Some(s.jump));
        },
    );
    // Half a pixel off: the distance is not 1, so the right vine misses (bug.right_vine_miss).
    with_kid(
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.vine_r, 107.0, 80.0);
            input(ctx, &[Action::Left], &[Action::Left], &[]);
            run(ctx, kid, CommonAction::Vines);
            assert_eq!(vs(ctx, kid), 0.0);
        },
    );
}

#[test]
fn vines_need_free_space_below() {
    with_kid(
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.vine_l, 63.0, 80.0);
            spawn(ctx, k.block, 80.0, 109.0);
            input(ctx, &[], &[], &[]);
            run(ctx, kid, CommonAction::Vines);
            assert_eq!(vs(ctx, kid), 0.0, "standing on a block: no slide");
        },
    );
}

#[test]
fn vine_model_none_does_nothing() {
    with_kid(
        |p| p.vine.model = VineModel::None,
        |ctx, kid, k, _| {
            spawn(ctx, k.vine_l, 63.0, 80.0);
            input(ctx, &[], &[], &[]);
            run(ctx, kid, CommonAction::Vines);
            assert_eq!(vs(ctx, kid), 0.0);
        },
    );
}

// ---- Gravity, platform gravity zero, fade in ----

#[test]
fn gravity_is_reset_each_step_unless_on_a_ladder() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            ctx.w.get_mut(kid).unwrap().m.set_gravity(Real::ZERO).unwrap();
            run(ctx, kid, CommonAction::Gravity);
            assert_eq!(ctx.w.get(kid).unwrap().m.gravity(), r(0.4));
            kstate(ctx, kid).ladder = true;
            ctx.w.get_mut(kid).unwrap().m.set_gravity(Real::ZERO).unwrap();
            run(ctx, kid, CommonAction::Gravity);
            assert_eq!(ctx.w.get(kid).unwrap().m.gravity(), Real::ZERO);
        },
    );
}

#[test]
fn platform_gravity_zero_only_on_a_platform() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            run_a1(ctx, kid, A1Action::PlatformGravityZero);
            assert_eq!(ctx.w.get(kid).unwrap().m.gravity(), r(0.4));
            kstate(ctx, kid).on_platform = true;
            run_a1(ctx, kid, A1Action::PlatformGravityZero);
            assert_eq!(ctx.w.get(kid).unwrap().m.gravity(), Real::ZERO);
        },
    );
}

#[test]
fn fade_in_adds_a_tenth_until_one_under_the_gm_epsilon() {
    with_kid(
        |_| {},
        |ctx, kid, _, _| {
            let mut steps = 0;
            let mut expect = Real::ZERO;
            // GM compares with an epsilon, so 0.9999999999999999 already counts as 1 (N1).
            while ctx.w.get(kid).unwrap().img.alpha().get() < 0.5 || steps < 10 {
                run_a1(ctx, kid, A1Action::FadeIn);
                expect += r(0.1);
                steps += 1;
                assert_eq!(ctx.w.get(kid).unwrap().img.alpha(), expect);
            }
            assert_eq!(steps, 10);
            run_a1(ctx, kid, A1Action::FadeIn);
            assert_eq!(
                ctx.w.get(kid).unwrap().img.alpha(),
                expect,
                "alpha is 1 to GM: unchanged"
            );
        },
    );
}

// ---- Event lists ----

#[test]
fn step_list_runs_in_order_so_a_press_and_release_cut_at_once() {
    let steps = vec![
        KidAction::Common(CommonAction::Jump),
        KidAction::Common(CommonAction::Release),
    ];
    let tweak = |p: &mut PhysicsProfile| p.events.step = steps.clone();
    with_kid(tweak, |ctx, kid, k, _| {
        spawn(ctx, k.block, 80.0, 109.0);
        input(ctx, &[], &[Action::Jump], &[Action::Jump]);
        let flow = kid::on_event(ctx, kid, &EventKind::Step).expect("step");
        assert_eq!(flow, Flow::Continue);
        assert_eq!(
            vs(ctx, kid),
            (r(-8.5) * r(0.45)).get(),
            "jump then cut in the same Step"
        );
    });
}

#[test]
fn event_set_follows_the_lists() {
    let mut p = PhysicsProfile::default();
    assert!(!kid::event_set(&p).step);
    p.events.step = vec![KidAction::Common(CommonAction::Walk)];
    p.events.end_step = vec![KidAction::Common(CommonAction::Gravity)];
    let e = kid::event_set(&p);
    assert!(e.step && e.end_step && !e.room_start);
}

/// #268: A1 Step action 3 (walk, sprites, clamp, shoot, jump, release) is wholly under frozen2,
/// by the kid's own flag or `global.frozen2`.
#[test]
fn all_of_step_action_3_is_skipped_while_frozen2() {
    for global in [false, true] {
        with_kid(
            |_| {},
            |ctx, kid, k, _| {
                spawn(ctx, k.block, 80.0, 109.0);
                if global {
                    ctx.w.engine.kid.frozen2 = true;
                } else {
                    kstate(ctx, kid).frozen2 = true;
                }
                set_v(ctx, kid, 2.0, 12.0);
                let before = sprite(ctx, kid);
                input(
                    ctx,
                    &[Action::Right, Action::Jump, Action::Shoot],
                    &[Action::Right, Action::Jump, Action::Shoot],
                    &[Action::Jump],
                );
                for a in [
                    CommonAction::Walk,
                    CommonAction::AirSprite,
                    CommonAction::FallCap,
                    CommonAction::Shoot,
                    CommonAction::Jump,
                    CommonAction::Release,
                ] {
                    run(ctx, kid, a);
                }
                assert_eq!((hs(ctx, kid), vs(ctx, kid)), (2.0, 12.0), "global {global}");
                assert_eq!(sprite(ctx, kid), before);
                assert_eq!(ctx.w.count_of(ClassMask::KID_BULLET), 0);
                // Not frozen: the same calls act.
                ctx.w.engine.kid.frozen2 = false;
                kstate(ctx, kid).frozen2 = false;
                run(ctx, kid, CommonAction::FallCap);
                assert_eq!(vs(ctx, kid), 9.0);
            },
        );
    }
}
