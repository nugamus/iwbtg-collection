//! WP5b.2 (#260): family B's `yflag` flip on hand-built worlds (kid/B.md Step, playerMove,
//! playerShoot, playerWallJump, playerReverse, Room Start).

use iwbtg_sim::def::physics::{AirJump, FallCapRule, FlipKind, GravityReapply, GroundAlso, WaterAir};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, PhysicsProfile, RoomDef, SpriteId,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::b::BAction;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::{self, KidAction, KidState};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
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

fn b_profile(s: &Sprites) -> PhysicsProfile {
    let mut p = PhysicsProfile::default();
    p.numbers.jumps = vec![r(8.5), r(7.0)];
    p.numbers.gravity = r(0.4);
    p.numbers.run_speed = r(3.0);
    p.numbers.max_vspeed = r(9.0);
    p.numbers.max_fall_speed = r(8.0);
    p.numbers.release_mult = r(0.45);
    p.numbers.sprite_threshold = r(0.05);
    p.jump.ground_classes = ClassMask::BLOCK;
    p.jump.ground_also = vec![GroundAlso::PlatformFlag];
    p.jump.air = AirJump::CurJumps;
    p.jump.water_air = WaterAir::WaterUnlimited;
    p.jump.max_jumps = 2;
    p.fall_cap.rule = FallCapRule::ClampAgainstGravity;
    p.gravity.reapply = GravityReapply::EndStepSlope;
    p.flip.model = FlipKind::Yflag;
    p.vine.model = iwbtg_sim::def::physics::VineModel::DistanceAsym;
    p.vine.left_below = r(2.0);
    p.vine.right_equal = r(1.0);
    p.vine.slide = r(2.0);
    p.vine.kick = [r(15.0), r(-9.0)];
    p.vine.drop = r(3.0);
    p.bullet.max = 4;
    p.bullet.speed = r(16.0);
    p.bullet.offset = [r(2.0), r(2.0)];
    p.sprites.idle = Some(s.idle);
    p.sprites.running = Some(s.running);
    p.sprites.jump = Some(s.jump);
    p.sprites.fall = Some(s.fall);
    p.sprites.sliding = Some(s.sliding);
    p
}

fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds, Sprites) {
    let mut b = GameDefBuilder::new("wp5b-flip");
    let tile = b.sprite_rect((32, 32), (0, 0));
    let sprites = Sprites {
        idle: b.sprite_rect((11, 21), (5, 12)),
        running: b.sprite_rect((11, 21), (5, 12)),
        jump: b.sprite_rect((11, 21), (5, 12)),
        fall: b.sprite_rect((11, 21), (5, 12)),
        sliding: b.sprite_rect((11, 21), (5, 12)),
    };
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(sprites.idle);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let vine = |name: &str, c: ClassMask, b: &mut GameDefBuilder| {
        let mut k = KindDef::new(name, Builtin::Inert);
        k.sprite = Some(tile);
        k.classes = c;
        b.kind(k)
    };
    let vine_l = vine("vine_l", ClassMask::VINE_L, &mut b);
    let vine_r = vine("vine_r", ClassMask::VINE_R, &mut b);
    let mut bk = KindDef::new("bullet", Builtin::Inert);
    bk.classes = ClassMask::KID_BULLET;
    let bullet = b.kind(bk);
    let mut bow = KindDef::new("bow", Builtin::Inert);
    bow.classes = ClassMask::BOW;
    let bow = b.kind(bow);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::KidBullet, bullet);
    b.engine_kind(EngineKind::Bow, bow);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = b_profile(&sprites);
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds {
            kid,
            vine_l,
            vine_r,
            bullet,
        },
        sprites,
    )
}

/// A kid at (100, 100) after `create_defaults` (global reverse as `reverse`).
fn with_kid<T>(
    reverse: bool,
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds, &Sprites) -> T,
) -> T {
    let (def, k, s) = game(tweak);
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let mut w = World::new(def.start_room, 1);
    w.room_speed = r(50.0);
    w.engine.kid.b.reverse = reverse;
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
    run(&mut ctx, kid, KidAction::Common(CommonAction::CreateDefaults));
    f(&mut ctx, kid, &k, &s)
}

fn run(ctx: &mut Ctx, kid: InstId, a: KidAction) {
    kid::run_action(ctx, kid, &a).expect("action");
}

fn common(ctx: &mut Ctx, kid: InstId, a: CommonAction) {
    run(ctx, kid, KidAction::Common(a));
}

fn b(ctx: &mut Ctx, kid: InstId, a: BAction) {
    run(ctx, kid, KidAction::B(a));
}

fn input(ctx: &mut Ctx, held: &[Action], pressed: &[Action]) {
    let bits = |l: &[Action]| l.iter().fold(0u32, |a, x| a | x.bit());
    ctx.w.input.latch(InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: 0,
    });
}

