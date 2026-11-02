//! WP5c.2 (#129): family A4 and A5 rules on hand-built worlds (kid/A4.md, kid/A5.md): the
//! shifting fall cap, the down-only cap, machinary's fixed room edges, little-runner's End Step,
//! spook house's frozen Step and idle sprite rule.

use iwbtg_sim::def::physics::{BoundaryModel, DeathStep, FallCapRule};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::a5::A5Action;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::{self, KidAction, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
    block: KindId,
}

fn game(
    family: KidFamily,
    tweak: impl FnOnce(&mut PhysicsProfile),
) -> (GameDef, Kinds, iwbtg_sim::def::SpriteId) {
    let mut b = GameDefBuilder::new("wp5c-a4a5");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let idle = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(kid_sprite);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let mut blk = KindDef::new("block", Builtin::Inert);
    blk.sprite = Some(tile);
    blk.classes = ClassMask::BLOCK;
    blk.solid = true;
    let block = b.kind(blk);
    let mut bow = KindDef::new("bow", Builtin::Inert);
    bow.classes = ClassMask::BOW;
    let bow = b.kind(bow);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::Bow, bow);
    let room = b.room(RoomDef::new("wide", 7232, 608));
    b.start_room(room);
    let mut p = presets::defaults(family);
    p.death.order = vec![DeathStep::DestroyAllKids];
    p.sprites.idle = Some(idle);
    tweak(&mut p);
    b.physics(p);
    (b.build().expect("game builds"), Kinds { kid, block }, idle)
}

fn with_kid<T>(
    family: KidFamily,
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds, iwbtg_sim::def::SpriteId) -> T,
) -> T {
    let (def, k, idle) = game(family, tweak);
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let mut w = World::new(def.start_room, 1);
    w.room_speed = r(50.0);
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
    kid::run_action(&mut ctx, kid, &KidAction::Common(CommonAction::CreateDefaults)).expect("create");
    f(&mut ctx, kid, &k, idle)
}

fn spawn(ctx: &mut Ctx, kind: KindId, x: f64, y: f64) -> InstId {
    ctx.spawn(kind, r(x), r(y), SpawnInit::default()).expect("spawn")
}

fn common(ctx: &mut Ctx, kid: InstId, a: CommonAction) {
    kid::run_action(ctx, kid, &KidAction::Common(a)).expect("action");
}

fn set(ctx: &mut Ctx, id: InstId, x: f64, y: f64, hs: f64, vs: f64) {
    let m = &mut ctx.w.get_mut(id).unwrap().m;
    m.set_x(r(x)).unwrap();
    m.set_y(r(y)).unwrap();
    m.set_hspeed(r(hs)).unwrap();
    m.set_vspeed(r(vs)).unwrap();
}

fn pos(ctx: &Ctx, id: InstId) -> (f64, f64) {
    let m = &ctx.w.get(id).unwrap().m;
    (m.x().get(), m.y().get())
}

fn vs(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.vspeed().get()
}

fn input(ctx: &mut Ctx, held: &[Action], pressed: &[Action], released: &[Action]) {
    let bits = |l: &[Action]| l.iter().fold(0u32, |a, x| a | x.bit());
    ctx.w.input.latch(InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: bits(released),
    });
}

// ---- A4: the fall cap ----

#[test]
fn the_shifting_fall_cap_moves_y_back_by_the_excess_then_clamps() {
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            // Falling at 12: y shifts up by 12 - 8 = 4, vspeed clamps to 9.
            set(ctx, kid, 100.0, 100.0, 0.0, 12.0);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!((pos(ctx, kid).1, vs(ctx, kid)), (96.0, 9.0));
            // Rising at -10: y shifts down by 2, vspeed clamps to -9 (both directions).
            set(ctx, kid, 100.0, 100.0, 0.0, -10.0);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!((pos(ctx, kid).1, vs(ctx, kid)), (102.0, -9.0));
            // At or below 8: nothing.
            set(ctx, kid, 100.0, 100.0, 0.0, 8.0);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!((pos(ctx, kid).1, vs(ctx, kid)), (100.0, 8.0));
            // Between 8 and 9: shifted, not clamped.
            set(ctx, kid, 100.0, 100.0, 0.0, 8.5);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!((pos(ctx, kid).1, vs(ctx, kid)), (99.5, 8.5));
        },
    );
}

