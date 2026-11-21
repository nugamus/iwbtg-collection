//! The lifecycle feel contract (#1149, decision 0005): death, retry, the N/N+1/N+2 init
//! sequence, one Kid at the checkpoint, the counters the checkpoint holds, the same music
//! event, and control on the first tick after the engine-owned delay.
//!
//! The same assertions are data in `tests/feel/lifecycle/retry_after_death.toml`; this test
//! gates them until the feel runner lands (#1161). Evidence: TR-ROOM-01
//! (`rely-the-needle-4-2/tr-room-01-a`, #453), #670, #1141.

use std::sync::Arc;

use iwbtg_sim::builtins;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, ParamValue, Params, RoomDef, RoomId,
    RoomRole, SoundDef, SoundId, SoundKind,
};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::{self, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::save::SaveSlot;
use iwbtg_sim::sound::{DllMode, SoundOutput, SoundTarget};
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::world::{KindState, World};
use iwbtg_sim::{Sim, SimOptions, StartPoint};

/// The one track the fixture plays.
const MUSIC: SoundId = SoundId(0);

struct Fixture {
    def: GameDef,
    boot: RoomId,
    play: RoomId,
    save: SaveSlot,
}

/// A game with an engine Boot room, one play room, the world built-in and the A1 Kid. The run
/// starts from a checkpoint in the play room, as a practice run does.
fn fixture() -> Fixture {
    let mut b = GameDefBuilder::new("lifecycle-retry");
    let mut kid = KindDef::new("kid", Builtin::Kid);
    kid.classes = ClassMask::KID;
    let kid_id = b.kind(kid);
    b.engine_kind(EngineKind::Kid, kid_id);
    let mut world = KindDef::new("world", Builtin::World);
    world.persistent = true;
    let world_id = b.kind(world);
    b.boot(world_id);
    let mut boot = RoomDef::new("init", 800, 608);
    boot.role = RoomRole::Boot;
    let boot = b.room(boot);
    let play = b.room(RoomDef::new("play", 800, 608));
    b.start_room(play);
    // The play room's music object: its Create plays the track on every entry, so a retry that
    // keeps the music behavior plays the same event when it re-enters the checkpoint's room.
    let music = b.kind({
        let mut k = KindDef::new("music", Builtin::Music);
        let mut p = Params::default();
        p.0.insert("track".into(), ParamValue::Sound(MUSIC));
        k.params = p;
        k
    });
    b.place(play, music, 0.0, 0.0).expect("place the music");
    let physics = presets::defaults(KidFamily::A1);
    b.physics(physics.clone());
    let mut def = b.build().expect("game builds");
    for k in &mut def.kinds {
        k.events = if k.base == Builtin::Kid {
            kid::event_set(&physics)
        } else {
            builtins::event_set(k.base, k)
        };
    }
    def.sounds = vec![SoundDef {
        name: "music".into(),
        kind: SoundKind::Multimedia,
        length_ticks: 1_000,
        volume: Real::ONE,
        pan: Real::ZERO,
    }];
    let save = SaveSlot {
        room: play,
        x: Real::new(200.0),
        y: Real::new(500.0),
        flipped: false,
        facing: 1,
        difficulty: 0,
        deaths: 0,
        time: 0,
        bosses: 0,
        items: 0,
        clear: false,
    };
    Fixture {
        def,
        boot,
        play,
        save,
    }
}

fn sim(f: &Fixture) -> Sim {
    Sim::new(
        Arc::new(f.def.clone()),
        SimOptions {
            start: StartPoint::FromSave(f.save.clone()),
            seed: 0,
            dll_mode: DllMode::Stub,
            mode: PlayMode::Original,
            map: None,
        },
    )
    .expect("sim starts")
}

fn bits(actions: &[Action]) -> u32 {
    actions.iter().fold(0u32, |acc, a| acc | a.bit())
}

fn frame(held: &[Action], pressed: &[Action], released: &[Action]) -> InputFrame {
    InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: bits(released),
    }
}

fn kid_count(w: &World) -> usize {
    w.iter_alive()
        .filter(|i| matches!(i.state, KindState::Kid(_)))
        .count()
}

fn kid_x(sim: &Sim) -> f64 {
    let id = sim.world().kid(&sim.rules().tick).expect("a live kid");
    sim.world().get(id).expect("the kid").m.x().get()
}

/// How many times this tick's output starts the fixture's track.
fn music_plays(sim: &Sim) -> usize {
    sim.world()
        .output
        .iter()
        .filter(|e| {
            matches!(
                e,
                OutputEvent::Sound(SoundOutput::Play {
                    target: SoundTarget::Builtin(MUSIC),
                    ..
                })
            )
        })
        .count()
}