fn spawn(ctx: &mut Ctx, kind: KindId, x: f64, y: f64) -> InstId {
    ctx.spawn(kind, r(x), r(y), SpawnInit::default()).expect("spawn")
}

fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

fn set_v(ctx: &mut Ctx, id: InstId, hs: f64, vs: f64) {
    let m = &mut ctx.w.get_mut(id).unwrap().m;
    m.set_hspeed(r(hs)).unwrap();
    m.set_vspeed(r(vs)).unwrap();
}

fn hv(ctx: &Ctx, id: InstId) -> (f64, f64) {
    let m = &ctx.w.get(id).unwrap().m;
    (m.hspeed().get(), m.vspeed().get())
}

fn sprite(ctx: &Ctx, id: InstId) -> Option<SpriteId> {
    ctx.w.get(id).unwrap().img.sprite()
}

fn yscale(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().img.yscale().get()
}

// ---- playerMove: yflag follows the global one tick late ----

#[test]
fn sync_yflag_scales_with_last_ticks_flag_then_follows_the_global() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, _| {
            ctx.w.engine.kid.b.reverse = true;
            b(ctx, kid, BAction::SyncYflag);
            assert_eq!(yscale(ctx, kid), 1.0, "image_yscale still has the old yflag");
            assert_eq!((kstate(ctx, kid).yflag, kstate(ctx, kid).flipped), (-1, true));
            b(ctx, kid, BAction::SyncYflag);
            assert_eq!(yscale(ctx, kid), -1.0);
            ctx.w.engine.kid.b.reverse = false;
            b(ctx, kid, BAction::SyncYflag);
            assert_eq!((kstate(ctx, kid).yflag, yscale(ctx, kid)), (1, -1.0));
        },
    );
}

#[test]
fn a_retry_kid_with_a_stale_yflag_gets_the_scale_fixed_on_the_first_step() {
    // The kid was created reversed (yflag -1, yscale -1); the load then said "not reversed".
    with_kid(
        true,
        |_| {},
        |ctx, kid, _, _| {
            ctx.w.engine.kid.b.reverse = false;
            b(ctx, kid, BAction::SyncYflag);
            assert_eq!(yscale(ctx, kid), -1.0, "the whole first tick has the wrong scale");
            assert_eq!(kstate(ctx, kid).yflag, 1);
            b(ctx, kid, BAction::SyncYflag);
            assert_eq!(yscale(ctx, kid), 1.0);
        },
    );
}

#[test]
fn a_step_list_syncs_before_it_walks() {
    let step = vec![
        KidAction::B(BAction::SyncYflag),
        KidAction::Common(CommonAction::Walk),
    ];
    with_kid(
        false,
        |p| p.events.step = step.clone(),
        |ctx, kid, _, _| {
            ctx.w.engine.kid.b.reverse = true;
            input(ctx, &[Action::Right], &[]);
            kid::on_event(ctx, kid, &EventKind::Step).unwrap();
            assert_eq!(
                hv(ctx, kid).0,
                -3.0,
                "reversed in the same tick: the keys already swapped"
            );
        },
    );
}

// ---- Walk, sprites ----

#[test]
fn reversed_swaps_the_walk_keys() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, _| {
            kstate(ctx, kid).yflag = -1;
            input(ctx, &[Action::Right], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(hv(ctx, kid).0, -3.0, "Right walks left");
            assert_eq!(kstate(ctx, kid).facing, -1);
            input(ctx, &[Action::Left], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(hv(ctx, kid).0, 3.0, "Left walks right");
            // Both held: R (the left key) wins, so it still reads as moving right.
            input(ctx, &[Action::Left, Action::Right], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(hv(ctx, kid).0, 3.0);
        },
    );
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, _| {
            input(ctx, &[Action::Right], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(hv(ctx, kid).0, 3.0);
        },
    );
}

#[test]
fn reversed_swaps_the_jump_and_fall_sprites() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, s| {
            set_v(ctx, kid, 0.0, -1.0);
            common(ctx, kid, CommonAction::AirSprite);
            assert_eq!(sprite(ctx, kid), Some(s.jump));
            set_v(ctx, kid, 0.0, 1.0);
            common(ctx, kid, CommonAction::AirSprite);
            assert_eq!(sprite(ctx, kid), Some(s.fall));
            kstate(ctx, kid).yflag = -1;
            set_v(ctx, kid, 0.0, -1.0);
            common(ctx, kid, CommonAction::AirSprite);
            assert_eq!(sprite(ctx, kid), Some(s.fall), "rising up is falling");
            set_v(ctx, kid, 0.0, 1.0);
            common(ctx, kid, CommonAction::AirSprite);
            assert_eq!(sprite(ctx, kid), Some(s.jump));
        },
    );
}

// ---- Shoot ----

