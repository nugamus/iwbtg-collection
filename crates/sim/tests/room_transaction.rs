//! The engine-owned room transaction (#1174 A2): the declared stage order, the single commit
//! point, and the build order inside it. Behavior on `gm8` is unchanged; the tests observe the
//! recorder's event order (CLONE_SPEC rooms, Confirmed).

use iwbtg_sim::def::game::{GM8_ROOM_STAGES, RoomStage};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::events::EventKind;
use iwbtg_sim::room::{RoomRequest, StartPoint};
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::world::Flow;

use crate::room_support::*;

/// The lifecycle declares the gm8 order as data, so a profile can name another order (#1160).
#[test]
fn the_declared_stage_order_is_the_gm8_sequence() {
    let mut b = GameDefBuilder::new("stage-order");
    let r = b.room(room("r", 50));
    b.start_room(r);
    let def = b.build().unwrap();
    let stages = def.lifecycle().enter_room.stages;
    assert_eq!(
        stages,
        [
            RoomStage::RoomEnd,
            RoomStage::Prepare,
            RoomStage::Commit,
            RoomStage::Placed,
            RoomStage::GameStart,
            RoomStage::CreationCode,
            RoomStage::RoomStart,
        ]
    );
    assert_eq!(GM8_ROOM_STAGES, stages);
}

/// The first room is one transaction: placed instances in room-file order, then Game Start,
/// then the room's Room Start — not one event interleaved into another stage.
#[test]
fn placed_instances_are_built_in_file_order_before_room_start() {
    let mut b = GameDefBuilder::new("build-order");
    let a = probe_kind(&mut b, "a", false);
    let c = probe_kind(&mut b, "c", false);
    let r = b.room(room("r", 50));
    b.start_room(r);
    b.place(r, a, 0.0, 0.0).unwrap();
    b.place(r, c, 0.0, 0.0).unwrap();
    let def = b.build().unwrap();

    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (_w, result) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: r,
            difficulty: 0,
        },
    );
    result.unwrap();
    assert_eq!(
        host.log,
        [
            "Create a",
            "Create c",
            "GameStart a",
            "GameStart c",
            "RoomStart a",
            "RoomStart c",
        ],
        "the declared order: placed in file order, then Game Start, then Room Start"
    );
}

/// A room request raised while the transaction builds waits for the whole transaction to
/// finish; only `after_sequence` resolves it. The build is not cut short or interleaved.
#[test]
fn a_request_from_the_build_runs_after_the_transaction() {
    let mut b = GameDefBuilder::new("nested-request");
    let a = probe_kind(&mut b, "a", false);
    let z = probe_kind(&mut b, "z", false);
    let ra = b.room(room("a", 50));
    let rb = b.room(room("b", 50));
    b.start_room(ra);
    b.place(ra, a, 0.0, 0.0).unwrap();
    b.place(rb, z, 0.0, 0.0).unwrap();
    let def = b.build().unwrap();

    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let mut fired = false;
    host.react = Some(Box::new(move |_, ctx, _, name, ev| {
        if !fired && *ev == EventKind::Create && name == "a" {
            fired = true;
            ctx.request_room(RoomRequest::Goto(rb));
        }
        Flow::Continue
    }));
    let (w, result) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: ra,
            difficulty: 0,
        },
    );
    result.unwrap();
    assert_eq!(w.room, rb, "the nested request runs after the transaction");
    assert_eq!(
        host.log,
        [
            "Create a",
            "GameStart a",
            "RoomStart a",
            "RoomEnd a",
            "Create z",
            "RoomStart z",
        ],
        "the first transaction's stages all run before the chained change starts"
    );
}
