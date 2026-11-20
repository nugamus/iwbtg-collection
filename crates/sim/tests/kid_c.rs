//! WP5c.3 (#130): family C (keep-celebrating-for-xia) on hand-built worlds (kid/C.md): the `grav`
//! sign flip, symmetric early vines, water 3, the flipped platform rule, the End Step edge and
//! killer tests, the suicide press.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef, SpriteId,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::c::CAction;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::actions::flip::FlipAction;
use iwbtg_sim::kid::collisions::block::BlockRun;
use iwbtg_sim::kid::collisions::platform::PlatformRun;
use iwbtg_sim::kid::{self, KidAction, KidCollisionRun, KidState, presets};
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
    block: KindId,
    platform: KindId,
    vine_l: KindId,
    vine_r: KindId,
    water2: KindId,
    water3: KindId,
    killer: KindId,
}

struct Sprites {
    jump: SpriteId,
    fall: SpriteId,
    sliding: SpriteId,
}

fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds, Sprites) {
    let mut b = GameDefBuilder::new("wp5c-c");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let plat = b.sprite_rect((32, 8), (0, 0));
    let sprites = Sprites {
        jump: b.sprite_rect((11, 21), (5, 12)),
        fall: b.sprite_rect((11, 21), (5, 12)),
        sliding: b.sprite_rect((11, 21), (5, 12)),
    };
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(kid_sprite);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let inert = |name: &str, classes: ClassMask, solid: bool, sprite: SpriteId, b: &mut GameDefBuilder| {
        let mut k = KindDef::new(name, Builtin::Inert);
        k.sprite = Some(sprite);
        k.classes = classes;
        k.solid = solid;
        b.kind(k)
    };
    let block = inert("block", ClassMask::BLOCK, true, tile, &mut b);
    let platform = inert(
        "platform",
        ClassMask::PLATFORM | ClassMask::BLOCK,
        false,
        plat,
        &mut b,
    );
    let vine_l = inert("vine_l", ClassMask::VINE_L, false, tile, &mut b);
    let vine_r = inert("vine_r", ClassMask::VINE_R, false, tile, &mut b);
    let water2 = inert("water2", ClassMask::WATER2, false, tile, &mut b);
    let water3 = inert(
        "water3",
        ClassMask::WATER2 | ClassMask::WATER3,
        false,
        tile,
        &mut b,
    );
    let killer = inert("killer", ClassMask::KILLER, false, tile, &mut b);
    let mut bow = KindDef::new("bow", Builtin::Inert);
    bow.classes = ClassMask::BOW;
    let bow = b.kind(bow);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::Bow, bow);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = presets::defaults(KidFamily::C);
    p.sprites.jump = Some(sprites.jump);
    p.sprites.fall = Some(sprites.fall);
    p.sprites.sliding = Some(sprites.sliding);
    p.death.order = vec![iwbtg_sim::def::physics::DeathStep::DestroyKid];
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds {
            kid,
            block,
            platform,
            vine_l,
            vine_r,
            water2,
            water3,
            killer,
        },
        sprites,
    )
}

/// A kid at (100, 100) after `create_defaults`, `flipped` as the global `grav` says.
fn with_kid<T>(
    flipped: bool,
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds, &Sprites) -> T,
) -> T {
    let (def, k, s) = game(tweak);
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let mut w = World::new(def.start_room, 1);
    w.room_speed = r(50.0);
    w.engine.kid.grav = flipped;
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
    common(&mut ctx, kid, CommonAction::CreateDefaults);
    f(&mut ctx, kid, &k, &s)
}

fn common(ctx: &mut Ctx, kid: InstId, a: CommonAction) {
    kid::run_action(ctx, kid, &KidAction::Common(a)).expect("action");
}

fn c(ctx: &mut Ctx, kid: InstId, a: CAction) {
    kid::run_action(ctx, kid, &KidAction::C(a)).expect("action");
}

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

fn y(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.y().get()
}

// ---- Create and the flip ----

