//! WP5a.4: kid death, the Create-time engine actions, the play timer and the family presets
//! (docs/plans/sim-core.md 5.3, kid/A1.md "Death", "Create", "Alarm 0").

use iwbtg_sim::def::physics::{
    DeathStep, JumpRulesDraft, KidEventsDraft, KidNumbersDraft, PhysicsProfileDraft,
};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef, RoomId,
    VarId, room::RoomRole,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::Action;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::api::{self, KillCause};
use iwbtg_sim::kid::{self, KidAction, KidState, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::save::SaveSlot;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Flow, InstId, KindState, SpawnInit};
use iwbtg_sim::{Ctx, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
    world: KindId,
    bow: KindId,
    blood_emitter: KindId,
    game_over: KindId,
    killer: KindId,
}

fn game(with_engine_kinds: bool, tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("wp5a-death");
    let inert = |name: &str, classes: ClassMask, b: &mut GameDefBuilder| {
        let mut k = KindDef::new(name, Builtin::Inert);
        k.classes = classes;
        b.kind(k)
    };
    let kid = b.kind({
        let mut k = KindDef::new("kid", Builtin::Kid);
        k.classes = ClassMask::KID;
        k
    });
    let world = inert("world", ClassMask::NONE, &mut b);
    let bow = inert("bow", ClassMask::BOW, &mut b);
    let blood_emitter = inert("blood_emitter", ClassMask::NONE, &mut b);
    let game_over = inert("game_over", ClassMask::NONE, &mut b);
    let killer = inert("killer", ClassMask::KILLER, &mut b);
    b.def_mut().var_names.insert("flipped".into(), VarId(0));
    b.engine_kind(EngineKind::Kid, kid);
    if with_engine_kinds {
        b.engine_kind(EngineKind::World, world);
        b.engine_kind(EngineKind::Bow, bow);
        b.engine_kind(EngineKind::BloodEmitter, blood_emitter);
        b.engine_kind(EngineKind::GameOver, game_over);
    }
    let play = b.room(RoomDef::new("play", 800, 608));
    let mut menu = RoomDef::new("menu", 800, 608);
    menu.role = RoomRole::Menu;
    b.room(menu);
    let mut select = RoomDef::new("select", 800, 608);
    select.role = RoomRole::Select;
    b.room(select);
    b.start_room(play);
    let mut p = presets::defaults(KidFamily::A1);
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds {
            kid,
            world,
            bow,
            blood_emitter,
            game_over,
            killer,
        },
    )
}

fn with_kid<T>(
    with_engine_kinds: bool,
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds) -> T,
) -> T {
    let (def, k) = game(with_engine_kinds, tweak);
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
        .spawn(k.kid, r(100.0), r(200.0), SpawnInit::default())
        .expect("kid");
    f(&mut ctx, kid, &k)
}

fn count(ctx: &Ctx, kind: KindId) -> usize {
    ctx.w.iter_alive().filter(|i| i.kind == kind).count()
}

fn first(ctx: &Ctx, kind: KindId) -> InstId {
    ctx.w.iter_alive().find(|i| i.kind == kind).expect("instance").id
}

fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

fn slot(deaths: u32, time: u32) -> SaveSlot {
    SaveSlot {
        room: RoomId(0),
        x: r(1.0),
        y: r(2.0),
        flipped: false,
        facing: 1,
        difficulty: 0,
        deaths,
        time,
        bosses: 0,
        items: 0,
        clear: false,
    }
}

fn run(ctx: &mut Ctx, kid: InstId, a: CommonAction) {
    let flow = kid::run_action(ctx, kid, &KidAction::Common(a)).expect("action");
    assert_eq!(flow, Flow::Continue);
}

// ---- Death ----

