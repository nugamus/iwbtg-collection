//! The room change sequence: the seven-step order, the Room Start dispatch switch and the
//! Room End depth switch.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::num::Real;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::room::{RoomOutput, RoomRequest, StartPoint};
use iwbtg_sim::rules::RunnerRules;

use crate::room_support::*;

#[test]
fn seven_step_order_with_probes() {
    let o = order_game();
    let rules = RunnerRules::default();
    let mut host = Probe {
        watch: Some(o.g),
        react: Some(ps_spawns_kid()),
        ..Probe::default()
    };
    let (mut w, r) = start(
        &o.def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: o.a,
            difficulty: 2,
        },
    );
    r.unwrap();
    assert_eq!(
        host.log,
        [
            // Boot instance, before any room.
            "Create world",
            // Step 4: placed instances in file order.
            "Create ps",
            "Create blk",
            // Step 5: first room only, dispatch order (rank: blk, world, ps).
            "GameStart blk",
            "GameStart world",
            "GameStart ps",
            // Step 6: creation actions; the global is set before the spawn.
            "Create spawned+g",
            // Step 7: the room's own instances in creation order, then the carried world.
            // The kid created in playerStart's Room Start gets no Room Start (R10).
            "RoomStart ps+g",
            "Create kid+g",
            "RoomStart blk+g",
            "RoomStart spawned+g",
            "RoomStart world+g",
        ]
    );
    assert_eq!(w.room, o.a);
    assert_eq!(w.room_speed, Real::new(30.0), "file speed on entry (R14)");
    assert_eq!(w.engine.run.difficulty, 2);
    let world_id = ids_of(&w, &o.def, "world");
    let kid_id = ids_of(&w, &o.def, "kid");
    assert_eq!(world_id.len(), 1);
    assert!(
        w.get(world_id[0]).unwrap().persistent,
        "boot instances are persistent"
    );

    host.log.clear();
    w.room_tick = 99;
    change(&o.def, &rules, &mut w, &mut host, RoomRequest::Goto(o.b)).unwrap();
    assert_eq!(
        host.log,
        [
            // Step 1: every instance, dispatch order (kid, blk, world, ps, spawned).
            "RoomEnd kid+g",
            "RoomEnd blk+g",
            "RoomEnd world+g",
            "RoomEnd ps+g",
            "RoomEnd spawned+g",
            // Steps 2-4: the non-persistent ones are gone without events; b's block.
            "Create blk+g",
            // No Game Start; no creation actions; Room Start: own, then carried by id.
            "RoomStart blk+g",
            "RoomStart world+g",
            "RoomStart kid+g",
        ]
    );
    assert_eq!(w.room, o.b);
    assert_eq!(w.room_tick, 0);
    assert_eq!(w.room_speed, Real::new(50.0));
    assert_eq!(ids_of(&w, &o.def, "world"), world_id, "persistent ids are kept");
    assert_eq!(ids_of(&w, &o.def, "kid"), kid_id);
    assert!(ids_of(&w, &o.def, "ps").is_empty());
    assert_eq!(w.instances().len(), 3);
    assert!(w.pending.is_empty());
    assert!(w.output.contains(&OutputEvent::Room(RoomOutput::Entered {
        from: Some(o.a),
        to: o.b
    })));
    assert!(
        w.output
            .contains(&OutputEvent::Room(RoomOutput::Entered { from: None, to: o.a }))
    );
}

#[test]
fn room_end_follows_the_dispatch_order() {
    let mut b = GameDefBuilder::new("depth");
    let deep = probe_kind(&mut b, "deep", false);
    let shallow = probe_kind(&mut b, "shallow", false);
    b.def_mut().kinds[0].depth = Real::new(-10.0);
    b.def_mut().kinds[1].depth = Real::new(100.0);
    let r0 = b.room(room("r0", 50));
    b.room(room("r1", 50));
    b.place(r0, deep, 0.0, 0.0).unwrap();
    b.place(r0, shallow, 0.0, 0.0).unwrap();
    let def = b.build().unwrap();
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: r0,
            difficulty: 0,
        },
    );
    r.unwrap();
    host.log.clear();
    change(&def, &rules, &mut w, &mut host, RoomRequest::Next).unwrap();
    // Rank, then id; depth plays no part.
    assert_eq!(host.log, ["RoomEnd deep", "RoomEnd shallow"]);
}