#[test]
fn create_signs_gravity_and_the_scale_by_grav() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, _| {
            assert_eq!(ctx.w.get(kid).unwrap().m.gravity(), r(0.4));
            assert_eq!(ctx.w.get(kid).unwrap().img.yscale(), Real::ONE);
            assert_eq!(kstate(ctx, kid).yflag, 1);
        },
    );
    with_kid(
        true,
        |_| {},
        |ctx, kid, _, _| {
            assert_eq!(ctx.w.get(kid).unwrap().m.gravity(), r(-0.4));
            assert_eq!(ctx.w.get(kid).unwrap().img.yscale(), r(-1.0));
            assert_eq!(kstate(ctx, kid).yflag, -1);
            assert_eq!(y(ctx, kid), 100.0, "no shift at Create");
        },
    );
}

#[test]
fn flip_grav_negates_everything_and_shifts_four() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, _| {
            set_v(ctx, kid, 1.0, 5.0);
            kstate(ctx, kid).djump = false;
            kid::run_action(ctx, kid, &KidAction::Flip(FlipAction::FlipGrav)).unwrap();
            assert!(ctx.w.engine.kid.grav);
            assert_eq!(hv(ctx, kid), (1.0, 0.0));
            assert_eq!(ctx.w.get(kid).unwrap().m.gravity(), r(-0.4));
            assert_eq!(ctx.w.get(kid).unwrap().img.yscale(), r(-1.0));
            assert_eq!(y(ctx, kid), 96.0, "4 * the new grav");
            assert!(kstate(ctx, kid).djump);
            kid::api::flip_grav(ctx, kid).unwrap();
            assert!(!ctx.w.engine.kid.grav);
            assert_eq!(y(ctx, kid), 100.0);
        },
    );
}

#[test]
fn flipped_walking_does_not_swap_the_keys() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, _, _| {
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(hv(ctx, kid).0, 3.0, "unlike family B");
        },
    );
}

// ---- Block and platform contact, flipped ----

#[test]
fn flipped_block_contact_reads_hspeed_zero_as_right() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, k, _| {
            // A wall 2 px to the right; the kid stands still (hspeed 0) but the diagonal case.
            let b = spawn(ctx, k.block, 106.0, 90.0);
            set_v(ctx, kid, 0.0, 0.0);
            let run = KidCollisionRun::Block(BlockRun::BlockStandard);
            kid::run_collision(ctx, kid, b, &run).unwrap();
            assert_eq!(hv(ctx, kid).0, 0.0);
        },
    );
    with_kid(
        true,
        |_| {},
        |ctx, kid, k, _| {
            // Moving left into a wall at the left edge: moves left (not right) to contact.
            let b = spawn(ctx, k.block, 62.0, 80.0);
            set_v(ctx, kid, -3.0, 0.0);
            let run = KidCollisionRun::Block(BlockRun::BlockStandard);
            kid::run_collision(ctx, kid, b, &run).unwrap();
            assert_eq!(ctx.w.get(kid).unwrap().m.x().get(), 99.0);
        },
    );
}

#[test]
fn flipped_ceiling_contact_refreshes_djump() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, k, _| {
            kstate(ctx, kid).djump = false;
            let b = spawn(ctx, k.block, 80.0, 56.0);
            set_v(ctx, kid, 0.0, -6.0);
            let run = KidCollisionRun::Block(BlockRun::BlockStandard);
            kid::run_collision(ctx, kid, b, &run).unwrap();
            assert!(kstate(ctx, kid).djump);
            assert_eq!(hv(ctx, kid).1, 0.0);
        },
    );
}

