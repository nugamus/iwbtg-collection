//! #629: the room creation code rows against showdown's rows.

use iwbtg_sim::def::{Params, RoomAction, RoomWhen};
use iwbtg_sim::num::Real;
use iwbtg_sim::room::{StartPoint, start_game_with};
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::world::{Value, World};

use crate::room_support::*;

#[test]
fn rows_629_move_the_kid_and_save_below_the_threshold() {
    let (def, stage, _) = rows_game(true, showdown_rows);
    let w = run_rows(&def, stage, 2);
    assert_eq!(kid_pos(&w, &def), (Real::new(241.0), Real::new(503.0)));
    let save = w.save.clone().expect("saved");
    assert_eq!(
        (save.room, save.x, save.y),
        (stage, Real::new(241.0), Real::new(503.0))
    );
    assert_eq!(save.difficulty, 2);
    // global.stage[global.savenum]: a fresh world's slot is 0, and GM8 aliases a global
    // with its element 0 (research#130), so this writes `stage` itself.
    assert_eq!(w.engine.run.slot, 0);
    let id = w.var_names["stage"];
    assert_eq!(w.globals.get(&id), Some(&Value::Real(Real::new(29.0))));
    assert!(!w.var_names.contains_key("stage[0]"));
}

#[test]
fn rows_629_at_or_above_the_threshold_do_nothing() {
    let (def, stage, _) = rows_game(true, showdown_rows);
    let w = run_rows(&def, stage, 3);
    assert_eq!(kid_pos(&w, &def), (Real::new(10.0), Real::new(20.0)));
    assert_eq!(w.save, None);
}

/// kid/A3.md "Saves", a3-t20-a: with no kid the move does nothing and the save holds 0, 0.
#[test]
fn rows_629_save_with_no_kid_is_at_zero() {
    let (def, stage, _) = rows_game(false, showdown_rows);
    let w = run_rows(&def, stage, 0);
    let save = w.save.clone().expect("saved");
    assert_eq!((save.room, save.x, save.y), (stage, Real::ZERO, Real::ZERO));
    assert_eq!(save.facing, 1);
    assert!(!save.flipped);
}

#[test]
fn rows_629_kid_exists_gates_the_save() {
    let rows = |_| {
        vec![when(
            RoomWhen {
                kid_exists: Some(true),
                ..RoomWhen::default()
            },
            RoomAction::SaveGame,
        )]
    };
    let (def, stage, _) = rows_game(false, rows);
    assert_eq!(run_rows(&def, stage, 0).save, None);
    let (def, stage, _) = rows_game(true, rows);
    assert!(run_rows(&def, stage, 0).save.is_some());
}

#[test]
fn rows_629_destroy_then_spawn_when_absent() {
    let rows = |junk| {
        let spawn = RoomAction::Spawn {
            kind: junk,
            x: Real::new(64.0),
            y: Real::ZERO,
            params: Params::default(),
        };
        let absent = RoomWhen {
            kind_absent: Some(junk),
            ..RoomWhen::default()
        };
        vec![
            // Two junk exist: nothing spawns.
            when(absent, spawn.clone()),
            RoomAction::Destroy { kind: junk },
            when(absent, spawn),
        ]
    };
    let (def, stage, _) = rows_game(true, rows);
    let w = run_rows(&def, stage, 0);
    let left: Vec<Real> = w
        .instances()
        .iter()
        .filter(|i| i.alive && def.kind(i.kind).unwrap().name == "junk")
        .map(|i| i.m.x())
        .collect();
    assert_eq!(left, [Real::new(64.0)]);
}

#[test]
fn rows_629_sounds() {
    use iwbtg_sim::def::SoundId;
    let snd = SoundId(0);
    let (def, stage, _) = rows_game(true, |_| {
        vec![
            RoomAction::PlaySound {
                sound: snd,
                looping: false,
            },
            RoomAction::StopSound { sound: snd },
            RoomAction::PlaySound {
                sound: snd,
                looping: true,
            },
        ]
    });
    let w = run_rows(&def, stage, 0);
    let copies: Vec<(SoundId, bool)> = w.sound.copies.iter().map(|c| (c.sound, c.looping)).collect();
    assert_eq!(copies, [(snd, true)]);
}

#[test]
fn rows_629_save_death_time_updates_the_counters_only() {
    let (def, stage, _) = rows_game(true, |_| vec![RoomAction::SaveDeathTime]);
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let mut w = World::new(def.start_room, 1);
    w.save = Some(slot(stage));
    w.engine.run.deaths = 12;
    w.engine.run.time = 34;
    with_ctx(&def, &rules, &mut w, |ctx| {
        start_game_with(
            ctx,
            &StartPoint::Room {
                room: stage,
                difficulty: 0,
            },
            &mut host,
        )
    })
    .unwrap();
    let save = w.save.clone().unwrap();
    assert_eq!((save.deaths, save.time), (w.engine.run.deaths, w.engine.run.time));
    assert_eq!(
        (save.x, save.y),
        (Real::new(49.0), Real::new(119.0)),
        "position kept"
    );
}