#[test]
fn the_down_only_cap_leaves_a_fast_rise_alone() {
    with_kid(
        KidFamily::A5,
        |p| p.fall_cap.rule = FallCapRule::ClampDownOnly,
        |ctx, kid, _, _| {
            set(ctx, kid, 100.0, 100.0, 0.0, 12.0);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!(vs(ctx, kid), 9.0);
            set(ctx, kid, 100.0, 100.0, 0.0, -12.0);
            common(ctx, kid, CommonAction::FallCap);
            assert_eq!(vs(ctx, kid), -12.0);
        },
    );
}

// ---- A4: jumps, release, boundary ----

#[test]
fn a4_ground_jump_leaves_djump_alone_and_release_halves() {
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.block, 80.0, 109.0);
            if let iwbtg_sim::world::KindState::Kid(s) = &mut ctx.w.get_mut(kid).unwrap().state {
                s.djump = false;
            }
            input(ctx, &[Action::Jump], &[Action::Jump], &[]);
            common(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), -8.5);
            let iwbtg_sim::world::KindState::Kid(s) = &ctx.w.get(kid).unwrap().state else {
                unreachable!()
            };
            assert!(!s.djump, "a ground jump does not touch djump in A4");
            input(ctx, &[], &[], &[Action::Jump]);
            common(ctx, kid, CommonAction::Release);
            assert_eq!(vs(ctx, kid), -4.25, "multiplier 0.5");
        },
    );
}

/// `muteki`: `killPlayer` does nothing, so the position adjustment that follows it shows.
fn tough(ctx: &mut Ctx, kid: InstId) {
    if let iwbtg_sim::world::KindState::Kid(s) = &mut ctx.w.get_mut(kid).unwrap().state {
        s.invincible = true;
    }
}

fn boundary(ctx: &mut Ctx, kid: InstId) {
    kid::on_event(ctx, kid, &EventKind::IntersectBoundary).expect("boundary");
}

#[test]
fn machinary_edges_are_800_by_608_whatever_the_room_and_wrap_by_those_numbers() {
    // x < 0 in a 7232 wide room: add 800, undo the vertical move. (No room changer: it dies,
    // but the position is adjusted after the kill all the same.)
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            tough(ctx, kid);
            set(ctx, kid, -3.0, 300.0, 0.0, 4.0);
            boundary(ctx, kid);
            assert!(
                ctx.w.get(kid).unwrap().alive,
                "muteki: killPlayer does nothing, the wrap still runs"
            );
            assert_eq!(pos(ctx, kid), (797.0, 296.0));
            assert_eq!(pos(ctx, kid), (797.0, 296.0));
        },
    );
    // Past 800 inside a 7232 wide room counts as outside too.
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            tough(ctx, kid);
            set(ctx, kid, 900.0, 300.0, 0.0, 0.0);
            boundary(ctx, kid);
            assert_eq!(pos(ctx, kid), (100.0, 300.0));
        },
    );
    // The top adds 608; the bottom subtracts 600, not 608.
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            tough(ctx, kid);
            set(ctx, kid, 50.0, -2.0, 0.0, 0.0);
            boundary(ctx, kid);
            assert_eq!(pos(ctx, kid).1, 606.0);
        },
    );
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            tough(ctx, kid);
            set(ctx, kid, 50.0, 620.0, 0.0, 0.0);
            boundary(ctx, kid);
            assert_eq!(pos(ctx, kid).1, 20.0, "620 - 600");
        },
    );
    // Inside the 800 by 608 box: nothing.
    with_kid(
        KidFamily::A4,
        |_| {},
        |ctx, kid, _, _| {
            tough(ctx, kid);
            set(ctx, kid, 800.0, 608.0, 0.0, 0.0);
            boundary(ctx, kid);
            assert!(ctx.w.get(kid).unwrap().alive);
            assert_eq!(pos(ctx, kid), (800.0, 608.0));
        },
    );
}

#[test]
fn the_fixed_model_wires_the_boundary_event() {
    let p = presets::defaults(KidFamily::A4);
    assert_eq!(p.events.boundary, BoundaryModel::IntersectOriginFixed);
    assert!(kid::event_set(&p).intersect_boundary);
}

// ---- A5: little-runner's End Step ----

