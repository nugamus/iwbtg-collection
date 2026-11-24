//! WP5b.5 (#263): family B's death order and the several-kid rules (kid/B.md "Death", "Saving";
//! eat-the-lemon spawns more than one kid).

use iwbtg_sim::def::physics::{DeathStep, SavePosition};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, PhysicsProfile, RoomDef};
use iwbtg_sim::kid::api::{self, KillCause};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::save::{self, SavedPosition};
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
    blood_emitter: KindId,
    game_over: KindId,
}

fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("wp5b-death");
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
    let blood_emitter = inert("blood_emitter", ClassMask::NONE, &mut b);
    let game_over = inert("game_over", ClassMask::NONE, &mut b);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::BloodEmitter, blood_emitter);
    b.engine_kind(EngineKind::GameOver, game_over);
    let room = b.room(RoomDef::new("play", 800, 608));
    b.start_room(room);
    let mut p = PhysicsProfile::default();
    // kid/B.md killPlayer: death sound, blood emitter, GAMEOVER, destroy every kid, deaths + 1,
    // the death-only save.
    p.death.order = vec![
        DeathStep::Sound,
        DeathStep::BloodEmitter,
        DeathStep::GameOver,
        DeathStep::DestroyAllKids,
        DeathStep::DeathsPlusOne,
        DeathStep::SaveCounters,
    ];
    p.death.first_kid = true;
    p.save.first_kid = true;
    p.save.position = SavePosition::XyPrevious;
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds {
            kid,
            blood_emitter,
            game_over,
        },
    )
}

/// Two kids: the first at (100, 200), the second at (300, 400).
fn with_kids<T>(
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, InstId, &Kinds) -> T,
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
    let a = ctx
        .spawn(k.kid, r(100.0), r(200.0), SpawnInit::default())
        .expect("kid");
    let b = ctx
        .spawn(k.kid, r(300.0), r(400.0), SpawnInit::default())
        .expect("kid 2");
    for id in [a, b] {
        let m = &mut ctx.w.get_mut(id).unwrap().m;
        let (x, y) = (m.x(), m.y());
        m.set_xprevious(x).unwrap();
        m.set_yprevious(y).unwrap();
    }
    f(&mut ctx, a, b, &k)
}

fn count(ctx: &Ctx, kind: KindId) -> usize {
    ctx.w.iter_alive().filter(|i| i.kind == kind).count()
}

fn pos_of(ctx: &Ctx, kind: KindId) -> (f64, f64) {
    let i = ctx.w.iter_alive().find(|i| i.kind == kind).expect("instance");
    (i.m.x().get(), i.m.y().get())
}

#[test]
fn killing_either_kid_destroys_every_kid_and_counts_one_death() {
    for victim in [0, 1] {
        with_kids(
            |_| {},
            |ctx, a, b, k| {
                ctx.w.engine.run.deaths = 4;
                api::kill(ctx, [a, b][victim], KillCause::Script).unwrap();
                assert_eq!(count(ctx, k.kid), 0);
                assert_eq!(ctx.w.engine.run.deaths, 5, "one death, not one per kid");
                assert_eq!(count(ctx, k.blood_emitter), 1);
                assert_eq!(count(ctx, k.game_over), 1);
            },
        );
    }
}

#[test]
fn the_blood_emitter_sits_on_the_first_kid() {
    with_kids(
        |_| {},
        |ctx, a, b, k| {
            api::kill(ctx, b, KillCause::Script).unwrap();
            let _ = a;
            assert_eq!(
                pos_of(ctx, k.blood_emitter),
                (100.0, 200.0),
                "not the second kid's (300, 400)"
            );
        },
    );
}

#[test]
fn without_first_kid_the_effects_use_the_killed_kid() {
    with_kids(
        |p| p.death.first_kid = false,
        |ctx, _, b, k| {
            api::kill(ctx, b, KillCause::Script).unwrap();
            assert_eq!(pos_of(ctx, k.blood_emitter), (300.0, 400.0));
        },
    );
}

#[test]
fn destroy_kid_leaves_the_other_kids_alone() {
    with_kids(
        |p| p.death.order = vec![DeathStep::DestroyKid],
        |ctx, a, b, k| {
            api::kill(ctx, b, KillCause::Script).unwrap();
            assert_eq!(count(ctx, k.kid), 1);
            assert!(ctx.w.get(a).unwrap().alive);
        },
    );
}

#[test]
fn a_second_kill_in_the_same_tick_does_nothing() {
    with_kids(
        |_| {},
        |ctx, a, b, _| {
            ctx.w.engine.run.deaths = 0;
            api::kill(ctx, a, KillCause::Script).unwrap();
            api::kill(ctx, b, KillCause::Script).unwrap();
            assert_eq!(ctx.w.engine.run.deaths, 1);
        },
    );
}

#[test]
fn a_save_stores_the_first_kid_wherever_the_save_point_was_touched() {
    with_kids(
        |_| {},
        |ctx, _, b, _| {
            save::save_now(ctx, b, SavedPosition::XyPrevious, None).unwrap();
            let s = ctx.w.save.as_ref().expect("saved");
            assert_eq!(
                (s.x.get(), s.y.get()),
                (100.0, 200.0),
                "xprevious and yprevious of the first kid"
            );
        },
    );
    with_kids(
        |p| p.save.first_kid = false,
        |ctx, _, b, _| {
            save::save_now(ctx, b, SavedPosition::XyPrevious, None).unwrap();
            let s = ctx.w.save.as_ref().expect("saved");
            assert_eq!((s.x.get(), s.y.get()), (300.0, 400.0));
        },
    );
}
