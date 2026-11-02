//! WP5b.3 (#261): family B's block and platform contact, `killer_check` and Outside Room on
//! hand-built worlds (kid/B.md "Collision with block", "Collision with platform", playerMisc,
//! "Key Q pressed and Outside Room").
//!
//! As in wp5a_collisions, each test starts with the kid at the position the solid put-back leaves
//! it in and the speeds it had.

use iwbtg_sim::def::physics::{BoundaryModel, DeathStep, FlipKind, PlatformTake};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, PhysicsProfile, RoomDef};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::kid::actions::b::BAction;
use iwbtg_sim::kid::collisions::block::BlockRun;
use iwbtg_sim::kid::collisions::platform::PlatformRun;
use iwbtg_sim::kid::{self, KidAction, KidCollisionRun, KidState};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, Flow, InstId, KindState, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
    block: KindId,
    platform: KindId,
    killer: KindId,
    warp: KindId,
}

fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("wp5b-collisions");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let mut kid = KindDef::new("kid", Builtin::Kid);
    kid.sprite = Some(kid_sprite);
    kid.classes = ClassMask::KID;
    let kid = b.kind(kid);
    let inert = |name: &str, classes: ClassMask, solid: bool, b: &mut GameDefBuilder| {
        let mut k = KindDef::new(name, Builtin::Inert);
        k.sprite = Some(tile);
        k.classes = classes;
        k.solid = solid;
        b.kind(k)
    };
    let block = inert("block", ClassMask::BLOCK, true, &mut b);
    let platform = inert("platform", ClassMask::PLATFORM | ClassMask::BLOCK, false, &mut b);
    let killer = inert("killer", ClassMask::KILLER, false, &mut b);
    let warp = inert("warp", ClassMask::ROOM_CHANGER, false, &mut b);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = PhysicsProfile::default();
    p.gravity.block_landing_zeroes = true;
    p.flip.model = FlipKind::Yflag;
    p.platform.snap = r(9.0);
    p.platform.take = PlatformTake::DeltaY;
    p.platform.reversed_above = r(15.0);
    p.platform.reversed_snap = r(24.0);
    p.events.boundary = BoundaryModel::OutsideRoomKill;
    p.death.order = vec![DeathStep::DestroyKid];
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds {
            kid,
            block,
            platform,
            killer,
            warp,
        },
    )
}

/// A kid at (100, 100), gravity 0.4 (or -0.4 reversed), `yflag` as given.
fn with_kid<T>(
    yflag: i8,
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds) -> T,
) -> T {
    let (def, k) = game(tweak);
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
    ctx.w
        .get_mut(kid)
        .unwrap()
        .m
        .set_gravity(r(0.4 * f64::from(yflag)))
        .unwrap();
    ctx.w
        .get_mut(kid)
        .unwrap()
        .img
        .set_yscale(r(f64::from(yflag)))
        .unwrap();
    kstate(&mut ctx, kid).yflag = yflag;
    kstate(&mut ctx, kid).flipped = yflag < 0;
    f(&mut ctx, kid, &k)
}

fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

fn spawn(ctx: &mut Ctx, kind: KindId, x: f64, y: f64) -> InstId {
    ctx.spawn(kind, r(x), r(y), SpawnInit::default()).expect("spawn")
}

fn set_v(ctx: &mut Ctx, id: InstId, hs: f64, vs: f64) {
    let i = ctx.w.get_mut(id).unwrap();
    i.m.set_hspeed(r(hs)).unwrap();
    i.m.set_vspeed(r(vs)).unwrap();
}

fn pos(ctx: &Ctx, id: InstId) -> (f64, f64) {
    let m = &ctx.w.get(id).unwrap().m;
    (m.x().get(), m.y().get())
}

fn speeds(ctx: &Ctx, id: InstId) -> (f64, f64) {
    let m = &ctx.w.get(id).unwrap().m;
    (m.hspeed().get(), m.vspeed().get())
}

fn block_run(ctx: &mut Ctx, kid: InstId, other: InstId) {
    let run = KidCollisionRun::Block(BlockRun::BlockStandard);
    assert_eq!(
        kid::run_collision(ctx, kid, other, &run).expect("run"),
        Flow::Continue
    );
}

fn platform_run(ctx: &mut Ctx, kid: InstId, other: InstId) {
    let run = KidCollisionRun::Platform(PlatformRun::PlatformStandard);
    assert_eq!(
        kid::run_collision(ctx, kid, other, &run).expect("run"),
        Flow::Continue
    );
}

// ---- Block ----

