//! WP5c.1 (#128): family A2 and A3 rules on hand-built worlds (kid/A2.md, kid/A3.md): A/D
//! nudge, the game's vine-key flags, walking that ignores vines, the held suicide key, the End
//! Step killer test, and showdown's triple jump.

use iwbtg_sim::def::physics::{AirJump, DeathStep, VineKeys};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef, RoomId,
    room::RoomRole,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::a2::A2Action;
use iwbtg_sim::kid::actions::a3::A3Action;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::collisions::block::BlockRun;
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
    vine_l: KindId,
    vine_r: KindId,
    killer: KindId,
}

fn game(family: KidFamily, tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("wp5c-a2a3");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(kid_sprite);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let inert = |name: &str, classes: ClassMask, solid: bool, b: &mut GameDefBuilder| {
        let mut k = KindDef::new(name, Builtin::Inert);
        k.sprite = Some(tile);
        k.classes = classes;
        k.solid = solid;
        b.kind(k)
    };
    let block = inert("block", ClassMask::BLOCK, true, &mut b);
    let vine_l = inert("vine_l", ClassMask::VINE_L, false, &mut b);
    let vine_r = inert("vine_r", ClassMask::VINE_R, false, &mut b);
    let killer = inert("killer", ClassMask::KILLER, false, &mut b);
    let mut bow = KindDef::new("bow", Builtin::Inert);
    bow.classes = ClassMask::BOW;
    let bow = b.kind(bow);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::Bow, bow);
    let play = b.room(RoomDef::new("play", 800, 608));
    let mut menu = RoomDef::new("menu", 800, 608);
    menu.role = RoomRole::Menu;
    b.room(menu);
    let mut select = RoomDef::new("select", 800, 608);
    select.role = RoomRole::Select;
    b.room(select);
    b.start_room(play);
    let mut p = presets::defaults(family);
    p.death.order = vec![DeathStep::DestroyAllKids];
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds {
            kid,
            block,
            vine_l,
            vine_r,
            killer,
        },
    )
}

/// A kid at (100, 100) after `create_defaults`.
fn with_kid<T>(
    family: KidFamily,
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds) -> T,
) -> T {
    let (def, k) = game(family, tweak);
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
    run(&mut ctx, kid, KidAction::Common(CommonAction::CreateDefaults));
    f(&mut ctx, kid, &k)
}

fn run(ctx: &mut Ctx, kid: InstId, a: KidAction) {
    kid::run_action(ctx, kid, &a).expect("action");
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

fn a2(ctx: &mut Ctx, kid: InstId, a: A2Action) {
    run(ctx, kid, KidAction::A2(a));
}

fn a3(ctx: &mut Ctx, kid: InstId, a: A3Action) {
    run(ctx, kid, KidAction::A3(a));
}

fn common(ctx: &mut Ctx, kid: InstId, a: CommonAction) {
    run(ctx, kid, KidAction::Common(a));
}

// ---- A2: A/D nudge ----

#[test]
fn ad_nudge_moves_one_pixel_per_press_while_standing_on_a_block() {
    with_kid(
        KidFamily::A2,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.block, 80.0, 109.0);
            set_v(ctx, kid, 0.0, 0.0);
            input(ctx, &[], &[Action::Extra2], &[]);
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, 1.0, "D");
            input(ctx, &[], &[Action::Extra1], &[]);
            set_v(ctx, kid, 0.0, 0.0);
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, -1.0, "A");
            // Right + D adds to the walk speed (trace: hspeed 4).
            set_v(ctx, kid, 3.0, 0.0);
            input(ctx, &[], &[Action::Extra2], &[]);
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, 4.0);
            // Both keys cancel.
            set_v(ctx, kid, 0.0, 0.0);
            input(ctx, &[], &[Action::Extra1, Action::Extra2], &[]);
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, 0.0);
        },
    );
}

#[test]
fn ad_nudge_does_nothing_in_the_air_or_when_frozen() {
    with_kid(
        KidFamily::A2,
        |_| {},
        |ctx, kid, k| {
            input(ctx, &[], &[Action::Extra2], &[]);
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, 0.0, "no block below");
            spawn(ctx, k.block, 80.0, 109.0);
            kstate(ctx, kid).frozen = true;
            a2(ctx, kid, A2Action::AdNudge);
            assert_eq!(hv(ctx, kid).0, 0.0);
        },
    );
}

// ---- A3: key flags, vines, walking ----

