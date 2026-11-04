//! WP5b.4 (#262): family B's End Step `slope_b` (playerSlope) and the air sprite's wall probe on
//! hand-built worlds (kid/B.md "End Step (player): slopes and gravity", playerMove 6).
//!
//! Kid mask 11x21 with origin (5, 12): the box is x-5..x+5 and y-12..y+8 (mirrored when reversed:
//! y-8..y+12). Walls are 32x32 `slope_wall` solids.

use iwbtg_sim::def::physics::{FlipKind, GravityReapply};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, PhysicsProfile, RoomDef, SpriteId};
use iwbtg_sim::events::EventKind;
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
    wall: KindId,
    block: KindId,
}

struct Sprites {
    jump: SpriteId,
    fall: SpriteId,
}

fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds, Sprites) {
    let mut b = GameDefBuilder::new("wp5b-slope");
    let tile = b.sprite_rect((32, 32), (0, 0));
    let sprites = Sprites {
        jump: b.sprite_rect((11, 21), (5, 12)),
        fall: b.sprite_rect((11, 21), (5, 12)),
    };
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(kid_sprite);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let solid = |name: &str, classes: ClassMask, b: &mut GameDefBuilder| {
        let mut k = KindDef::new(name, Builtin::Inert);
        k.sprite = Some(tile);
        k.classes = classes;
        k.solid = true;
        b.kind(k)
    };
    let wall = solid("wall", ClassMask::SLOPE_WALL, &mut b);
    let block = solid("block", ClassMask::BLOCK, &mut b);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = PhysicsProfile::default();
    p.numbers.gravity = r(0.4);
    p.numbers.sprite_threshold = r(0.05);
    p.numbers.air_wall_probe = r(2.0);
    p.gravity.reapply = GravityReapply::EndStepSlope;
    p.flip.model = FlipKind::Yflag;
    p.sprites.jump = Some(sprites.jump);
    p.sprites.fall = Some(sprites.fall);
    p.events.end_step = vec![KidAction::B(BAction::SlopeB)];
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds { kid, wall, block },
        sprites,
    )
}

/// A kid at (100, 100) with `yflag` and gravity 0 (as after Create).
fn with_kid<T>(
    yflag: i8,
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
    ctx.w
        .get_mut(kid)
        .unwrap()
        .img
        .set_yscale(r(f64::from(yflag)))
        .unwrap();
    kstate(&mut ctx, kid).yflag = yflag;
    f(&mut ctx, kid, &k, &s)
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

/// Puts the kid at `(x, y)` having come from `(px, py)` with `vspeed`.
fn place(ctx: &mut Ctx, kid: InstId, (x, y): (f64, f64), (px, py): (f64, f64), vs: f64) {
    let m = &mut ctx.w.get_mut(kid).unwrap().m;
    m.set_x(r(px)).unwrap();
    m.set_y(r(py)).unwrap();
    m.set_xprevious(r(px)).unwrap();
    m.set_yprevious(r(py)).unwrap();
    m.set_x(r(x)).unwrap();
    m.set_y(r(y)).unwrap();
    m.set_vspeed(r(vs)).unwrap();
}

fn pos(ctx: &Ctx, id: InstId) -> (f64, f64) {
    let m = &ctx.w.get(id).unwrap().m;
    (m.x().get(), m.y().get())
}

fn gravity(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.gravity().get()
}

fn vspeed(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.vspeed().get()
}

fn end_step(ctx: &mut Ctx, kid: InstId) {
    kid::on_event(ctx, kid, &EventKind::EndStep).expect("end step");
}

// ---- The gravity restore, with no walls at all ----

#[test]
fn without_walls_the_end_step_turns_gravity_on_and_keeps_y_and_vspeed() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, _, _| {
            place(ctx, kid, (100.0, 100.0), (100.0, 96.0), 4.0);
            end_step(ctx, kid);
            assert_eq!(gravity(ctx, kid), 0.4);
            assert_eq!((pos(ctx, kid), vspeed(ctx, kid)), ((100.0, 100.0), 4.0));
        },
    );
}