#[test]
fn a_kill_runs_the_a1_death_sequence() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, k| {
            ctx.w.save = Some(slot(5, 7));
            ctx.w.engine.run.deaths = 5;
            ctx.w.engine.run.time = 9;
            let flow = api::kill(ctx, kid, KillCause::Script).expect("kill");
            assert_eq!(flow, Flow::Continue);
            assert!(!ctx.w.get(kid).unwrap().alive, "the kid is destroyed");
            let be = ctx.w.get(first(ctx, k.blood_emitter)).unwrap();
            assert_eq!(
                (be.m.x(), be.m.y()),
                (r(100.0), r(200.0)),
                "blood emitter at the kid"
            );
            assert_eq!(
                count(ctx, k.game_over),
                1,
                "GAMEOVER places itself (its own offset params)"
            );
            assert_eq!(ctx.w.engine.run.deaths, 6);
            let s = ctx.w.save.as_ref().unwrap();
            assert_eq!(
                (s.deaths, s.time),
                (6, 9),
                "the death-only save writes the counters"
            );
        },
    );
}

#[test]
fn the_death_order_is_the_profiles() {
    with_kid(
        true,
        |p| p.death.order = vec![DeathStep::DeathsPlusOne, DeathStep::GameOver],
        |ctx, kid, k| {
            api::kill(ctx, kid, KillCause::Script).expect("kill");
            assert!(ctx.w.get(kid).unwrap().alive, "no destroy step: the kid lives");
            assert_eq!(count(ctx, k.blood_emitter), 0);
            assert_eq!(count(ctx, k.game_over), 1);
            assert_eq!(ctx.w.engine.run.deaths, 1);
        },
    );
}

#[test]
fn muteki_ignores_the_kill() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, k| {
            kstate(ctx, kid).invincible = true;
            api::kill(ctx, kid, KillCause::Killer(kid)).expect("kill");
            assert!(ctx.w.get(kid).unwrap().alive);
            assert_eq!(count(ctx, k.blood_emitter), 0);
            assert_eq!(ctx.w.engine.run.deaths, 0);
        },
    );
}

#[test]
fn a_game_without_blood_or_gameover_kinds_just_skips_them() {
    with_kid(
        false,
        |_| {},
        |ctx, kid, _| {
            api::kill(ctx, kid, KillCause::Script).expect("kill");
            assert!(!ctx.w.get(kid).unwrap().alive);
            assert_eq!(ctx.w.engine.run.deaths, 1);
        },
    );
}

#[test]
fn a_kill_in_a_select_room_goes_to_that_room_again_and_does_nothing_else() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, k| {
            ctx.w.room = RoomId(2);
            let flow = api::kill(ctx, kid, KillCause::Script).expect("kill");
            assert_eq!(flow, Flow::AbortTick);
            assert_eq!(ctx.w.pending, Some(RoomRequest::Goto(RoomId(2))));
            assert!(!ctx.w.get(kid).unwrap().alive);
            assert_eq!(count(ctx, k.blood_emitter) + count(ctx, k.game_over), 0);
            assert_eq!(ctx.w.engine.run.deaths, 0);
        },
    );
}

#[test]
fn the_select_special_case_is_a_profile_switch() {
    with_kid(
        true,
        |p| p.death.select_room_reset = false,
        |ctx, kid, k| {
            ctx.w.room = RoomId(2);
            let flow = api::kill(ctx, kid, KillCause::Script).expect("kill");
            assert_eq!(flow, Flow::Continue);
            assert_eq!(count(ctx, k.blood_emitter), 1);
        },
    );
}

#[test]
fn death_that_stops_the_music_is_unsupported_until_wp10() {
    with_kid(
        true,
        |p| p.death.stop_music = true,
        |ctx, kid, _| {
            assert!(api::kill(ctx, kid, KillCause::Script).is_err());
        },
    );
}

