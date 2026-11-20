//! Persistent rooms: deactivation, restore without Create, goto and restart, and next/previous.

use iwbtg_sim::error::SimError;
use iwbtg_sim::motion::Motion;
use iwbtg_sim::num::Real;
use iwbtg_sim::room::{RoomRequest, StartPoint};
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::world::{InstId, SimStatus};

use crate::room_support::*;

#[test]
fn deactivated_persistent_stays_behind() {
    let (def, p, q) = two_rooms(false);
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: p,
            difficulty: 0,
        },
    );
    r.unwrap();
    let sleeper = deactivate(&mut w, &def, "sleeper");
    host.log.clear();
    change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(q)).unwrap();
    assert!(
        w.get(sleeper).is_none(),
        "a deactivated persistent instance does not move"
    );
    assert!(
        !host.log.iter().any(|l| l.contains("sleeper")),
        "inactive: no events"
    );
    assert_eq!(ids_of(&w, &def, "keeper").len(), 1);
    assert!(
        w.persistent_rooms.is_empty(),
        "a non-persistent room stores nothing"
    );
}

#[test]
fn persistent_room_restores_without_create() {
    let (def, p, q) = two_rooms(true);
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: p,
            difficulty: 0,
        },
    );
    r.unwrap();
    let sleeper = deactivate(&mut w, &def, "sleeper");
    let local = ids_of(&w, &def, "local")[0];
    w.get_mut(local).unwrap().m = Motion::new_at(Real::new(77.0), Real::new(5.0));
    change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(q)).unwrap();
    assert!(w.get(sleeper).is_none() && w.get(local).is_none());
    assert_eq!(
        w.persistent_rooms.get(&p).map(Vec::len),
        Some(2),
        "left behind and stored"
    );

    host.log.clear();
    change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(p)).unwrap();
    assert_eq!(
        host.log,
        ["RoomEnd keeper", "RoomStart local", "RoomStart keeper"],
        "no Create on a revisit; Room Start yes; the inactive sleeper gets nothing (R18)"
    );
    let back = w.get(local).expect("restored with its id");
    assert_eq!(back.m.x(), Real::new(77.0), "restored state is kept");
    assert!(
        w.get(sleeper).is_some_and(|s| !s.active),
        "restored, still inactive"
    );
    assert!(w.persistent_rooms.is_empty());
    let ids: Vec<InstId> = w.instances().iter().map(|i| i.id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "the store stays in creation order");
    assert_eq!(w.room_speed, Real::new(50.0));
}

#[test]
fn goto_current_and_restart_rebuild() {
    let (def, p, _) = two_rooms(false);
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: p,
            difficulty: 0,
        },
    );
    r.unwrap();
    let before = ids_of(&w, &def, "local");
    for req in [RoomRequest::Restart, RoomRequest::Goto(p)] {
        host.log.clear();
        change(&def, &rules, &mut w, &mut host, req).unwrap();
        assert!(host.log.contains(&"Create local".to_string()), "{req:?} rebuilds");
        let now = ids_of(&w, &def, "local");
        assert_eq!(now.len(), 1);
        assert_ne!(now, before, "{req:?} makes a fresh instance");
        assert_eq!(w.room, p);
    }
}

#[test]
fn restart_of_persistent_room_is_unsupported() {
    let (def, p, _) = two_rooms(true);
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: p,
            difficulty: 0,
        },
    );
    r.unwrap();
    for req in [RoomRequest::Restart, RoomRequest::Goto(p)] {
        let mut w = w.clone();
        let err = change(&def, &rules, &mut w, &mut host, req).unwrap_err();
        assert!(matches!(err, SimError::Unsupported { .. }), "{req:?}");
        assert_eq!(w.status, SimStatus::Halted(err));
        assert!(w.persistent_rooms.is_empty(), "{req:?}: nothing was stored");
    }
}

#[test]
fn next_previous_and_past_the_end() {
    let (def, p, q) = two_rooms(false);
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: p,
            difficulty: 0,
        },
    );
    r.unwrap();
    change(&def, &rules, &mut w, &mut host, RoomRequest::Next).unwrap();
    assert_eq!(w.room, q);
    change(&def, &rules, &mut w, &mut host, RoomRequest::Previous).unwrap();
    assert_eq!(w.room, p);
    let err = change(&def, &rules, &mut w, &mut host, RoomRequest::Previous).unwrap_err();
    assert!(matches!(err, SimError::Script { .. }));
    assert!(matches!(w.status, SimStatus::Halted(_)));
    assert_eq!(w.room, p, "nothing changed");
    // A halted sim does nothing more.
    host.log.clear();
    assert!(change(&def, &rules, &mut w, &mut host, RoomRequest::Next).is_err());
    assert!(host.log.is_empty());
    assert_eq!(w.room, p);
}