#[test]
fn reversed_the_restored_gravity_is_negative() {
    with_kid(
        -1,
        |_| {},
        |ctx, kid, _, _| {
            place(ctx, kid, (100.0, 100.0), (100.0, 104.0), -4.0);
            end_step(ctx, kid);
            assert_eq!(gravity(ctx, kid), -0.4);
            assert_eq!(vspeed(ctx, kid), -4.0);
        },
    );
}

#[test]
fn the_restore_is_the_end_step_not_the_create() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, _, _| {
            iwbtg_sim::kid::run_action(ctx, kid, &KidAction::Common(CommonAction::CreateDefaults)).unwrap();
            assert_eq!(gravity(ctx, kid), 0.0, "Create leaves gravity at 0");
            end_step(ctx, kid);
            assert_eq!(gravity(ctx, kid), 0.4);
        },
    );
}

#[test]
fn a_kid_overlapping_a_plain_block_is_pushed_back_and_moved_toward_wy() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k, _| {
            // Only `obj_wall`s count for the slope code; a block is just a solid: the kid is at (100,
            // 106) inside it, was at (100, 100). Step 4.2: y = wy fails place_free, so y = yprevious
            // and contact toward wy (down) with no limit.
            spawn(ctx, k.block, 80.0, 112.0);
            place(ctx, kid, (100.0, 106.0), (100.0, 100.0), 6.0);
            end_step(ctx, kid);
            assert_eq!(
                pos(ctx, kid),
                (100.0, 103.0),
                "flush above the block (bottom row 111)"
            );
            assert_eq!(vspeed(ctx, kid), 6.0, "wSpeed comes back");
            assert_eq!(gravity(ctx, kid), 0.4);
        },
    );
}

// ---- Standing on a wall ----

#[test]
fn standing_on_a_wall_leaves_gravity_alone_and_refreshes_the_counter() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.wall, 80.0, 109.0);
            kstate(ctx, kid).cur_jumps = 3;
            place(ctx, kid, (100.0, 100.0), (100.0, 100.0), 0.0);
            end_step(ctx, kid);
            assert_eq!(
                gravity(ctx, kid),
                0.0,
                "a wall is 1 px below: no gravity restore (step 5 skips it)"
            );
            assert_eq!(kstate(ctx, kid).cur_jumps, 1);
        },
    );
}

#[test]
fn a_wall_below_with_vspeed_does_not_refresh() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.wall, 80.0, 109.0);
            kstate(ctx, kid).cur_jumps = 3;
            place(ctx, kid, (100.0, 100.0), (100.0, 100.0), -3.0);
            end_step(ctx, kid);
            assert_eq!(kstate(ctx, kid).cur_jumps, 3);
        },
    );
}

#[test]
fn reversed_standing_under_a_ceiling_wall_refreshes() {
    with_kid(
        -1,
        |_| {},
        |ctx, kid, k, _| {
            // The mirrored box top is y - 8 = 92; the wall's bottom row is 91 at y 60.
            spawn(ctx, k.wall, 80.0, 60.0);
            kstate(ctx, kid).cur_jumps = 3;
            place(ctx, kid, (100.0, 100.0), (100.0, 100.0), 0.0);
            end_step(ctx, kid);
            assert_eq!(kstate(ctx, kid).cur_jumps, 1);
            assert_eq!(gravity(ctx, kid), 0.0);
        },
    );
}

// ---- Inside a wall ----

#[test]
fn entering_a_wall_goes_back_and_stops_flush() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.wall, 80.0, 110.0);
            place(ctx, kid, (100.0, 104.0), (100.0, 100.0), 4.0);
            end_step(ctx, kid);
            assert_eq!(
                pos(ctx, kid),
                (100.0, 101.0),
                "bottom row 109, just above the wall"
            );
            assert_eq!((vspeed(ctx, kid), gravity(ctx, kid)), (0.0, 0.0));
            assert_eq!(kstate(ctx, kid).cur_jumps, 1);
        },
    );
}