#[test]
fn key_flags_stay_set_from_a_press_to_its_release() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, _| {
            input(
                ctx,
                &[Action::Right, Action::Jump],
                &[Action::Right, Action::Jump],
                &[],
            );
            a3(ctx, kid, A3Action::KeyFlags);
            let e = ctx.w.engine.kid.ext.clone();
            assert!(e.key_right && e.key_jump && !e.key_left);
            // No edge this tick: the flags stay (they are not the key state).
            input(ctx, &[], &[], &[]);
            a3(ctx, kid, A3Action::KeyFlags);
            assert!(ctx.w.engine.kid.ext.key_right);
            input(ctx, &[], &[], &[Action::Right]);
            a3(ctx, kid, A3Action::KeyFlags);
            let e = &ctx.w.engine.kid.ext;
            assert!(!e.key_right && e.key_jump);
        },
    );
}

#[test]
fn latched_flags_drive_the_vine_kick_and_drop() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.vine_l, 64.0, 96.0);
            // No key edge this tick, but the flags say Right and Jump.
            ctx.w.engine.kid.ext.key_right = true;
            ctx.w.engine.kid.ext.key_jump = true;
            input(ctx, &[], &[], &[]);
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid), (15.0, -9.0));
        },
    );
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.vine_l, 64.0, 96.0);
            ctx.w.engine.kid.ext.key_right = true;
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid), (3.0, 2.0), "no jump flag: the drop, slide speed 2");
        },
    );
    // `edges` would not have fired on the same input.
    with_kid(
        KidFamily::A3,
        |p| p.input.vine_keys = VineKeys::Edges,
        |ctx, kid, k| {
            spawn(ctx, k.vine_l, 64.0, 96.0);
            ctx.w.engine.kid.ext.key_right = true;
            common(ctx, kid, CommonAction::Vines);
            assert_eq!(hv(ctx, kid).0, 0.0);
        },
    );
}

#[test]
fn a3_walking_ignores_vines() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, k| {
            // A right vine flush against the kid: A1 would not let it walk left.
            spawn(ctx, k.vine_r, 106.0, 96.0);
            input(ctx, &[Action::Left], &[Action::Left], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(hv(ctx, kid).0, -3.0);
            assert_eq!(kstate(ctx, kid).facing, -1);
        },
    );
    with_kid(
        KidFamily::A1,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.vine_r, 106.0, 96.0);
            input(ctx, &[Action::Left], &[Action::Left], &[]);
            common(ctx, kid, CommonAction::Walk);
            assert_eq!(hv(ctx, kid).0, 0.0, "A1 keeps hspeed near its right vine");
        },
    );
}

// ---- A3: suicide and the End Step killer test ----

#[test]
fn the_held_suicide_key_kills_outside_menu_rooms() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, _| {
            input(ctx, &[], &[], &[]);
            a3(ctx, kid, A3Action::SuicideCheck);
            assert!(ctx.w.get(kid).unwrap().alive, "not held");
            input(ctx, &[Action::Suicide], &[], &[]);
            a3(ctx, kid, A3Action::SuicideCheck);
            assert!(!ctx.w.get(kid).unwrap().alive, "held, no press needed");
        },
    );
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, _| {
            ctx.w.room = RoomId(1); // the Menu room
            input(ctx, &[Action::Suicide], &[], &[]);
            a3(ctx, kid, A3Action::SuicideCheck);
            assert!(ctx.w.get(kid).unwrap().alive);
        },
    );
}

fn end_step(ctx: &mut Ctx, kid: InstId) {
    kid::on_event(ctx, kid, &EventKind::EndStep).expect("end step");
}

#[test]
fn the_end_step_killer_test_kills_after_movement() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.killer, 300.0, 300.0);
            end_step(ctx, kid);
            assert!(ctx.w.get(kid).unwrap().alive);
            spawn(ctx, k.killer, 90.0, 90.0);
            end_step(ctx, kid);
            assert!(!ctx.w.get(kid).unwrap().alive);
        },
    );
}

#[test]
fn the_killer_test_skips_menu_rooms_and_resets_in_select_rooms() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.killer, 90.0, 90.0);
            ctx.w.room = RoomId(1);
            end_step(ctx, kid);
            assert!(ctx.w.get(kid).unwrap().alive);
            ctx.w.room = RoomId(2);
            let m = &mut ctx.w.get_mut(kid).unwrap().m;
            m.set_xstart(r(10.0)).unwrap();
            m.set_ystart(r(20.0)).unwrap();
            end_step(ctx, kid);
            let m = &ctx.w.get(kid).unwrap().m;
            assert_eq!((m.x().get(), m.y().get()), (10.0, 20.0));
            assert!(ctx.w.get(kid).unwrap().alive, "moved to its start, not killed");
        },
    );
}