#[test]
fn the_flipped_platform_rule_uses_the_sprite_height_and_the_yspeed() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, k, _| {
            // Platform height 8 at y 85: lands when y - vspeed/2 >= 85 + 8 - 1 = 92 (103 here).
            let p = spawn(ctx, k.platform, 80.0, 85.0);
            set_v(ctx, kid, 0.0, -6.0);
            let run = KidCollisionRun::Platform(PlatformRun::PlatformStandard);
            kid::run_collision(ctx, kid, p, &run).unwrap();
            assert_eq!(y(ctx, kid), 101.0, "platform.y + height + 8");
            assert_eq!(hv(ctx, kid).1, 0.0, "the platform's yspeed");
            assert!(kstate(ctx, kid).on_platform && kstate(ctx, kid).djump);
        },
    );
    with_kid(
        true,
        |_| {},
        |ctx, kid, k, _| {
            let p = spawn(ctx, k.platform, 80.0, 200.0);
            set_v(ctx, kid, 0.0, -6.0);
            let run = KidCollisionRun::Platform(PlatformRun::PlatformStandard);
            kid::run_collision(ctx, kid, p, &run).unwrap();
            assert!(
                !kstate(ctx, kid).on_platform,
                "103 < 200 + 7: from the wrong side"
            );
        },
    );
    with_kid(
        false,
        |_| {},
        |ctx, kid, k, _| {
            let p = spawn(ctx, k.platform, 80.0, 105.0);
            set_v(ctx, kid, 0.0, 2.0);
            let run = KidCollisionRun::Platform(PlatformRun::PlatformStandard);
            kid::run_collision(ctx, kid, p, &run).unwrap();
            assert_eq!(y(ctx, kid), 96.0, "normal gravity: the A1 rule");
        },
    );
}

// ---- Vines ----

#[test]
fn vine_probe_sets_the_flags_within_one_pixel_and_needs_free_space_below() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, k, _| {
            c(ctx, kid, CAction::VineProbe);
            assert!(!kstate(ctx, kid).ext.on_vine_l && !kstate(ctx, kid).ext.on_vine_r);
            spawn(ctx, k.vine_l, 64.0, 96.0); // flush against the kid's left edge: distance 1
            spawn(ctx, k.vine_r, 108.0, 96.0); // distance 3
            c(ctx, kid, CAction::VineProbe);
            assert!(kstate(ctx, kid).ext.on_vine_l, "<= 1 grabs, unlike A1's below 2");
            assert!(!kstate(ctx, kid).ext.on_vine_r);
            spawn(ctx, k.block, 80.0, 109.0);
            c(ctx, kid, CAction::VineProbe);
            assert!(!kstate(ctx, kid).ext.on_vine_l, "a block 1 px below: not free");
        },
    );
}

#[test]
fn a_left_vine_slides_kicks_and_lets_go() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, k, s| {
            spawn(ctx, k.vine_l, 64.0, 96.0);
            c(ctx, kid, CAction::VineProbe);
            input(ctx, &[Action::Jump], &[], &[]);
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid).1, 2.0);
            assert_eq!(ctx.w.get(kid).unwrap().img.sprite(), Some(s.sliding));
            input(ctx, &[Action::Jump, Action::Right], &[Action::Right], &[]);
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid), (15.0, -9.0));
            assert_eq!(ctx.w.get(kid).unwrap().img.sprite(), Some(s.jump));
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid), (3.0, 2.0));
            assert_eq!(ctx.w.get(kid).unwrap().img.sprite(), Some(s.fall));
        },
    );
}

#[test]
fn the_right_vine_wins_and_the_flipped_kick_goes_the_other_way_up() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.vine_l, 64.0, 96.0);
            spawn(ctx, k.vine_r, 106.0, 96.0);
            c(ctx, kid, CAction::VineProbe);
            assert!(kstate(ctx, kid).ext.on_vine_l && kstate(ctx, kid).ext.on_vine_r);
            // Pressing either direction with Jump held kicks with hspeed -15 (the right vine wins).
            input(ctx, &[Action::Jump, Action::Right], &[Action::Right], &[]);
            common(ctx, kid, CommonAction::Vines);
            // Flipped: vspeed -9 * grav = +9; Right on the left vine only counts when it is on_l.
            assert_eq!(hv(ctx, kid).1, 9.0);
            assert_eq!(kstate(ctx, kid).facing, -1);
            assert_eq!(ctx.w.get(kid).unwrap().img.xscale(), r(-1.0));
        },
    );
}

