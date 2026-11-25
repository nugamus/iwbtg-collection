//! The start room: StartPoint choices and the stripped (ranked) start, its cut rooms, kinds and
//! instances.

use iwbtg_sim::def::KindId;
use iwbtg_sim::def::NameId;
use iwbtg_sim::def::RoomId;
use iwbtg_sim::events::EventKind;
use iwbtg_sim::num::Real;
use iwbtg_sim::room::{RoomRequest, StartPoint, start_game_with};
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::save::{Checkpoint, RoomKey};
use iwbtg_sim::stripped::{PlayMode, RankedStart};
use iwbtg_sim::world::{Flow, SimStatus, Value, World};

use crate::room_support::*;

#[test]
fn start_points() {
    let (def, _select, _load, saved) = load_game();
    let mut def = def;
    def.new_game_room = saved;
    let rules = RunnerRules::default();

    let mut host = Probe::default();
    let mut w = World::new(def.start_room, 1);
    w.save = Some(slot(saved));
    with_ctx(&def, &rules, &mut w, |ctx| {
        start_game_with(ctx, &StartPoint::NewGame { difficulty: 3 }, &mut host)
    })
    .unwrap();
    assert_eq!(w.room, saved);
    assert_eq!(w.save, None, "a new game deletes the save");
    assert_eq!(w.engine.run.difficulty, 3);

    let mut host = Probe {
        react: Some(Box::new(|host, ctx, _, name, ev| {
            if name == "kid" && *ev == EventKind::Create {
                let room = &ctx.def.room(ctx.w.room).unwrap().name;
                host.log.push(format!(
                    "kid sees {room} at {} difficulty {}",
                    ctx.w.room_speed.get(),
                    ctx.w.engine.run.difficulty
                ));
            }
            Flow::Continue
        })),
        ..Probe::default()
    };
    let mut s = slot(saved);
    s.difficulty = 1;
    let (w, r) = start(&def, &rules, &mut host, from_save(&def, &s));
    r.unwrap();
    assert_eq!(
        host.log,
        [
            "Create world",
            "Load",
            "Create kid",
            // R14: the saved room's file speed, not 0 or the start room.
            "kid sees room14 at 30 difficulty 1",
            "Create marker",
            "GameStart world",
            "GameStart kid",
            "GameStart marker",
            "RoomStart marker",
            "RoomStart world",
            "RoomStart kid",
        ],
        "the loaded kid is carried in like a persistent instance"
    );
    assert_eq!(w.room, saved);
    assert!(w.pending.is_none(), "the load's own request is this entry");
    assert_eq!(w.save, Some(s));
    assert_eq!(w.engine.run.difficulty, 1);
    let kid = ids_of(&w, &def, "kid");
    assert_eq!(
        w.get(kid[0]).map(|k| (k.m.x(), k.m.y())),
        Some((Real::new(49.0), Real::new(119.0)))
    );
}

/// #1238: `FromSave` carries the checkpoint's stable room name, not an index. A reordering that
/// moves the room keeps the name pointing at it, and a name this package does not have halts the
/// start instead of landing in whatever room holds the old index.
#[test]
fn a_saved_start_resolves_the_room_name_in_the_room_sequence() {
    let (mut def, _select, load, saved) = load_game();
    let rules = RunnerRules::default();
    let checkpoint = Checkpoint::from_slot(&def, &slot(saved)).expect("a gameplay room");

    // Rooms: the save room and the load room trade places.
    def.rooms.swap(
        usize::try_from(load.0).expect("an index"),
        usize::try_from(saved.0).expect("an index"),
    );
    let moved = RoomId(
        u32::try_from(
            def.rooms
                .iter()
                .position(|r| r.name == "room14")
                .expect("still there"),
        )
        .unwrap(),
    );
    assert_ne!(moved, saved, "the fixture's reorder must move the room's index");

    let mut host = Probe::default();
    let (w, r) = start(&def, &rules, &mut host, StartPoint::FromSave(checkpoint.clone()));
    r.unwrap();
    assert_eq!(w.room, moved, "the name decides the room, not the stored index");

    let mut missing = checkpoint;
    missing.room = RoomKey("no-such-room".into());
    let mut host = Probe::default();
    let (w, r) = start(&def, &rules, &mut host, StartPoint::FromSave(missing));
    let e = r.expect_err("a room the package does not have cannot start");
    assert!(
        e.to_string().contains("no-such-room"),
        "the error names the room: {e}"
    );
    assert!(matches!(w.status, SimStatus::Halted(_)), "the start halts");
}

