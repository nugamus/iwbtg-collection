//! WP5a.3: the A1 kid's block and platform collision runs on hand-built worlds
//! (docs/plans/sim-core.md 5.3, kid/A1.md "Collision with block", "Collision with platform").
//!
//! The pipeline's solid put-back is not part of these tests: each starts with the kid at the
//! position the put-back leaves it in (the previous position) and the speeds it had, then runs the
//! kid's collision run and reads the result.

use iwbtg_sim::def::physics::KidCollision;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, PhysicsProfile, RoomDef};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::kid::collisions::block::BlockRun;
use iwbtg_sim::kid::collisions::platform::PlatformRun;
use iwbtg_sim::kid::{self, KidCollisionRun};
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
}

fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("wp5a-collisions");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let mut kid = KindDef::new("kid", Builtin::Kid);
    kid.sprite = Some(kid_sprite);
    kid.classes = ClassMask::KID;
    let kid = b.kind(kid);
    let mut block = KindDef::new("block", Builtin::Inert);
    block.sprite = Some(tile);
    block.classes = ClassMask::BLOCK;
    block.solid = true;
    let block = b.kind(block);
    let mut platform = KindDef::new("platform", Builtin::Inert);
    platform.sprite = Some(tile);
    platform.classes = ClassMask::PLATFORM | ClassMask::BLOCK;
    let platform = b.kind(platform);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = PhysicsProfile::default();
    p.gravity.block_landing_zeroes = true;
    p.platform.snap = r(9.0);
    p.events.collisions = vec![
        KidCollision {
            with: ClassMask::BLOCK,
            run: KidCollisionRun::Block(BlockRun::BlockStandard),
        },
        KidCollision {
            with: ClassMask::PLATFORM,
            run: KidCollisionRun::Platform(PlatformRun::PlatformStandard),
        },
    ];
    tweak(&mut p);
    b.physics(p);
    (b.build().expect("game builds"), Kinds { kid, block, platform })
}

/// Runs `f` with a kid at (100, 100), gravity 0.4, `djump` false.
fn with_kid<T>(tweak: impl FnOnce(&mut PhysicsProfile), f: impl FnOnce(&mut Ctx, InstId, &Kinds) -> T) -> T {
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
    ctx.w.get_mut(kid).unwrap().m.set_gravity(r(0.4)).unwrap();
    f(&mut ctx, kid, &k)
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

fn gravity(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.gravity().get()
}

fn djump(ctx: &Ctx, id: InstId) -> bool {
    match &ctx.w.get(id).unwrap().state {
        KindState::Kid(s) => s.djump,
        _ => unreachable!(),
    }
}

fn on_platform(ctx: &Ctx, id: InstId) -> bool {
    match &ctx.w.get(id).unwrap().state {
        KindState::Kid(s) => s.on_platform,
        _ => unreachable!(),
    }
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

// ---- block_standard ----

#[test]
fn wall_on_the_right_stops_the_kid_flush() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            // Box right edge is x+5 = 105; the block starts at 107. A 3 px move would overlap.
            let b = spawn(ctx, k.block, 107.0, 80.0);
            set_v(ctx, kid, 3.0, 0.0);
            block_run(ctx, kid, b);
            assert_eq!(
                pos(ctx, kid),
                (101.0, 100.0),
                "moves until one more step would hit"
            );
            assert_eq!(speeds(ctx, kid), (0.0, 0.0));
        },
    );
}

#[test]
fn wall_on_the_left_stops_the_kid_flush() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            // Box left edge is x-5 = 95; the block covers up to 93 (x 62..93).
            let b = spawn(ctx, k.block, 62.0, 80.0);
            set_v(ctx, kid, -3.0, 0.0);
            block_run(ctx, kid, b);
            assert_eq!(pos(ctx, kid), (99.0, 100.0));
            assert_eq!(speeds(ctx, kid).0, 0.0);
        },
    );
}

#[test]
fn a_free_side_is_left_alone() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let b = spawn(ctx, k.block, 200.0, 80.0);
            set_v(ctx, kid, 3.0, 0.0);
            block_run(ctx, kid, b);
            assert_eq!(pos(ctx, kid), (100.0, 100.0));
            assert_eq!(speeds(ctx, kid), (3.0, 0.0), "no contact: hspeed kept");
        },
    );
}

#[test]
fn landing_rests_on_the_block_zeroes_gravity_and_refreshes_the_double_jump() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            // Box bottom is y+8 = 108; block top row 110. A 4 px fall would overlap.
            let b = spawn(ctx, k.block, 80.0, 110.0);
            set_v(ctx, kid, 0.0, 4.0);
            block_run(ctx, kid, b);
            assert_eq!(
                pos(ctx, kid),
                (100.0, 101.0),
                "bottom row 109, just above the block"
            );
            assert_eq!(speeds(ctx, kid), (0.0, 0.0));
            assert_eq!(gravity(ctx, kid), 0.0);
            assert!(djump(ctx, kid), "landing refreshes djump");
        },
    );
}

#[test]
fn landing_keeps_gravity_when_the_game_does_not_zero_it() {
    with_kid(
        |p| p.gravity.block_landing_zeroes = false,
        |ctx, kid, k| {
            let b = spawn(ctx, k.block, 80.0, 110.0);
            set_v(ctx, kid, 0.0, 4.0);
            block_run(ctx, kid, b);
            assert_eq!(gravity(ctx, kid), 0.4);
            assert_eq!(speeds(ctx, kid).1, 0.0);
        },
    );
}