#[test]
fn the_bullet_appears_at_x_plus_2_facing_and_y_minus_2_yflag() {
    for (yflag, facing, bx, by) in [
        (1i8, 1i8, 102.0, 98.0),
        (1, -1, 98.0, 98.0),
        (-1, 1, 102.0, 102.0),
    ] {
        with_kid(
            false,
            |_| {},
            |ctx, kid, k, _| {
                kstate(ctx, kid).yflag = yflag;
                kstate(ctx, kid).facing = facing;
                input(ctx, &[Action::Shoot], &[Action::Shoot]);
                common(ctx, kid, CommonAction::Shoot);
                let bullet = ctx.w.iter_alive().find(|i| i.kind == k.bullet).expect("bullet");
                assert_eq!(
                    (bullet.m.x().get(), bullet.m.y().get()),
                    (bx, by),
                    "yflag {yflag} facing {facing}"
                );
                assert_eq!(bullet.m.hspeed().get(), 16.0 * f64::from(facing));
            },
        );
    }
}

// ---- Vines ----

#[test]
fn a_reversed_kid_slides_and_kicks_up_the_other_way() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, k, s| {
            kstate(ctx, kid).yflag = -1;
            // Left vine within reach: its right edge touches the kid's left edge.
            spawn(ctx, k.vine_l, 64.0, 96.0);
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid).1, -2.0, "slide is 2 * yflag");
            assert_eq!(sprite(ctx, kid), Some(s.sliding));
            // Reversed, the away key is Left (R reads the left key): Right does nothing.
            input(ctx, &[Action::Jump, Action::Right], &[Action::Right]);
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid).0, 0.0);
            input(ctx, &[Action::Jump, Action::Left], &[Action::Left]);
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid), (15.0, 9.0), "kick: hspeed +15, vspeed -9 * yflag");
            assert_eq!(sprite(ctx, kid), Some(s.jump));
            // Without jump held: the drop.
            input(ctx, &[Action::Left], &[Action::Left]);
            common(ctx, kid, CommonAction::Vines);
            assert_eq!((hv(ctx, kid).0, sprite(ctx, kid)), (3.0, Some(s.fall)));
        },
    );
}

#[test]
fn a_right_vine_reversed_uses_the_right_key() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, k, _| {
            kstate(ctx, kid).yflag = -1;
            spawn(ctx, k.vine_r, 106.0, 96.0);
            input(ctx, &[Action::Jump, Action::Right], &[Action::Right]);
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid), (-15.0, 9.0));
            assert_eq!(kstate(ctx, kid).facing, -1);
        },
    );
}

#[test]
fn an_unreversed_kid_is_unchanged() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.vine_l, 64.0, 96.0);
            input(ctx, &[Action::Jump, Action::Right], &[Action::Right]);
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid), (15.0, -9.0));
        },
    );
}

// ---- playerReverse ----

#[test]
fn player_reverse_flips_everything_and_a_second_call_undoes_it() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, _| {
            kstate(ctx, kid).cur_jumps = 3;
            set_v(ctx, kid, 1.0, 5.0);
            b(ctx, kid, BAction::PlayerReverse);
            assert!(ctx.w.engine.kid.b.reverse);
            let s = kstate(ctx, kid).clone();
            assert_eq!((s.yflag, s.cur_jumps), (-1, 1));
            assert_eq!(ctx.w.get(kid).unwrap().m.y(), r(96.0), "y + 4 * -1");
            assert_eq!(hv(ctx, kid), (1.0, 0.0));
            // The scale follows on the next Step.
            b(ctx, kid, BAction::SyncYflag);
            b(ctx, kid, BAction::SyncYflag);
            assert_eq!(yscale(ctx, kid), -1.0);
            b(ctx, kid, BAction::PlayerReverse);
            assert!(!ctx.w.engine.kid.b.reverse);
            assert_eq!(kstate(ctx, kid).yflag, 1);
            assert_eq!(ctx.w.get(kid).unwrap().m.y(), r(100.0));
        },
    );
}

#[test]
fn the_api_function_does_the_same() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, _| {
            iwbtg_sim::kid::api::player_reverse(ctx, kid).unwrap();
            assert_eq!((ctx.w.engine.kid.b.reverse, kstate(ctx, kid).yflag), (true, -1));
        },
    );
}

// ---- Room Start ----

#[test]
fn room_start_turns_the_view_while_reversed() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, _, _| {
            b(ctx, kid, BAction::SetViewAngle);
            assert!(ctx.w.views[0].enabled);
            assert_eq!(ctx.w.views[0].angle, r(180.0));
            ctx.w.engine.kid.b.reverse = false;
            b(ctx, kid, BAction::SetViewAngle);
            assert_eq!(ctx.w.views[0].angle, Real::ZERO);
        },
    );
}

#[test]
fn ensure_bow_makes_one_bow() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, _| {
            assert_eq!(ctx.w.count_of(ClassMask::BOW), 0);
            b(ctx, kid, BAction::EnsureBow);
            b(ctx, kid, BAction::EnsureBow);
            assert_eq!(ctx.w.count_of(ClassMask::BOW), 1);
        },
    );
}