#[test]
fn the_suicide_key_and_the_killer_collision_kill_through_their_rows() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, _| {
            let set = kid::event_set(&ctx.def.physics);
            assert_ne!(set.key_press & Action::Suicide.bit(), 0);
            assert_eq!(set.key_press & Action::Jump.bit(), 0);
            assert_eq!(set.key_held, 0);
            kid::on_event(ctx, kid, &EventKind::KeyPress(Action::Jump)).expect("event");
            assert!(ctx.w.get(kid).unwrap().alive, "no row for Jump");
            kid::on_event(ctx, kid, &EventKind::KeyPress(Action::Suicide)).expect("event");
            assert!(!ctx.w.get(kid).unwrap().alive);
        },
    );
    with_kid(
        true,
        |_| {},
        |ctx, kid, k| {
            let killer = ctx
                .spawn(k.killer, r(0.0), r(0.0), SpawnInit::default())
                .expect("killer");
            let ev = EventKind::Collision {
                with: ClassMask::KILLER,
                other: killer,
            };
            kid::on_event(ctx, kid, &ev).expect("event");
            assert!(!ctx.w.get(kid).unwrap().alive);
            assert_eq!(ctx.w.engine.run.deaths, 1);
        },
    );
}

// ---- Create-time engine actions ----

#[test]
fn ensure_world_spawns_one_world_only_when_none_exists() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, k| {
            // The kid's Create (the A1 preset) already made the world.
            assert_eq!(count(ctx, k.world), 1);
            run(ctx, kid, CommonAction::EnsureWorld);
            assert_eq!(count(ctx, k.world), 1, "one exists: no second");
            let w = first(ctx, k.world);
            ctx.w.get_mut(w).unwrap().alive = false;
            run(ctx, kid, CommonAction::EnsureWorld);
            assert_eq!(count(ctx, k.world), 1, "the dead one is replaced");
        },
    );
}

#[test]
fn create_bow_puts_a_bow_at_the_kid() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, k| {
            let before = count(ctx, k.bow);
            run(ctx, kid, CommonAction::CreateBow);
            assert_eq!(count(ctx, k.bow), before + 1);
            let bow = ctx.w.iter_alive().filter(|i| i.kind == k.bow).last().unwrap();
            assert_eq!((bow.m.x(), bow.m.y()), (r(100.0), r(200.0)));
        },
    );
}

#[test]
fn first_play_save_writes_a_save_only_when_none_exists() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, _| {
            ctx.w.save = None; // the kid's Create already saved once
            ctx.w.engine.run.deaths = 4;
            ctx.w.engine.run.time = 4;
            run(ctx, kid, CommonAction::FirstPlaySave);
            let s = ctx.w.save.clone().expect("saved");
            assert_eq!((s.x, s.y), (r(100.0), r(200.0)));
            assert_eq!(
                (s.deaths, s.time),
                (0, 0),
                "a first save starts the counters at 0"
            );
            ctx.w.engine.run.deaths = 9;
            run(ctx, kid, CommonAction::FirstPlaySave);
            assert_eq!(
                ctx.w.save.as_ref().unwrap().deaths,
                0,
                "an existing save is left alone"
            );
        },
    );
}

// ---- Play timer ----

#[test]
fn arm_play_timer_sets_alarm_zero_to_room_speed() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, _| {
            run(ctx, kid, CommonAction::ArmPlayTimer);
            assert_eq!(ctx.w.get(kid).unwrap().alarms[0], r(50.0));
        },
    );
}

#[test]
fn alarm_zero_counts_time_in_play_rooms_and_re_arms() {
    with_kid(
        true,
        |_| {},
        |ctx, kid, _| {
            run(ctx, kid, CommonAction::ArmPlayTimer);
            ctx.w.get_mut(kid).unwrap().alarms[0] = r(0.0);
            kid::on_event(ctx, kid, &EventKind::Alarm(0)).expect("alarm");
            assert_eq!(ctx.w.engine.run.time, 1);
            assert_eq!(ctx.w.get(kid).unwrap().alarms[0], r(50.0));
            kid::on_event(ctx, kid, &EventKind::Alarm(1)).expect("other alarm");
            assert_eq!(ctx.w.engine.run.time, 1, "only alarm 0 has a row");
        },
    );
}

