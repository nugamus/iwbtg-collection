//! #330: family B's `objDeathSound` (kid/B.md "Death (killPlayer)", traces kaenbyou
//! famb-death-timing-* and audio-supersound-deathfade-*): the death step creates it, it fades the
//! music each Step and plays the death sound one tick after its Create; the retry releases it.

use iwbtg_sim::builtins;
use iwbtg_sim::def::physics::DeathStep;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, KindId, ParamValue, Params, RoomDef,
    SoundDef, SoundId, SoundKind,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::api::{self, KillCause};
use iwbtg_sim::kid::{self, KidAction, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::{self, DllMode, SoundOutput, SoundTarget};
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, World};

const DEATH: SoundId = SoundId(0);
const MUSIC: SoundId = SoundId(1);

fn r(v: f64) -> Real {
    Real::new(v)
}

fn game() -> (GameDef, KindId, KindId) {
    let mut b = GameDefBuilder::new("death-sound");
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let mut dk = KindDef::new("death_sound", Builtin::DeathSound);
    dk.params.0.insert("sound".into(), ParamValue::Sound(DEATH));
    let ds = b.kind(dk);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::DeathSound, ds);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = presets::defaults(KidFamily::B);
    p.death.order = vec![DeathStep::DeathSound, DeathStep::DestroyKid];
    b.physics(p);
    let mut def = b.build().expect("game builds");
    let sd = |name: &str, kind: SoundKind| SoundDef {
        name: name.to_owned(),
        kind,
        length_ticks: 100,
        volume: Real::ONE,
        pan: Real::ZERO,
    };
    def.sounds = vec![sd("death", SoundKind::Normal), sd("music", SoundKind::Normal)];
    (def, kid, ds)
}

fn run<T>(f: impl FnOnce(&mut Ctx, KindId, KindId) -> T) -> T {
    let (def, kid, ds) = game();
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
    f(&mut ctx, kid, ds)
}

fn spawn_ds(ctx: &mut Ctx, ds: KindId) -> InstId {
    let mut p = Params::default();
    p.0.insert("sound".into(), ParamValue::Sound(DEATH));
    p.0.insert("music".into(), ParamValue::Sound(MUSIC));
    let id = ctx
        .spawn(
            ds,
            r(0.0),
            r(0.0),
            SpawnInit {
                params: p,
                ..SpawnInit::default()
            },
        )
        .expect("spawn");
    builtins::on_event(ctx, id, &EventKind::Create).expect("create");
    id
}

fn step(ctx: &mut Ctx, id: InstId) {
    builtins::on_event(ctx, id, &EventKind::Step).expect("step");
}

fn volumes(ctx: &Ctx) -> Vec<f64> {
    ctx.w
        .output
        .iter()
        .filter_map(|o| match o {
            OutputEvent::Sound(SoundOutput::Volume {
                target: SoundTarget::Builtin(s),
                value,
            }) if *s == MUSIC => Some(value.get()),
            _ => None,
        })
        .collect()
}

fn playing(ctx: &Ctx, s: SoundId) -> bool {
    sound::is_playing(ctx.w, ctx.def, s, &ctx.rules.sound)
}

#[test]
fn the_fade_sends_full_volume_first_then_drops_by_a_hundredth_of_full_each_tick() {
    run(|ctx, _, ds| {
        let id = spawn_ds(ctx, ds);
        for _ in 0..3 {
            step(ctx, id);
        }
        assert_eq!(volumes(ctx), vec![1.0, 0.99, 0.98]);
    });
}

#[test]
fn the_fade_stops_dropping_at_the_floor() {
    run(|ctx, _, ds| {
        let id = spawn_ds(ctx, ds);
        for _ in 0..130 {
            step(ctx, id);
        }
        let v = volumes(ctx);
        // 10000 down to 1900 takes 81 drops; from then on the volume stays at 0.19.
        assert!((v[80] - 0.2).abs() < 1e-9, "{}", v[80]);
        assert!((v[81] - 0.19).abs() < 1e-9 && (v[129] - 0.19).abs() < 1e-9);
    });
}