/// How many stop or stop-all events this tick's output has.
fn music_stops(sim: &Sim) -> usize {
    sim.world()
        .output
        .iter()
        .filter(|e| {
            matches!(
                e,
                OutputEvent::Sound(
                    SoundOutput::Stop {
                        target: SoundTarget::Builtin(MUSIC),
                        ..
                    } | SoundOutput::StopAll { .. }
                )
            )
        })
        .count()
}

/// The player-observable retry contract: R counts the death (already counted by the death
/// event), resets the room, leaves exactly one Kid at the checkpoint after the same N/N+1/N+2
/// ticks, restores the counters the checkpoint holds, plays the same music event, and gives
/// control on the tick after the declared delay. The package Boot room is the gm8 profile's
/// destination; the phase is engine state either way.
#[test]
fn retry_after_death_returns_to_the_checkpoint_after_the_delay() {
    let f = fixture();
    let mut sim = sim(&f);
    assert_eq!(sim.world().room, f.play, "the run starts from the checkpoint");
    assert_eq!(kid_count(sim.world()), 1);

    // Tick 1: the checkpoint Kid exists in the play room, and the room's music object started
    // its track on entry (the first step keeps what `Sim::new` produced).
    sim.step(InputFrame::default());
    assert_eq!(sim.world().tick, 1);
    assert_eq!(kid_count(sim.world()), 1);
    assert_eq!(music_plays(&sim), 1, "the play room's music starts");

    // Ticks 2-4: the Kid dies on the suicide press; the death counter and the death-only save
    // record it. Only one Kid ever exists.
    sim.step(frame(&[], &[Action::Suicide], &[]));
    sim.step(frame(&[], &[], &[Action::Suicide]));
    sim.step(InputFrame::default());
    assert_eq!(sim.world().tick, 4);
    assert_eq!(sim.world().engine.run.deaths, 1, "the death is counted");
    assert_eq!(kid_count(sim.world()), 0);
    let checkpoint = sim.world().save.clone().expect("the death-only save");
    assert_eq!(checkpoint.room, f.play);
    assert_eq!(checkpoint.deaths, 1, "the checkpoint holds the counter");

    // Tick 5 (N): R starts the retry. The room is reset to the Boot-role room and the
    // engine-owned init phase begins; no Kid is left behind.
    sim.step(frame(&[Action::Restart], &[Action::Restart], &[]));
    assert_eq!(sim.world().tick, 5);
    assert_eq!(sim.world().room, f.boot, "game_restart enters the Boot room");
    assert_eq!(kid_count(sim.world()), 0);
    assert_eq!(
        sim.world().save.as_ref().map(|s| s.room),
        Some(f.play),
        "the checkpoint file survives the restart"
    );
    assert_eq!(music_stops(&sim), 0, "R does not stop the music");

    // Tick 6 (N+1): the init phase's last tick loads the checkpoint and enters its room. The
    // Kid is created at the saved spot, with no motion that tick, and the counters come back.
    // The room's music object asks for its track again, but it is the same track already
    // playing, so the music is uninterrupted: no play and no stop event.
    sim.step(frame(&[], &[], &[Action::Restart]));
    assert_eq!(sim.world().tick, 6);
    assert_eq!(sim.world().room, f.play, "N+1 enters the checkpoint's room");
    assert_eq!(kid_count(sim.world()), 1, "exactly one Kid after the load");
    assert_eq!(kid_x(&sim), f.save.x.get(), "the Kid is at the checkpoint");
    assert_eq!(
        sim.world().engine.run.deaths,
        1,
        "the checkpoint's counters come back"
    );
    assert_eq!(
        music_plays(&sim),
        0,
        "the retry keeps the same music playing: no new play event"
    );
    assert_eq!(music_stops(&sim), 0, "and never stops it");

    // Tick 7 (N+2): the first controllable tick. A Right press on this step (frame 6, observed
    // at tick 7) moves the Kid: latency 1 from the press frame.
    let before = kid_x(&sim);
    sim.step(frame(&[Action::Right], &[Action::Right], &[]));
    assert_eq!(sim.world().tick, 7);
    assert_eq!(sim.world().room, f.play, "no further room change");
    assert_eq!(kid_count(sim.world()), 1);
    assert!(
        kid_x(&sim) > before,
        "control returns on the first tick after the init delay"
    );
    assert_eq!(sim.world().engine.run.deaths, 1, "the counter stays");
}
