//! Game end and restart on entry: the switches, creation code on revisit, particle systems,
//! views and the difficulty global.

use iwbtg_sim::collide::Target;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{EngineKind, RoomAction, RoomRole, VarId, ViewDef};
use iwbtg_sim::num::Real;
use iwbtg_sim::room::{RoomRequest, StartPoint};
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::world::{SimStatus, Value};

use crate::room_support::*;

/// A room's `set_global difficulty` also sets the engine difficulty built-ins read (#568).
#[test]
fn set_global_difficulty_sets_the_engine_difficulty() {
    let mut b = GameDefBuilder::new("difficulty");
    let g = VarId(0);
    let mut ra = room("a", 30);
    ra.on_create = vec![RoomAction::SetGlobal {
        var: g,
        value: Value::Real(Real::new(7.0)),
        difficulty: Some(7),
    }];
    let a = b.room(ra);
    let def = b.build().unwrap();
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: a,
            difficulty: 2,
        },
    );
    r.unwrap();
    assert_eq!(w.engine.run.difficulty, 7);
    assert_eq!(w.globals.get(&g), Some(&Value::Real(Real::new(7.0))));
}

#[test]
fn game_end_and_game_restart() {
    let mut b = GameDefBuilder::new("restart");
    let world = probe_kind(&mut b, "world", true);
    let kid = probe_kind(&mut b, "kid", true);
    b.boot(world);
    b.engine_kind(EngineKind::Kid, kid);
    let title = b.room(room("title", 50));
    let mut boot = room("rInit", 50);
    boot.role = RoomRole::Boot;
    let init = b.room(boot);
    let mut stage = room("stage", 30);
    stage.persistent = true;
    let stage = b.room(stage);
    b.start_room(stage);
    let def = b.build().unwrap();
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start(&def, &rules, &mut host, from_save(&def, &slot(stage)));
    r.unwrap();
    change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(title)).unwrap();
    assert!(w.persistent_rooms.contains_key(&stage));
    w.globals.insert(VarId(3), Value::Real(Real::ONE));
    w.engine.run.difficulty = 2;
    let old_world = ids_of(&w, &def, "world");

    host.log.clear();
    change(&def, &rules, &mut w, &mut host, RoomRequest::GameRestart).unwrap();
    assert_eq!(host.log, ["Create world", "GameStart world", "RoomStart world"]);
    assert_eq!(
        w.room, init,
        "the Boot-role room (engine init), not room order index 0 or start_room (#1149)"
    );
    assert!(
        ids_of(&w, &def, "kid").is_empty(),
        "persistent instances are removed too"
    );
    let new_world = ids_of(&w, &def, "world");
    assert_eq!(new_world.len(), 1);
    assert_ne!(new_world, old_world);
    assert!(
        w.globals.is_empty() && w.engine.run.difficulty == 0,
        "globals cleared (R16)"
    );
    assert!(w.persistent_rooms.is_empty());
    assert_eq!(w.save, Some(slot(stage)), "the save file survives");

    change(&def, &rules, &mut w, &mut host, RoomRequest::GameEnd).unwrap();
    assert_eq!(w.status, SimStatus::Ended);
    assert_eq!(w.room, init);
    // An ended sim changes nothing.
    change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(title)).unwrap();
    assert_eq!(w.room, init);
}

/// The Boot-role room is engine init, not a stage: next and previous step over it (#1149).
/// `game_restart` still enters it (`restart_room`), and an explicit `Goto` may name it.
#[test]
fn goto_next_and_previous_skip_the_boot_room() {
    let mut b = GameDefBuilder::new("skip-boot");
    let a = b.room(room("a", 50));
    let mut boot = room("init", 50);
    boot.role = RoomRole::Boot;
    let init = b.room(boot);
    let c = b.room(room("c", 50));
    let def = b.build().unwrap();
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: a,
            difficulty: 0,
        },
    );
    r.unwrap();
    change(&def, &rules, &mut w, &mut host, RoomRequest::Next).unwrap();
    assert_eq!(w.room, c, "next skips the Boot room");
    change(&def, &rules, &mut w, &mut host, RoomRequest::Previous).unwrap();
    assert_eq!(w.room, a, "previous skips the Boot room");
    change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(init)).unwrap();
    assert_eq!(w.room, init, "an explicit Goto may name the Boot room");
}