#[test]
fn the_play_timer_does_not_count_in_menu_or_select_rooms_but_still_re_arms() {
    for room in [RoomId(1), RoomId(2)] {
        with_kid(
            true,
            |_| {},
            |ctx, kid, _| {
                ctx.w.room = room;
                kid::on_event(ctx, kid, &EventKind::Alarm(0)).expect("alarm");
                assert_eq!(ctx.w.engine.run.time, 0);
                assert_eq!(ctx.w.get(kid).unwrap().alarms[0], r(50.0));
            },
        );
    }
}

#[test]
fn event_set_handles_alarm_zero() {
    let p = presets::defaults(KidFamily::A1);
    assert_eq!(kid::event_set(&p).alarms, 1);
}

// ---- Presets ----

#[test]
fn the_a1_preset_is_rely() {
    let p = presets::defaults(KidFamily::A1);
    assert_eq!(p.preset, KidFamily::A1);
    assert_eq!(p.numbers.jumps, vec![r(8.5), r(7.0)]);
    assert_eq!(
        (p.numbers.gravity, p.numbers.run_speed, p.numbers.max_vspeed),
        (r(0.4), r(3.0), r(9.0))
    );
    assert_eq!(p.numbers.release_mult, r(0.45));
    assert_eq!(p.platform.snap, r(9.0));
    assert_eq!(p.vine.kick, [r(15.0), r(-9.0)]);
    assert_eq!((p.bullet.max, p.bullet.speed), (4, r(16.0)));
    assert!(p.misc.fade_in);
    assert_eq!(p.events.step.len(), 10);
}

#[test]
fn an_empty_draft_is_the_default_of_its_family() {
    let mut d = PhysicsProfileDraft::default();
    assert_eq!(presets::apply(&mut d), presets::defaults(KidFamily::A1));
    let mut d = PhysicsProfileDraft {
        preset: Some(KidFamily::B),
        ..Default::default()
    };
    let p = presets::apply(&mut d);
    assert_eq!(p.preset, KidFamily::B, "the family is recorded");
}

#[test]
fn a_draft_overrides_single_fields_and_keeps_the_rest() {
    let mut d = PhysicsProfileDraft {
        numbers: Some(KidNumbersDraft {
            run_speed: Some(r(4.0)),
            ..Default::default()
        }),
        jump: Some(JumpRulesDraft {
            ground_sets_djump: Some(false),
            ..Default::default()
        }),
        events: Some(KidEventsDraft {
            step: Some(vec![KidAction::Common(CommonAction::Walk)]),
            ..Default::default()
        }),
        ..Default::default()
    };
    let p = presets::apply(&mut d);
    let base = presets::defaults(KidFamily::A1);
    assert_eq!(p.numbers.run_speed, r(4.0));
    assert_eq!(
        p.numbers.jumps, base.numbers.jumps,
        "other fields keep the preset's value"
    );
    assert!(
        !p.jump.ground_sets_djump,
        "an explicit false overrides a true default"
    );
    assert_eq!(p.jump.ground_classes, base.jump.ground_classes);
    assert_eq!(p.events.step, vec![KidAction::Common(CommonAction::Walk)]);
    assert_eq!(p.events.create, base.events.create);
}

#[test]
fn the_blood_emitter_is_told_whether_the_kid_was_flipped() {
    use iwbtg_sim::world::Value;
    for flipped in [false, true] {
        with_kid(
            true,
            |_| {},
            |ctx, kid, k| {
                kstate(ctx, kid).flipped = flipped;
                api::kill(ctx, kid, KillCause::Script).expect("kill");
                let be = ctx.w.get(first(ctx, k.blood_emitter)).unwrap();
                let want = if flipped { Real::ONE } else { Real::ZERO };
                assert_eq!(be.vars.get(&VarId(0)), Some(&Value::Real(want)));
            },
        );
    }
}