#[test]
fn landing_on_the_floor_refreshes_the_counter_when_normal() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k| {
            kstate(ctx, kid).cur_jumps = 3;
            let b = spawn(ctx, k.block, 80.0, 110.0);
            set_v(ctx, kid, 0.0, 4.0);
            block_run(ctx, kid, b);
            assert_eq!(pos(ctx, kid), (100.0, 101.0));
            assert_eq!(speeds(ctx, kid), (0.0, 0.0));
            assert_eq!(kstate(ctx, kid).cur_jumps, 1);
        },
    );
}

#[test]
fn hitting_a_ceiling_does_not_refresh_when_normal() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k| {
            kstate(ctx, kid).cur_jumps = 3;
            // The box top is y - 12 = 88; the block's bottom row is 87 at y 56.
            let b = spawn(ctx, k.block, 80.0, 56.0);
            set_v(ctx, kid, 0.0, -4.0);
            block_run(ctx, kid, b);
            assert_eq!(speeds(ctx, kid), (0.0, 0.0));
            assert_eq!(kstate(ctx, kid).cur_jumps, 3);
        },
    );
}

#[test]
fn reversed_the_ceiling_is_the_floor() {
    with_kid(
        -1,
        |_| {},
        |ctx, kid, k| {
            kstate(ctx, kid).cur_jumps = 3;
            // The mirrored box spans y - 8 .. y + 12: top 92. A block ending at row 87 leaves 4 px.
            let b = spawn(ctx, k.block, 80.0, 56.0);
            set_v(ctx, kid, 0.0, -6.0);
            block_run(ctx, kid, b);
            assert_eq!(pos(ctx, kid), (100.0, 96.0), "flush against the ceiling blocks");
            assert_eq!(speeds(ctx, kid), (0.0, 0.0));
            assert_eq!(
                kstate(ctx, kid).cur_jumps,
                1,
                "landing on the ceiling side refreshes"
            );
            assert_eq!(ctx.w.get(kid).unwrap().m.gravity(), Real::ZERO);
        },
    );
}

#[test]
fn reversed_the_floor_does_not_refresh() {
    with_kid(
        -1,
        |_| {},
        |ctx, kid, k| {
            kstate(ctx, kid).cur_jumps = 3;
            // The mirrored box bottom is y + 12 = 112; the block's top row is 114.
            let b = spawn(ctx, k.block, 80.0, 114.0);
            set_v(ctx, kid, 0.0, 6.0);
            block_run(ctx, kid, b);
            assert_eq!(speeds(ctx, kid), (0.0, 0.0));
            assert_eq!(kstate(ctx, kid).cur_jumps, 3);
        },
    );
}

#[test]
fn walls_work_the_same_reversed() {
    with_kid(
        -1,
        |_| {},
        |ctx, kid, k| {
            let b = spawn(ctx, k.block, 107.0, 80.0);
            set_v(ctx, kid, 3.0, 0.0);
            block_run(ctx, kid, b);
            assert_eq!(pos(ctx, kid), (101.0, 100.0));
            assert_eq!(speeds(ctx, kid).0, 0.0);
        },
    );
}

#[test]
fn a_flipped_kid_of_another_model_is_still_unsupported() {
    with_kid(
        -1,
        |p| p.flip.model = FlipKind::None,
        |ctx, kid, k| {
            let b = spawn(ctx, k.block, 80.0, 110.0);
            let run = KidCollisionRun::Block(BlockRun::BlockStandard);
            assert!(kid::run_collision(ctx, kid, b, &run).is_err());
        },
    );
}

// ---- Platform ----

fn move_platform(ctx: &mut Ctx, p: InstId, from_y: f64, to_y: f64) {
    let m = &mut ctx.w.get_mut(p).unwrap().m;
    m.set_y(r(from_y)).unwrap();
    m.set_yprevious(r(from_y)).unwrap();
    m.set_y(r(to_y)).unwrap();
}

#[test]
fn landing_takes_the_platforms_delta_y() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k| {
            let p = spawn(ctx, k.platform, 80.0, 105.0);
            move_platform(ctx, p, 104.0, 105.0);
            set_v(ctx, kid, 0.0, 2.0);
            platform_run(ctx, kid, p);
            assert_eq!(pos(ctx, kid).1, 96.0, "platform.y - 9");
            assert_eq!(speeds(ctx, kid).1, 1.0, "y - yprevious, not vspeed");
            assert_eq!(kstate(ctx, kid).cur_jumps, 1);
            assert!(kstate(ctx, kid).on_platform);
        },
    );
}