#[test]
fn the_end_step_sets_gravity_every_tick_even_when_frozen() {
    with_kid(
        KidFamily::A5,
        |_| {},
        |ctx, kid, _, _| {
            ctx.w.get_mut(kid).unwrap().m.set_gravity(Real::ZERO).unwrap();
            if let iwbtg_sim::world::KindState::Kid(s) = &mut ctx.w.get_mut(kid).unwrap().state {
                s.frozen = true;
            }
            kid::run_action(ctx, kid, &KidAction::A5(A5Action::GravityAndUnstuck)).unwrap();
            assert_eq!(ctx.w.get(kid).unwrap().m.gravity(), r(0.4));
        },
    );
}

#[test]
fn a_kid_stuck_in_a_solid_goes_back_and_moves_to_contact() {
    with_kid(
        KidFamily::A5,
        |_| {},
        |ctx, kid, k, _| {
            spawn(ctx, k.block, 80.0, 110.0);
            // Came from y 100, ended inside the block at 106.
            let m = &mut ctx.w.get_mut(kid).unwrap().m;
            m.set_y(r(100.0)).unwrap();
            m.set_yprevious(r(100.0)).unwrap();
            m.set_y(r(106.0)).unwrap();
            m.set_vspeed(r(6.0)).unwrap();
            kid::run_action(ctx, kid, &KidAction::A5(A5Action::GravityAndUnstuck)).unwrap();
            assert_eq!(pos(ctx, kid).1, 101.0, "flush above the block");
            assert_eq!(vs(ctx, kid), 6.0, "vspeed is kept");
        },
    );
    with_kid(
        KidFamily::A5,
        |_| {},
        |ctx, kid, _, _| {
            set(ctx, kid, 100.0, 120.0, 0.0, 2.0);
            kid::run_action(ctx, kid, &KidAction::A5(A5Action::GravityAndUnstuck)).unwrap();
            assert_eq!(pos(ctx, kid).1, 120.0, "free: nothing moves");
        },
    );
}

// ---- Spook house: frozen Step, idle sprite ----

#[test]
fn stop_if_frozen_skips_the_rest_of_the_step_list() {
    let step = vec![
        KidAction::Common(CommonAction::StopIfFrozen),
        KidAction::Common(CommonAction::Walk),
    ];
    with_kid(
        KidFamily::A5,
        |p| p.events.step = step.clone(),
        |ctx, kid, _, _| {
            set(ctx, kid, 100.0, 100.0, 2.0, 0.0);
            ctx.w.engine.kid.frozen = true;
            input(ctx, &[Action::Right], &[], &[]);
            kid::on_event(ctx, kid, &EventKind::Step).unwrap();
            assert_eq!(
                ctx.w.get(kid).unwrap().m.hspeed(),
                r(2.0),
                "nothing ran, hspeed not rewritten"
            );
            ctx.w.engine.kid.frozen = false;
            kid::on_event(ctx, kid, &EventKind::Step).unwrap();
            assert_eq!(ctx.w.get(kid).unwrap().m.hspeed(), r(3.0));
        },
    );
}

#[test]
fn the_idle_sprite_needs_a_block_below_when_the_game_says_so() {
    with_kid(
        KidFamily::A5,
        |p| p.misc.idle_needs_block = true,
        |ctx, kid, k, idle| {
            let before = ctx.w.get(kid).unwrap().img.sprite();
            input(ctx, &[], &[], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(
                ctx.w.get(kid).unwrap().img.sprite(),
                before,
                "in the air the sprite is left alone"
            );
            spawn(ctx, k.block, 80.0, 109.0);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(ctx.w.get(kid).unwrap().img.sprite(), Some(idle));
        },
    );
}

// ---- The presets ----

#[test]
fn the_a4_and_a5_lists_run_end_to_end() {
    for family in [KidFamily::A4, KidFamily::A5] {
        with_kid(
            family,
            |_| {},
            |ctx, kid, _, _| {
                input(ctx, &[Action::Right], &[Action::Right], &[]);
                kid::on_event(ctx, kid, &EventKind::Step).expect("step");
                assert_eq!(ctx.w.get(kid).unwrap().m.hspeed(), r(3.0), "{family:?}");
                kid::on_event(ctx, kid, &EventKind::EndStep).expect("end step");
            },
        );
    }
}

#[test]
fn only_a5_sets_gravity_in_the_end_step() {
    assert!(presets::defaults(KidFamily::A4).events.end_step.is_empty());
    assert_eq!(presets::defaults(KidFamily::A5).events.end_step.len(), 1);
}