#[test]
fn walking_up_a_step_climbs_it() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.wall, 80.0, 109.0); // the floor, 1 px under the kid
            spawn(ctx, k.wall, 106.0, 106.0); // a 3 px step starting at x 106
            // The kid walks one px right into the step.
            place(ctx, kid, (101.0, 100.0), (100.0, 100.0), 0.0);
            end_step(ctx, kid);
            assert_eq!(
                pos(ctx, kid),
                (101.0, 97.0),
                "x = wx, 3 up, then down onto the step's top"
            );
            assert_eq!((vspeed(ctx, kid), gravity(ctx, kid)), (0.0, 0.0));
        },
    );
}

#[test]
fn walking_into_a_wall_with_no_floor_just_stops() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.wall, 106.0, 106.0);
            place(ctx, kid, (101.0, 100.0), (100.0, 100.0), 0.0);
            end_step(ctx, kid);
            assert_eq!(
                pos(ctx, kid).0,
                100.0,
                "back to the previous x, contact toward the wall"
            );
            assert_eq!(gravity(ctx, kid), 0.4, "no wall below: gravity restored");
        },
    );
}

// ---- Walking down ----

#[test]
fn walking_down_a_slope_follows_the_wall_down() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k, _| {
            // A wall 2 px under the kid's feet, reached by the probe 3 px down and 1 px ahead.
            spawn(ctx, k.wall, 80.0, 111.0);
            place(ctx, kid, (100.0, 100.0), (100.0, 100.0), 0.0);
            end_step(ctx, kid);
            assert_eq!(
                pos(ctx, kid),
                (100.0, 102.0),
                "contact in the gravity direction, at most 10"
            );
            assert_eq!(kstate(ctx, kid).cur_jumps, 1, "now standing on it with vspeed 0");
        },
    );
}

// ---- The air sprite's wall probe ----

#[test]
fn the_fall_sprite_needs_no_wall_two_pixels_below() {
    with_kid(
        1,
        |_| {},
        |ctx, kid, k, s| {
            place(ctx, kid, (100.0, 100.0), (100.0, 100.0), 1.0);
            iwbtg_sim::kid::run_action(ctx, kid, &KidAction::Common(CommonAction::AirSprite)).unwrap();
            assert_eq!(ctx.w.get(kid).unwrap().img.sprite(), Some(s.fall));
            // A wall 2 px below keeps the sprite as it was.
            ctx.w.get_mut(kid).unwrap().img.set_sprite(Some(s.jump));
            spawn(ctx, k.wall, 80.0, 110.0);
            iwbtg_sim::kid::run_action(ctx, kid, &KidAction::Common(CommonAction::AirSprite)).unwrap();
            assert_eq!(ctx.w.get(kid).unwrap().img.sprite(), Some(s.jump));
        },
    );
}

#[test]
fn reversed_the_wall_probe_looks_up() {
    with_kid(
        -1,
        |_| {},
        |ctx, kid, k, s| {
            place(ctx, kid, (100.0, 100.0), (100.0, 100.0), -1.0);
            spawn(ctx, k.wall, 80.0, 60.0); // above: the mirrored box top is 92, 2 px up is row 90
            ctx.w.get_mut(kid).unwrap().img.set_sprite(Some(s.jump));
            iwbtg_sim::kid::run_action(ctx, kid, &KidAction::Common(CommonAction::AirSprite)).unwrap();
            assert_eq!(
                ctx.w.get(kid).unwrap().img.sprite(),
                Some(s.jump),
                "no fall sprite near a wall"
            );
            // Without the probe the fall sprite shows.
        },
    );
    with_kid(
        -1,
        |p| p.numbers.air_wall_probe = Real::ZERO,
        |ctx, kid, k, s| {
            place(ctx, kid, (100.0, 100.0), (100.0, 100.0), -1.0);
            spawn(ctx, k.wall, 80.0, 60.0);
            iwbtg_sim::kid::run_action(ctx, kid, &KidAction::Common(CommonAction::AirSprite)).unwrap();
            assert_eq!(ctx.w.get(kid).unwrap().img.sprite(), Some(s.fall));
        },
    );
}