#[test]
fn a_ceiling_stops_the_kid_without_refreshing_djump() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            // Block rows 50..81; the box top is y-12 = 88. Moving up 8 would overlap.
            let b = spawn(ctx, k.block, 80.0, 50.0);
            set_v(ctx, kid, 0.0, -8.0);
            block_run(ctx, kid, b);
            assert_eq!(pos(ctx, kid), (100.0, 94.0), "top row 82, just below the block");
            assert_eq!(speeds(ctx, kid), (0.0, 0.0));
            assert_eq!(gravity(ctx, kid), 0.0);
            assert!(!djump(ctx, kid));
        },
    );
}

#[test]
fn a_corner_only_hit_zeroes_hspeed_and_keeps_vspeed() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            // Neither the sideways nor the vertical shift hits; the diagonal (103, 103) does.
            let b = spawn(ctx, k.block, 108.0, 110.0);
            set_v(ctx, kid, 3.0, 3.0);
            block_run(ctx, kid, b);
            assert_eq!(pos(ctx, kid), (100.0, 100.0), "no contact move");
            assert_eq!(speeds(ctx, kid), (0.0, 3.0));
            assert_eq!(gravity(ctx, kid), 0.4);
        },
    );
}

#[test]
fn a_wall_and_a_floor_in_one_hit_both_apply() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let wall = spawn(ctx, k.block, 107.0, 80.0);
            spawn(ctx, k.block, 80.0, 110.0);
            set_v(ctx, kid, 3.0, 4.0);
            block_run(ctx, kid, wall);
            assert_eq!(pos(ctx, kid), (101.0, 101.0));
            assert_eq!(speeds(ctx, kid), (0.0, 0.0));
        },
    );
}

// ---- platform_standard ----

#[test]
fn landing_on_a_platform_snaps_nine_above_and_takes_its_speed() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let p = spawn(ctx, k.platform, 80.0, 105.0);
            set_v(ctx, kid, 0.0, 2.0);
            platform_run(ctx, kid, p);
            assert_eq!(pos(ctx, kid).1, 96.0);
            assert_eq!(speeds(ctx, kid).1, 0.0, "the platform's vspeed");
            assert!(on_platform(ctx, kid) && djump(ctx, kid));
        },
    );
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let p = spawn(ctx, k.platform, 80.0, 105.0);
            set_v(ctx, p, 0.0, 2.0);
            set_v(ctx, kid, 0.0, 1.0);
            platform_run(ctx, kid, p);
            assert_eq!(pos(ctx, kid).1, 96.0);
            assert_eq!(speeds(ctx, kid).1, 2.0, "moving down: the kid rides it");
        },
    );
}

#[test]
fn a_platform_moving_up_flags_the_kid_without_snapping() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let p = spawn(ctx, k.platform, 80.0, 105.0);
            set_v(ctx, p, 0.0, -1.0);
            set_v(ctx, kid, 0.0, 1.0);
            platform_run(ctx, kid, p);
            assert_eq!(pos(ctx, kid).1, 100.0, "no snap");
            assert_eq!(speeds(ctx, kid).1, 1.0, "no speed copy");
            assert!(on_platform(ctx, kid) && djump(ctx, kid));
        },
    );
}

#[test]
fn a_platform_is_one_way_from_below() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            // y - vspeed/2 = 100 + 1.5 = 101.5, below the platform's y of 90.
            let p = spawn(ctx, k.platform, 80.0, 90.0);
            set_v(ctx, kid, 0.0, -3.0);
            platform_run(ctx, kid, p);
            assert_eq!(pos(ctx, kid).1, 100.0);
            assert_eq!(speeds(ctx, kid).1, -3.0);
            assert!(!on_platform(ctx, kid) && !djump(ctx, kid));
        },
    );
}

#[test]
fn the_half_vspeed_boundary_is_inclusive() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            // y - vspeed/2 = 100 - 1 = 99 = platform.y: lands.
            let p = spawn(ctx, k.platform, 80.0, 99.0);
            set_v(ctx, kid, 0.0, 2.0);
            platform_run(ctx, kid, p);
            assert!(on_platform(ctx, kid));
            assert_eq!(pos(ctx, kid).1, 90.0);
        },
    );
}

#[test]
fn collision_events_run_the_rows_for_their_handler_class() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let b = spawn(ctx, k.block, 80.0, 110.0);
            set_v(ctx, kid, 0.0, 4.0);
            let ev = EventKind::Collision {
                with: ClassMask::BLOCK,
                other: b,
            };
            assert_eq!(kid::on_event(ctx, kid, &ev).expect("event"), Flow::Continue);
            assert_eq!(pos(ctx, kid).1, 101.0);
            // A class with no row does nothing.
            let ev = EventKind::Collision {
                with: ClassMask::KILLER,
                other: b,
            };
            set_v(ctx, kid, 0.0, 4.0);
            kid::on_event(ctx, kid, &ev).expect("event");
            assert_eq!(speeds(ctx, kid).1, 4.0);
        },
    );
}

#[test]
fn a_platform_runs_both_its_handler_rows_in_row_order() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let p = spawn(ctx, k.platform, 80.0, 105.0);
            set_v(ctx, kid, 0.0, 2.0);
            for with in [ClassMask::BLOCK, ClassMask::PLATFORM] {
                let ev = EventKind::Collision { with, other: p };
                kid::on_event(ctx, kid, &ev).expect("event");
            }
            // The block row finds no solid; the platform row lands the kid.
            assert_eq!(pos(ctx, kid).1, 96.0);
            assert!(on_platform(ctx, kid));
        },
    );
}

#[test]
fn event_set_lists_the_handler_classes_once_in_order() {
    let (def, _) = game(|_| {});
    let set = kid::event_set(&def.physics);
    assert_eq!(
        set.collisions.as_slice(),
        &[ClassMask::BLOCK, ClassMask::PLATFORM]
    );
}
