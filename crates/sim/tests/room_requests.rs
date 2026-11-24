//! Room requests around the sequence: nesting, load_game, the change bound, self-destroy in
//! creation code and halting.

use iwbtg_sim::def::RoomAction;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::error::SimError;
use iwbtg_sim::events::EventKind;
use iwbtg_sim::num::Real;
use iwbtg_sim::room::{RoomRequest, StartPoint};
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::trace::{TraceTable, Value as TV};
use iwbtg_sim::world::{Flow, SimStatus};

use crate::room_support::*;

#[test]
fn nested_request_chains_after_the_sequence() {
    let (def, select, load, saved) = load_game();
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: select,
            difficulty: 1,
        },
    );
    r.unwrap();
    w.save = Some(slot(saved));
    host.log.clear();
    change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(load)).unwrap();
    assert_eq!(
        host.log,
        [
            "RoomEnd world",
            "Create marker",
            "Load",
            "Create kid",
            // rLoad finishes all 7 steps (Design default; the recorder cannot see this).
            // The kid made by the creation code is in the room at step 7, so he gets it.
            "RoomStart marker",
            "RoomStart kid",
            "RoomStart world",
            // Then the chained change, before any tick.
            "RoomEnd world",
            "RoomEnd kid",
            "RoomEnd marker",
            "Create marker",
            "RoomStart marker",
            "RoomStart world",
            "RoomStart kid",
        ]
    );
    assert_eq!(
        w.room, saved,
        "0 ticks in rLoad (kaenbyou tr-room-12 rows 136-137)"
    );
    assert!(w.pending.is_none());
    assert_eq!(ids_of(&w, &def, "kid").len(), 1);
    assert_eq!(
        w.engine.run.difficulty, 0,
        "LoadGame applies the save's difficulty (tr-room-12 row 137; it was 1 before)"
    );
}

#[test]
fn load_game_without_save_is_an_error() {
    let (def, select, load, _) = load_game();
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: select,
            difficulty: 1,
        },
    );
    r.unwrap();
    let err = change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(load)).unwrap_err();
    assert!(matches!(err, SimError::Unsupported { .. }));
    assert!(matches!(w.status, SimStatus::Halted(_)));
}

#[test]
fn chained_changes_are_bounded() {
    let (res, entries, w) = looping(8);
    let err = res.unwrap_err();
    assert!(matches!(err, SimError::Unsupported { .. }), "{err:?}");
    assert_eq!(entries, 9, "the requested change plus 8 chained ones");
    assert_eq!(w.status, SimStatus::Halted(err));
    let (res, entries, _) = looping(2);
    assert!(res.is_err());
    assert_eq!(entries, 3);
}

#[test]
fn destroy_self_in_creation_code_is_a_no_op() {
    let mut b = GameDefBuilder::new("destroy");
    let mut r = room("stage", 50);
    r.on_create = vec![RoomAction::DestroySelf];
    let stage = b.room(r);
    let def = b.build().unwrap();
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: stage,
            difficulty: 0,
        },
    );
    // R24: nothing is destroyed and the start goes on.
    r.expect("the start succeeds");
    assert_eq!(w.status, SimStatus::Running);
    assert_eq!(w.room, stage);
}

#[test]
fn halt_inside_the_sequence_stops_it() {
    let o = order_game();
    let rules = RunnerRules::default();
    let mut host = Probe {
        react: Some(Box::new(|_, ctx, _, name, ev| {
            if name == "blk" && *ev == EventKind::Create {
                return ctx.halt(SimError::non_finite("x"));
            }
            Flow::Continue
        })),
        ..Probe::default()
    };
    let (w, r) = start(
        &o.def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: o.a,
            difficulty: 0,
        },
    );
    assert_eq!(r.unwrap_err(), SimError::non_finite("x"));
    assert_eq!(host.log, ["Create world", "Create ps", "Create blk"]);
    assert_eq!(w.status, SimStatus::Halted(SimError::non_finite("x")));
}

/// kaenbyou tr-room-12 (rSelectStage -> rLoad, whose creation code loads the save and calls
/// room_goto -> room14) against the sim on the same room chain: the room and difficulty of
/// the first row after the change are what the sim has once tick row 15 finishes, and no
/// row ever shows rLoad.
#[test]
#[allow(clippy::disallowed_methods, reason = "test-only: reads research trace files")]
fn tr_room_12_spends_no_tick_in_rload() {
    let Some(root) = std::env::var_os("IWBTG_RESEARCH") else {
        eprintln!("skipped: IWBTG_RESEARCH is not set");
        return;
    };
    let path = std::path::Path::new(&root).join("specs/engines/gm8/traces/kaenbyou/tr-room-12.csv");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let table = TraceTable::parse(&text).expect("trace parses");
    let col = |name: &str| table.column(name).unwrap_or_else(|| panic!("no column {name}"));
    let (frame, room_col, diff) = (col("frame"), col("room"), col("global.difficulty"));
    let row = |f: i64| {
        let r = table
            .rows
            .iter()
            .find(|r| r[frame] == TV::Int(f))
            .unwrap_or_else(|| panic!("frame {f} missing"));
        let TV::Str(room) = &r[room_col] else {
            panic!("frame {f}: room is not a string")
        };
        let TV::Real(d) = r[diff] else {
            panic!("frame {f}: difficulty is not a real")
        };
        (room.clone(), d)
    };
    assert!(
        !table.rows.iter().any(|r| r[room_col] == TV::Str("rLoad".into())),
        "no tick in rLoad"
    );
    let (before_room, before_diff) = row(136);
    let (after_room, after_diff) = row(137);
    // Part 1 saved on Medium: row 52 is the first room14 tick after the save.
    let saved_diff = row(52).1;
    let as_u8 = |d: f64| {
        (0..=3u8)
            .find(|&n| Real::new(f64::from(n)) == Real::new(d))
            .expect("difficulty 0-3")
    };

    let (def, select, load, saved) = load_game();
    assert_eq!(def.room(select).unwrap().name, before_room);
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: select,
            difficulty: as_u8(before_diff),
        },
    );
    r.unwrap();
    let mut s = slot(saved);
    s.difficulty = as_u8(saved_diff);
    w.save = Some(s);
    // Tick 136, row 15: the warp's request.
    change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(load)).unwrap();
    assert!(w.pending.is_none(), "nothing left for tick 137");
    assert_eq!(def.room(w.room).unwrap().name, after_room, "row 137's room");
    assert_eq!(w.engine.run.difficulty, as_u8(after_diff), "row 137's difficulty");
}