#[test]
fn muted_music_is_not_faded() {
    run(|ctx, _, ds| {
        ctx.w.engine.mech.music_muted = true;
        let id = spawn_ds(ctx, ds);
        step(ctx, id);
        assert!(volumes(ctx).is_empty());
    });
}

#[test]
fn the_death_sound_plays_one_tick_after_create_and_only_once_at_a_time() {
    run(|ctx, _, ds| {
        let id = spawn_ds(ctx, ds);
        assert!(!playing(ctx, DEATH), "not on the Create tick");
        builtins::on_event(ctx, id, &EventKind::Alarm(0)).expect("alarm");
        assert!(playing(ctx, DEATH));
        let copies = ctx.w.sound.copies.len();
        builtins::on_event(ctx, id, &EventKind::Alarm(0)).expect("alarm");
        assert_eq!(ctx.w.sound.copies.len(), copies, "already playing");
    });
}

#[test]
fn the_death_step_creates_the_object_and_the_retry_releases_it() {
    run(|ctx, kid, _| {
        let k = ctx.spawn(kid, r(1.0), r(2.0), SpawnInit::default()).unwrap();
        kid::run_action(ctx, k, &KidAction::Common(CommonAction::CreateDefaults)).unwrap();
        api::kill(ctx, k, KillCause::Boundary).expect("kill");
        let alive = |ctx: &Ctx| {
            ctx.w
                .iter_alive()
                .filter(|i| {
                    ctx.def
                        .kind(i.kind)
                        .is_some_and(|d| d.base == Builtin::DeathSound)
                })
                .count()
        };
        assert_eq!(alive(ctx), 1, "the death step made one");
        let id = ctx
            .w
            .iter_alive()
            .find(|i| {
                ctx.def
                    .kind(i.kind)
                    .is_some_and(|d| d.base == Builtin::DeathSound)
            })
            .unwrap()
            .id;
        let ds = ctx.w.get(id).unwrap();
        assert_eq!(
            (ds.m.x(), ds.m.y()),
            (Real::ZERO, Real::ZERO),
            "made at (0, 0), not at the kid (1, 2)"
        );
        builtins::on_event(ctx, id, &EventKind::Alarm(0)).unwrap();
        assert!(playing(ctx, DEATH));
        builtins::death_sound::release(ctx).expect("release");
        assert!(!playing(ctx, DEATH), "the death sound stops");
        assert_eq!(alive(ctx), 0, "and the object is gone");
    });
}

#[test]
fn without_a_music_param_it_fades_the_music_playing_at_create() {
    run(|ctx, _, ds| {
        // The looping music plays first; a one-shot effect after it is not music.
        sound::play(ctx, MUSIC, true).expect("music");
        sound::play(ctx, DEATH, false).expect("effect");
        let mut p = Params::default();
        p.0.insert("sound".into(), ParamValue::Sound(DEATH));
        let id = ctx
            .spawn(
                ds,
                r(0.0),
                r(0.0),
                SpawnInit {
                    params: p,
                    ..SpawnInit::default()
                },
            )
            .expect("spawn");
        builtins::on_event(ctx, id, &EventKind::Create).expect("create");
        for _ in 0..2 {
            step(ctx, id);
        }
        assert_eq!(volumes(ctx), vec![1.0, 0.99]);
    });
}

#[test]
fn without_a_music_param_and_no_music_playing_only_the_sound_plays() {
    run(|ctx, _, ds| {
        let mut p = Params::default();
        p.0.insert("sound".into(), ParamValue::Sound(DEATH));
        let id = ctx
            .spawn(
                ds,
                r(0.0),
                r(0.0),
                SpawnInit {
                    params: p,
                    ..SpawnInit::default()
                },
            )
            .expect("spawn");
        builtins::on_event(ctx, id, &EventKind::Create).expect("create");
        step(ctx, id);
        assert!(volumes(ctx).is_empty());
    });
}
