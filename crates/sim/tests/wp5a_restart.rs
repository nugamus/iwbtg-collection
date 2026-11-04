//! WP4b follow-up (#224): the kid's flipped state and facing in saves, and the kid-side retry
//! steps of family B and showdown (kid/B.md "Restart", kid/A3.md showdown "Restart").

use iwbtg_sim::def::physics::AirJump;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, DeathOnRestart, EngineKind, GameDef, KindDef, KindId, PhysicsProfile, RestartKind,
    RetryKidSteps, RoomDef, RoomId,
};
use iwbtg_sim::kid::KidState;
use iwbtg_sim::kid::api::{self, ParamZone};
use iwbtg_sim::num::Real;
use iwbtg_sim::restart::run_restart;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::save::{SaveSlot, SavedPosition, load_into, save_now};
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

fn game(edit: impl FnOnce(&mut GameDef)) -> (GameDef, KindId) {
    let (def, kid, _) = game2(edit);
    (def, kid)
}

fn game2(edit: impl FnOnce(&mut GameDef)) -> (GameDef, KindId, KindId) {
    let mut b = GameDefBuilder::new("wp5a-restart");
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.persistent = true;
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    b.engine_kind(EngineKind::Kid, kid);
    let mut f = KindDef::new("kid2", Builtin::Kid);
    f.persistent = true;
    f.classes = ClassMask::KID_FLIPPED;
    let kid2 = b.kind(f);
    b.engine_kind(EngineKind::KidFlipped, kid2);
    let room = b.room(RoomDef::new("play", 800, 608));
    b.start_room(room);
    let mut p = PhysicsProfile::default(); // no Create actions: these tests build their own state
    p.numbers.jumps = vec![r(8.5), r(7.0)];
    p.numbers.run_speed = r(3.0);
    b.physics(p);
    let mut def = b.build().expect("game builds");
    edit(&mut def);
    (def, kid, kid2)
}

fn with_ctx<T>(def: &GameDef, f: impl FnOnce(&mut Ctx) -> T) -> T {
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let mut w = World::new(def.start_room, 1);
    w.room_speed = r(50.0);
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    f(&mut ctx)
}

fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

fn slot(flipped: bool, facing: i8) -> SaveSlot {
    SaveSlot {
        room: RoomId(0),
        x: r(70.0),
        y: r(119.0),
        flipped,
        facing,
        difficulty: 1,
        deaths: 3,
        time: 10,
        bosses: 0,
        items: 0,
        clear: false,
    }
}

fn live_kids(ctx: &Ctx, kind: KindId) -> Vec<InstId> {
    ctx.w
        .iter_alive()
        .filter(|i| i.kind == kind)
        .map(|i| i.id)
        .collect()
}

// ---- Flipped state and facing in saves ----

#[test]
fn a_save_reads_flipped_and_facing_from_the_kid_state() {
    let (def, kid) = game(|_| {});
    with_ctx(&def, |ctx| {
        let id = ctx.spawn(kid, r(10.0), r(20.0), SpawnInit::default()).unwrap();
        // A game that flips by a flag: one kind, the class never changes.
        kstate(ctx, id).flipped = true;
        kstate(ctx, id).facing = -1;
        save_now(ctx, id, SavedPosition::XyBase100Bytes, None).unwrap();
        let s = ctx.w.save.clone().unwrap();
        assert!(s.flipped);
        assert_eq!(s.facing, -1, "state.facing, although image_xscale is still 1");
        assert!(api::flipped(ctx.w, id));
        assert_eq!(api::saved_facing(ctx.w, id), -1);
    });
}

#[test]
fn a_load_restores_the_flipped_flag_yflag_and_facing() {
    let (def, _) = game(|_| {});
    with_ctx(&def, |ctx| {
        let id = load_into(ctx, &slot(true, -1)).unwrap();
        let s = kstate(ctx, id).clone();
        assert!(s.flipped);
        assert_eq!(s.yflag, -1);
        assert_eq!(s.facing, -1);
        assert_eq!(ctx.w.get(id).unwrap().img.xscale(), r(-1.0));
        let id = load_into(ctx, &slot(false, 1)).unwrap();
        let s = kstate(ctx, id).clone();
        assert!(!s.flipped && s.yflag == 1 && s.facing == 1);
    });
}

// ---- Kid-side retry steps ----

fn b_retry(def: &mut GameDef) {
    def.restart.kind = RestartKind::RoomGotoSaved;
    def.restart.kid_steps = RetryKidSteps {
        add_time: 2,
        save_death_time: true,
        clear_frozen: true,
        reset_jump_values: false,
        release_death_sound: false,
        clear_triggers: false,
    };
}