// ---- Showdown: the triple jump ----

fn showdown(p: &mut PhysicsProfile) {
    p.jump.air = AirJump::DjumpThenTjump;
    p.jump.release_twice_while_tjump = true;
    p.numbers.jumps = vec![r(8.5), r(7.0), r(6.0)];
}

#[test]
fn create_grants_the_third_jump() {
    with_kid(KidFamily::A3, showdown, |ctx, kid, _| {
        assert!(kstate(ctx, kid).tjump, "Create sets tjump (I8)");
    });
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, _| {
            assert!(!kstate(ctx, kid).tjump);
        },
    );
}

#[test]
fn jump_then_double_then_triple_then_nothing() {
    with_kid(KidFamily::A3, showdown, |ctx, kid, k| {
        let floor = spawn(ctx, k.block, 80.0, 109.0);
        input(ctx, &[Action::Jump], &[Action::Jump], &[]);
        common(ctx, kid, CommonAction::Jump);
        assert_eq!(hv(ctx, kid).1, -8.5);
        ctx.w.get_mut(floor).unwrap().alive = false;
        common(ctx, kid, CommonAction::Jump);
        assert_eq!(hv(ctx, kid).1, -7.0);
        assert!(!kstate(ctx, kid).djump && kstate(ctx, kid).tjump);
        common(ctx, kid, CommonAction::Jump);
        assert_eq!(hv(ctx, kid).1, -6.0, "the third jump");
        assert!(!kstate(ctx, kid).tjump);
        set_v(ctx, kid, 0.0, 1.0);
        common(ctx, kid, CommonAction::Jump);
        assert_eq!(hv(ctx, kid).1, 1.0, "a fourth press does nothing");
    });
}

#[test]
fn a_release_cuts_twice_while_the_third_jump_is_pending() {
    with_kid(KidFamily::A3, showdown, |ctx, kid, _| {
        set_v(ctx, kid, 0.0, -8.0);
        input(ctx, &[], &[], &[Action::Jump]);
        common(ctx, kid, CommonAction::Release);
        let once = (r(-8.0) * r(0.45)).get();
        assert_eq!(
            hv(ctx, kid).1,
            (r(once) * r(0.45)).get(),
            "x 0.45 x 0.45 (0.2025)"
        );
        kstate(ctx, kid).tjump = false;
        set_v(ctx, kid, 0.0, -8.0);
        common(ctx, kid, CommonAction::Release);
        assert_eq!(hv(ctx, kid).1, once);
    });
}

#[test]
fn landing_on_a_block_spends_the_third_jump() {
    with_kid(KidFamily::A3, showdown, |ctx, kid, k| {
        let b = spawn(ctx, k.block, 80.0, 110.0);
        set_v(ctx, kid, 0.0, 4.0);
        let run = KidCollisionRun::Block(BlockRun::BlockStandard);
        kid::run_collision(ctx, kid, b, &run).unwrap();
        assert!(!kstate(ctx, kid).tjump);
        assert!(kstate(ctx, kid).djump);
    });
}

// ---- The presets ----

#[test]
fn the_a2_preset_is_rely_without_the_fade() {
    let a2 = presets::defaults(KidFamily::A2);
    let a1 = presets::defaults(KidFamily::A1);
    assert_eq!(a2.preset, KidFamily::A2);
    assert!(!a2.misc.fade_in);
    assert_eq!(
        a2.events.step.len(),
        a1.events.step.len() - 2,
        "no platform_gravity_zero or fade_in"
    );
    assert_eq!(a2.numbers, a1.numbers);
}

#[test]
fn the_a3_lists_run_end_to_end() {
    with_kid(
        KidFamily::A3,
        |_| {},
        |ctx, kid, _| {
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            kid::on_event(ctx, kid, &EventKind::Step).expect("step");
            assert_eq!(hv(ctx, kid).0, 3.0);
            assert_eq!(
                ctx.w.get(kid).unwrap().m.gravity(),
                r(0.4),
                "gravity reasserted after the jump code"
            );
            assert!(ctx.w.engine.kid.ext.key_right);
            kid::on_event(ctx, kid, &EventKind::EndStep).expect("end step");
        },
    );
}