#[test]
fn a_vine_within_one_pixel_blocks_both_facing_and_the_blocked_direction() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, k, _| {
            // A right vine flush (distance 1): A1 also blocks it; the left one at exactly 1 now blocks
            // walking right and facing too.
            spawn(ctx, k.vine_l, 64.0, 96.0);
            set_v(ctx, kid, -3.0, 0.0);
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(hv(ctx, kid).0, -3.0, "hspeed keeps its old value");
            assert_eq!(kstate(ctx, kid).facing, 1, "facing unchanged near a vine");
            input(ctx, &[Action::Left], &[Action::Left], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(hv(ctx, kid).0, -3.0, "facing does not change either (still 1)");
            assert_eq!(kstate(ctx, kid).facing, 1);
        },
    );
}

// ---- Water 3 ----

#[test]
fn water_two_gives_the_air_jump_and_water_three_keeps_djump() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.water2, 80.0, 109.0);
            kstate(ctx, kid).djump = false;
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            common(ctx, kid, CommonAction::Jump);
            assert_eq!(hv(ctx, kid).1, -7.0, "djump false but water 2 below");
            assert!(!kstate(ctx, kid).djump, "spent");
        },
    );
    with_kid(
        false,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.water3, 80.0, 109.0);
            kstate(ctx, kid).djump = false;
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            common(ctx, kid, CommonAction::Jump);
            assert_eq!(hv(ctx, kid).1, -7.0);
            assert!(kstate(ctx, kid).djump, "water 3 keeps it");
        },
    );
}

// ---- End Step and the Step ----

fn end_step(ctx: &mut Ctx, kid: InstId) {
    kid::on_event(ctx, kid, &EventKind::EndStep).expect("end step");
}

#[test]
fn the_end_step_kills_at_the_edge_and_on_a_killer() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, _| {
            end_step(ctx, kid);
            assert!(ctx.w.get(kid).unwrap().alive);
            ctx.w.get_mut(kid).unwrap().m.set_x(r(-1.0)).unwrap();
            end_step(ctx, kid);
            assert!(!ctx.w.get(kid).unwrap().alive, "the origin is outside: edgeDeath");
        },
    );
    with_kid(
        false,
        |_| {},
        |ctx, kid, k, _| {
            ctx.w.get_mut(kid).unwrap().m.set_x(r(800.0)).unwrap();
            end_step(ctx, kid);
            assert!(ctx.w.get(kid).unwrap().alive, "x = room_width is inside");
            spawn(ctx, k.killer, 90.0, 90.0);
            ctx.w.get_mut(kid).unwrap().m.set_x(r(100.0)).unwrap();
            end_step(ctx, kid);
            assert!(!ctx.w.get(kid).unwrap().alive);
        },
    );
}

#[test]
fn the_suicide_press_kills_unless_frozen() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, _| {
            input(ctx, &[Action::Suicide], &[], &[]);
            c(ctx, kid, CAction::SuicidePress);
            assert!(ctx.w.get(kid).unwrap().alive, "held is not a press");
            kstate(ctx, kid).frozen = true;
            input(ctx, &[Action::Suicide], &[Action::Suicide], &[]);
            c(ctx, kid, CAction::SuicidePress);
            assert!(ctx.w.get(kid).unwrap().alive, "frozen");
            kstate(ctx, kid).frozen = false;
            c(ctx, kid, CAction::SuicidePress);
            assert!(!ctx.w.get(kid).unwrap().alive);
        },
    );
}

#[test]
fn the_c_lists_run_end_to_end() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _, _| {
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            kid::on_event(ctx, kid, &EventKind::Step).expect("step");
            assert_eq!(hv(ctx, kid).0, 3.0);
            end_step(ctx, kid);
            assert!(ctx.w.get(kid).unwrap().alive);
        },
    );
}