#[test]
fn family_b_retry_adds_two_to_the_timer_saves_it_and_clears_frozen() {
    let (def, kid) = game(b_retry);
    with_ctx(&def, |ctx| {
        ctx.w.save = Some(slot(false, 1));
        ctx.w.engine.run.time = 10;
        ctx.w.engine.run.deaths = 3;
        ctx.w.engine.kid.frozen = true;
        ctx.w.engine.kid.frozen2 = true;
        let old = ctx.spawn(kid, r(1.0), r(2.0), SpawnInit::default()).unwrap();
        kstate(ctx, old).frozen = true;
        run_restart(ctx, old).unwrap();
        assert_eq!(ctx.w.engine.run.time, 12, "+2");
        assert_eq!(ctx.w.save.as_ref().unwrap().time, 12, "saveDeathTime wrote it");
        assert_eq!(ctx.w.engine.run.deaths, 3, "B counts no death on R");
        assert!(!ctx.w.engine.kid.frozen && !ctx.w.engine.kid.frozen2);
        let kids = live_kids(ctx, kid);
        assert_eq!(
            kids.len(),
            1,
            "the old kid is gone, a new one stands at the saved spot"
        );
        assert_ne!(kids[0], old);
        assert!(!kstate(ctx, kids[0]).frozen);
        assert_eq!(ctx.w.get(kids[0]).unwrap().m.x(), r(70.0));
    });
}

#[test]
fn r_does_nothing_in_a_no_restart_room() {
    // animus room52 (#445): a play room where the original's R event is off.
    let (mut def, kid) = game(|d| d.restart.kind = RestartKind::RoomGotoSaved);
    let start = def.start_room;
    def.rooms[start.0 as usize].no_restart = true;
    with_ctx(&def, |ctx| {
        ctx.w.save = Some(slot(false, 1));
        let old = ctx.spawn(kid, r(1.0), r(2.0), SpawnInit::default()).unwrap();
        run_restart(ctx, old).unwrap();
        assert_eq!(ctx.w.get(old).unwrap().m.x(), r(1.0), "the kid stays");
        assert!(ctx.w.pending.is_none());
    });
}

#[test]
fn a_death_only_save_without_a_death_is_save_death_time_on_game_restart_via_boot() {
    // wanna (C13): saveDeathTime runs before loadGame and R adds no death (#441).
    let (def, kid) = game(|d| {
        d.restart.kind = RestartKind::GameRestartViaBoot;
        d.restart.kid_steps.save_death_time = true;
    });
    with_ctx(&def, |ctx| {
        ctx.w.save = Some(slot(false, 1));
        ctx.w.engine.run.time = 10;
        ctx.w.engine.run.deaths = 3;
        let old = ctx.spawn(kid, r(1.0), r(2.0), SpawnInit::default()).unwrap();
        run_restart(ctx, old).unwrap();
        assert_eq!(ctx.w.engine.run.deaths, 3, "no death");
        assert_eq!(ctx.w.save.as_ref().unwrap().time, 10, "the time is saved");
        assert_eq!(ctx.w.save.as_ref().unwrap().deaths, 3, "and the deaths");
    });
}

#[test]
fn the_steps_are_off_unless_the_model_asks() {
    let (def, kid) = game(|d| d.restart.kind = RestartKind::RoomGotoSaved);
    with_ctx(&def, |ctx| {
        ctx.w.save = Some(slot(false, 1));
        ctx.w.engine.run.time = 10;
        ctx.w.engine.kid.frozen = true;
        let old = ctx.spawn(kid, r(1.0), r(2.0), SpawnInit::default()).unwrap();
        run_restart(ctx, old).unwrap();
        assert_eq!(ctx.w.engine.run.time, 10);
        assert!(ctx.w.engine.kid.frozen);
    });
}

fn showdown_retry(def: &mut GameDef) {
    def.restart.kind = RestartKind::ShowdownLoad;
    def.restart.death_on_restart = DeathOnRestart::SaveCountersPerKid;
    def.restart.kid_steps = RetryKidSteps {
        clear_frozen: true,
        reset_jump_values: true,
        clear_triggers: true,
        ..RetryKidSteps::default()
    };
    def.physics.jump.air = AirJump::DjumpThenTjump;
}