#[test]
fn a_platform_with_negative_vspeed_does_not_snap() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k| {
            kstate(ctx, kid).cur_jumps = 3;
            let p = spawn(ctx, k.platform, 80.0, 105.0);
            set_v(ctx, p, 0.0, -1.0);
            set_v(ctx, kid, 0.0, 1.0);
            platform_run(ctx, kid, p);
            assert_eq!(pos(ctx, kid).1, 100.0);
            assert_eq!(speeds(ctx, kid).1, 1.0);
            assert!(kstate(ctx, kid).on_platform);
            assert_eq!(kstate(ctx, kid).cur_jumps, 1);
        },
    );
}

#[test]
fn reversed_the_platform_is_a_ceiling_the_kid_lands_under() {
    with_kid(
        -1,
        |_| {},
        |ctx, kid, k| {
            // y - vspeed / 2 = 100 + 3 = 103 >= 85 + 15 = 100: lands.
            let p = spawn(ctx, k.platform, 80.0, 85.0);
            move_platform(ctx, p, 85.0, 85.0);
            set_v(ctx, kid, 0.0, -6.0);
            platform_run(ctx, kid, p);
            assert_eq!(pos(ctx, kid).1, 109.0, "platform.y + 24");
            assert_eq!(speeds(ctx, kid).1, 0.0);
            assert!(kstate(ctx, kid).on_platform);
            assert_eq!(kstate(ctx, kid).cur_jumps, 1);
        },
    );
}

#[test]
fn reversed_from_the_wrong_side_passes_through() {
    with_kid(
        -1,
        |_| {},
        |ctx, kid, k| {
            // 100 + 3 = 103 < 90 + 15 = 105.
            let p = spawn(ctx, k.platform, 80.0, 90.0);
            set_v(ctx, kid, 0.0, -6.0);
            platform_run(ctx, kid, p);
            assert_eq!(pos(ctx, kid).1, 100.0);
            assert!(!kstate(ctx, kid).on_platform);
        },
    );
}

#[test]
fn the_yspeed_flipped_take_is_still_unsupported() {
    with_kid(
        1,
        |p| p.platform.take = PlatformTake::YspeedFlipped,
        |ctx, kid, k| {
            let p = spawn(ctx, k.platform, 80.0, 105.0);
            let run = KidCollisionRun::Platform(PlatformRun::PlatformStandard);
            assert!(kid::run_collision(ctx, kid, p, &run).is_err());
        },
    );
}

// ---- playerMisc: killer_check ----

#[test]
fn killer_check_kills_a_kid_touching_a_killer() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.killer, 90.0, 90.0);
            let flow = kid::run_action(ctx, kid, &KidAction::B(BAction::KillerCheck)).unwrap();
            assert_eq!(flow, Flow::Continue);
            assert!(!ctx.w.get(kid).unwrap().alive);
        },
    );
}

#[test]
fn killer_check_leaves_a_kid_alone_that_touches_nothing() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.killer, 300.0, 300.0);
            kid::run_action(ctx, kid, &KidAction::B(BAction::KillerCheck)).unwrap();
            assert!(ctx.w.get(kid).unwrap().alive);
        },
    );
}

#[test]
fn killer_check_uses_the_position_at_the_start_of_the_step() {
    // The killer is where the kid is going, not where it is: nothing yet.
    with_kid(
        1,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.killer, 90.0, 120.0);
            set_v(ctx, kid, 0.0, 20.0);
            kid::run_action(ctx, kid, &KidAction::B(BAction::KillerCheck)).unwrap();
            assert!(ctx.w.get(kid).unwrap().alive);
        },
    );
}

// ---- Outside Room ----

fn outside_room(ctx: &mut Ctx, kid: InstId) -> Flow {
    kid::on_event(ctx, kid, &EventKind::OutsideRoom).expect("event")
}

#[test]
fn outside_the_room_the_kid_dies() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, _| {
            ctx.w.get_mut(kid).unwrap().m.set_x(r(-40.0)).unwrap();
            outside_room(ctx, kid);
            assert!(!ctx.w.get(kid).unwrap().alive);
        },
    );
}

#[test]
fn outside_the_room_on_a_room_changer_the_kid_lives() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k| {
            ctx.w.get_mut(kid).unwrap().m.set_x(r(-40.0)).unwrap();
            spawn(ctx, k.warp, -64.0, 80.0);
            assert_eq!(outside_room(ctx, kid), Flow::Continue);
            assert!(ctx.w.get(kid).unwrap().alive);
        },
    );
}

#[test]
fn the_event_is_wired_only_for_this_model() {
    with_kid(
        1,
        |p| p.events.boundary = BoundaryModel::None,
        |ctx, kid, _| {
            ctx.w.get_mut(kid).unwrap().m.set_x(r(-40.0)).unwrap();
            outside_room(ctx, kid);
            assert!(ctx.w.get(kid).unwrap().alive);
        },
    );
}