#[test]
fn creation_code_on_revisit_switch() {
    let (def, p, q) = revisit_game();
    for (on_revisit, want) in [(true, 2), (false, 1)] {
        let mut rules = RunnerRules::default();
        rules.room.creation_code_on_revisit = on_revisit;
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
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(q)).unwrap();
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(p)).unwrap();
        assert_eq!(ids_of(&w, &def, "made").len(), want, "on_revisit {on_revisit}");
        assert_eq!(host.params.len(), want);
        assert_eq!(
            host.params[0],
            ("made".to_string(), num_param()),
            "Spawn passes its params"
        );
    }
}

/// The runner destroys no particle system on a room change or on game_restart; the games'
/// own Room End code does (CLONE_SPEC "Particle draw rules" item 9, Confirmed (recorder)).
#[test]
fn particle_systems_survive_room_changes_and_game_restart() {
    use iwbtg_sim::particles::{PartRange, PartType, Particles, Region, RegionDistr, RegionShape};

    let mut b = GameDefBuilder::new("particles");
    let world = probe_kind(&mut b, "world", true);
    b.boot(world);
    let title = b.room(room("title", 50));
    let stage = b.room(room("stage", 30));
    b.start_room(title);
    let def = b.build().unwrap();
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (mut w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: title,
            difficulty: 0,
        },
    );
    r.unwrap();
    let mut p = Particles::default();
    let ty = p.add_type(PartType {
        life: PartRange::fixed(40.0),
        ..PartType::default()
    });
    let s = p.add_system();
    let e = p
        .add_emitter(
            s,
            Region {
                shape: RegionShape::Rectangle,
                distr: RegionDistr::Linear,
            },
        )
        .unwrap();
    p.stream(e, ty, 2).unwrap();
    w.particles = p.clone();
    for req in [
        RoomRequest::Next,
        RoomRequest::Restart,
        RoomRequest::Previous,
        RoomRequest::Goto(stage),
        RoomRequest::GameRestart,
    ] {
        change(&def, &rules, &mut w, &mut host, req).unwrap();
        assert_eq!(w.particles, p, "{req:?} keeps the particle systems");
    }
}

#[test]
fn game_restart_switches() {
    let o = order_game();
    let mut rules = RunnerRules::default();
    rules.room.restart_events = true;
    rules.room.globals_survive_restart = true;
    let mut host = Probe::default();
    let (mut w, r) = start(
        &o.def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: o.b,
            difficulty: 2,
        },
    );
    r.unwrap();
    w.globals.insert(VarId(3), Value::Real(Real::ONE));
    host.log.clear();
    change(&o.def, &rules, &mut w, &mut host, RoomRequest::GameRestart).unwrap();
    assert_eq!(
        host.log[..2],
        ["RoomEnd blk", "RoomEnd world"],
        "restart_events: Room End before removal"
    );
    assert_eq!(w.room, o.a, "room_first");
    assert_eq!(w.globals.get(&VarId(3)), Some(&Value::Real(Real::ONE)));
    assert_eq!(
        w.engine.run.difficulty, 2,
        "globals_survive_restart keeps the engine globals"
    );
}

#[test]
fn views_reset_on_entry() {
    let mut b = GameDefBuilder::new("views");
    let follow = probe_kind(&mut b, "follow", false);
    let r0 = b.room(room("plain", 50));
    let mut rv = room("viewed", 50);
    rv.views_enabled = true;
    rv.views[0] = ViewDef {
        visible: true,
        view: (Real::ZERO, Real::ZERO, Real::new(800.0), Real::new(608.0)),
        port: (Real::ZERO, Real::ZERO, Real::new(800.0), Real::new(608.0)),
        follow: Some(follow),
        hborder: Real::new(400.0),
        vborder: Real::new(304.0),
        hspeed: Real::new(-1.0),
        vspeed: Real::new(-1.0),
    };
    let viewed = b.room(rv);
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
    assert!(w.views.iter().all(|v| !v.enabled));
    change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(viewed)).unwrap();
    let v = &w.views[0];
    assert!(v.enabled && v.visible);
    assert_eq!(
        (v.w, v.h, v.hborder),
        (Real::new(800.0), Real::new(608.0), Real::new(400.0))
    );
    assert_eq!(v.follow, Some(Target::Kind(follow)));
    assert!(
        w.views[1].enabled && !w.views[1].visible,
        "views_enabled is per room"
    );
    // Moved by a camera, then the room is entered again: back to the file's view.
    w.views[0].x = Real::new(1600.0);
    change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(viewed)).unwrap();
    assert_eq!(w.views[0].x, Real::ZERO);
    change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(r0)).unwrap();
    assert!(w.views.iter().all(|v| !v.enabled && v.follow.is_none()));
}