#[test]
fn showdown_retry_clears_every_trigger_array() {
    let (def, kid) = game(showdown_retry);
    with_ctx(&def, |ctx| {
        ctx.w.save = Some(slot(false, 1));
        ctx.spawn(kid, r(1.0), r(2.0), SpawnInit::default()).unwrap();
        ctx.w.engine.mech.global_triggered = r(3.0);
        ctx.w.engine.mech.trigger_flags.insert(4);
        ctx.w.engine.mech.stop_flags.insert(5);
        ctx.w.engine.mech.switches.insert(6);
        run_restart(ctx, InstId(1)).unwrap();
        let m = &ctx.w.engine.mech;
        assert_eq!(m.global_triggered, Real::ZERO);
        assert!(m.trigger_flags.is_empty() && m.stop_flags.is_empty() && m.switches.is_empty());
    });
    // Without the step the arrays stay.
    let (def, kid) = game(|d| {
        showdown_retry(d);
        d.restart.kid_steps.clear_triggers = false;
    });
    with_ctx(&def, |ctx| {
        ctx.w.save = Some(slot(false, 1));
        ctx.spawn(kid, r(1.0), r(2.0), SpawnInit::default()).unwrap();
        ctx.w.engine.mech.trigger_flags.insert(4);
        run_restart(ctx, InstId(1)).unwrap();
        assert!(ctx.w.engine.mech.trigger_flags.contains(&4));
    });
}

#[test]
fn showdown_retry_clears_frozen_adds_a_death_and_resets_the_jump_values() {
    let (def, kid) = game(showdown_retry);
    with_ctx(&def, |ctx| {
        ctx.w.save = Some(slot(false, 1));
        ctx.w.engine.run.deaths = 3;
        let old = ctx.spawn(kid, r(1.0), r(2.0), SpawnInit::default()).unwrap();
        // A speed zone changed the old kid's numbers; the retry must not carry them over.
        let mut zone = ParamZone::default();
        zone.params
            .0
            .insert("run_speed".into(), iwbtg_sim::def::ParamValue::Real(r(5.0)));
        zone.params
            .0
            .insert("jump".into(), iwbtg_sim::def::ParamValue::Real(r(16.0)));
        api::apply_param_zone(ctx, old, &zone).unwrap();
        kstate(ctx, old).frozen = true;
        run_restart(ctx, old).unwrap();
        assert_eq!(ctx.w.engine.run.deaths, 4);
        assert_eq!(ctx.w.save.as_ref().unwrap().deaths, 4);
        let kids = live_kids(ctx, kid);
        assert_eq!(kids.len(), 1);
        let s = kstate(ctx, kids[0]).clone();
        assert_eq!(s.jump_speeds, def.physics.numbers.jumps);
        assert!(s.djump && s.tjump && !s.frozen);
        let m = &ctx.w.get(kids[0]).unwrap().m;
        assert_eq!((m.hspeed(), m.vspeed()), (Real::ZERO, Real::ZERO));
    });
}

#[test]
fn the_api_steps_work_on_their_own() {
    let (def, kid) = game(|_| {});
    with_ctx(&def, |ctx| {
        let a = ctx.spawn(kid, r(1.0), r(2.0), SpawnInit::default()).unwrap();
        let b = ctx.spawn(kid, r(3.0), r(4.0), SpawnInit::default()).unwrap();
        for id in [a, b] {
            kstate(ctx, id).frozen = true;
            kstate(ctx, id).frozen2 = true;
        }
        ctx.w.engine.kid.frozen = true;
        api::clear_frozen(ctx);
        assert!(!kstate(ctx, a).frozen && !kstate(ctx, b).frozen2 && !ctx.w.engine.kid.frozen);
        ctx.w.get_mut(a).unwrap().m.set_hspeed(r(3.0)).unwrap();
        api::reset_jump_values(ctx, a).unwrap();
        assert_eq!(ctx.w.get(a).unwrap().m.hspeed(), Real::ZERO);
        assert!(api::reset_jump_values(ctx, InstId(1)).is_err());
    });
}

// ---- Showdown's death count follows the live kids (#246) ----

#[test]
fn showdown_counts_one_death_per_live_kid_and_none_without() {
    let (def, kid, kid2) = game2(showdown_retry);
    // Zero kids: no death, no save write.
    with_ctx(&def, |ctx| {
        ctx.w.save = Some(slot(false, 1));
        ctx.w.engine.run.deaths = 3;
        ctx.w.engine.run.time = 77;
        run_restart(ctx, InstId(1)).unwrap();
        assert_eq!(ctx.w.engine.run.deaths, 3);
        assert_eq!(ctx.w.save.as_ref().unwrap().time, 10, "no death-only save either");
        assert_eq!(live_kids(ctx, kid).len(), 1, "the load still makes a kid");
    });
    // Two kids, one of each kind: two deaths.
    with_ctx(&def, |ctx| {
        ctx.w.save = Some(slot(false, 1));
        ctx.w.engine.run.deaths = 3;
        ctx.spawn(kid, r(1.0), r(2.0), SpawnInit::default()).unwrap();
        ctx.spawn(kid2, r(5.0), r(6.0), SpawnInit::default()).unwrap();
        run_restart(ctx, InstId(1)).unwrap();
        assert_eq!(ctx.w.engine.run.deaths, 5);
        assert_eq!(ctx.w.save.as_ref().unwrap().deaths, 5);
    });
}