#[test]
fn stripped_new_game_starts_in_the_ranked_room_with_its_globals_set_first() {
    let (mut o, _) = stripped_game();
    o.def.stripped.start = RankedStart {
        room: Some(o.b),
        globals: vec![(o.g, Value::Real(Real::new(3.0)))],
        difficulty: Some(2),
    };
    let mut host = Probe {
        watch: Some(o.g),
        ..Probe::default()
    };
    let at = StartPoint::NewGame { difficulty: 0 };
    let (w, r) = start_in(&o.def, PlayMode::Stripped, &mut host, at.clone());
    r.unwrap();
    assert_eq!(w.room, o.b, "the ranked start room, not new_game_room");
    assert_eq!(w.globals.get(&o.g), Some(&Value::Real(Real::new(3.0))));
    assert_eq!(
        w.engine.run.difficulty, 2,
        "the `difficulty` global sets the engine's"
    );
    assert_eq!(
        host.log,
        [
            "Create world",
            "Create blk+g",
            "GameStart blk+g",
            "GameStart world+g",
            "RoomStart blk+g",
            "RoomStart world+g"
        ],
        "the globals are set after boot, before the start room's first Create"
    );

    // Play original ignores [stripped]: new_game_room, no globals.
    let mut host = Probe::default();
    let (w, r) = start_in(&o.def, PlayMode::Original, &mut host, at);
    r.unwrap();
    assert_eq!(w.room, o.def.new_game_room);
    assert_eq!(w.engine.run.difficulty, 0);
}

#[test]
fn stripped_start_without_a_room_uses_new_game_room() {
    let (mut o, _) = stripped_game();
    o.def.new_game_room = o.b;
    let mut host = Probe::default();
    let (w, r) = start_in(
        &o.def,
        PlayMode::Stripped,
        &mut host,
        StartPoint::NewGame { difficulty: 1 },
    );
    r.unwrap();
    assert_eq!(w.room, o.b);
    assert_eq!(
        w.engine.run.difficulty, 1,
        "no `difficulty` global keeps the start's"
    );
}

#[test]
fn stripped_cut_rooms_redirect_every_room_change() {
    let (mut o, c) = stripped_game();
    o.def.stripped.cut_rooms.insert(o.b, c);
    let rules = RunnerRules::default();
    let at = StartPoint::Room {
        room: o.a,
        difficulty: 0,
    };
    for (mode, want) in [(PlayMode::Stripped, c), (PlayMode::Original, o.b)] {
        for req in [RoomRequest::Goto(o.b), RoomRequest::Next] {
            let mut host = Probe::default();
            let (mut w, r) = start_in(&o.def, mode, &mut host, at.clone());
            r.unwrap();
            change(&o.def, &rules, &mut w, &mut host, req).unwrap();
            assert_eq!(w.room, want, "{mode:?} {req:?}");
        }
    }
    // A start point and a save load into a cut room land in its `to` room too.
    let mut host = Probe::default();
    let (w, r) = start_in(
        &o.def,
        PlayMode::Stripped,
        &mut host,
        StartPoint::Room {
            room: o.b,
            difficulty: 0,
        },
    );
    r.unwrap();
    assert_eq!(w.room, c);
    let mut host = Probe::default();
    let (w, r) = start_in(
        &o.def,
        PlayMode::Stripped,
        &mut host,
        from_save(&o.def, &slot(o.b)),
    );
    r.unwrap();
    assert_eq!(w.room, c);
}

#[test]
fn stripped_cut_kinds_skip_placed_instances_and_spawn_actions() {
    let (mut o, _) = stripped_game();
    let blk = o.def.kinds.iter().position(|k| k.name == "blk").unwrap();
    let spawned = o.def.kinds.iter().position(|k| k.name == "spawned").unwrap();
    for k in [blk, spawned] {
        o.def.stripped.cut_kinds.insert(KindId(u32::try_from(k).unwrap()));
    }
    let at = StartPoint::Room {
        room: o.a,
        difficulty: 0,
    };
    let mut host = Probe::default();
    let (w, r) = start_in(&o.def, PlayMode::Stripped, &mut host, at.clone());
    r.unwrap();
    assert!(ids_of(&w, &o.def, "blk").is_empty(), "placed blk is cut");
    assert!(
        ids_of(&w, &o.def, "spawned").is_empty(),
        "the spawn action is cut"
    );
    assert_eq!(ids_of(&w, &o.def, "ps").len(), 1, "other kinds stay");
    assert_eq!(
        w.globals.get(&o.g),
        Some(&Value::Real(Real::ONE)),
        "the room's other actions still run"
    );
    let mut host = Probe::default();
    let (w, r) = start_in(&o.def, PlayMode::Original, &mut host, at);
    r.unwrap();
    assert_eq!(ids_of(&w, &o.def, "blk").len(), 1);
    assert_eq!(ids_of(&w, &o.def, "spawned").len(), 1);
}

#[test]
fn stripped_cut_instances_skip_one_named_placed_instance() {
    let (mut o, _) = stripped_game();
    o.def.stripped.cut_instances.insert((o.a, NameId(0)));
    // The same name in another room is not cut.
    o.def.rooms[1].instances[0].name = Some(NameId(0));
    let at = StartPoint::Room {
        room: o.a,
        difficulty: 0,
    };
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start_in(&o.def, PlayMode::Stripped, &mut host, at.clone());
    r.unwrap();
    assert!(ids_of(&w, &o.def, "ps").is_empty(), "`tut` in room a is cut");
    assert_eq!(ids_of(&w, &o.def, "blk").len(), 1);
    change(&o.def, &rules, &mut w, &mut host, RoomRequest::Goto(o.b)).unwrap();
    assert_eq!(ids_of(&w, &o.def, "blk").len(), 1, "`tut` in room b stays");
    let mut host = Probe::default();
    let (w, r) = start_in(&o.def, PlayMode::Original, &mut host, at);
    r.unwrap();
    assert_eq!(ids_of(&w, &o.def, "ps").len(), 1);
}
