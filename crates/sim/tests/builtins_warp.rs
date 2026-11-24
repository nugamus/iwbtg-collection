//! warp: the target, zero-axis, screen-shift, clear and gate rules, and the transition.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, KindDef, ParamValue, Params, RoomId};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::room::{RoomOutput, RoomRequest};
use iwbtg_sim::world::{Flow, World};

use crate::builtins_rely_b_support::*;

#[test]
fn warp_to_zero_zero_destroys_the_kid_and_the_bow_and_requests_the_room() {
    let g = warp_game(|_| Params::default());
    let mut w = World::new(g.room, 1);
    let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
    spawn(&g, &mut w, g.bow, 5.0, 5.0);
    let flow = fire(&g, &mut w, warp, &touch(kid));
    assert_eq!(flow, Flow::AbortTick);
    assert_eq!(w.pending, Some(RoomRequest::Goto(g.room2)));
    assert!(alive(&w, g.kid).is_empty() && alive(&w, g.bow).is_empty());
}

#[test]
fn warp_with_a_target_moves_the_kid_and_keeps_his_speed() {
    let g = warp_game(|_| params(&[("warp_x", real(40.0)), ("warp_y", real(80.0))]));
    let mut w = World::new(g.room, 1);
    let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
    w.get_mut(kid).unwrap().m.set_hspeed(r(3.0)).unwrap();
    fire(&g, &mut w, warp, &touch(kid));
    let k = w.get(kid).unwrap();
    assert!(k.alive);
    assert_eq!((k.m.x(), k.m.y(), k.m.hspeed()), (r(40.0), r(80.0), r(3.0)));
    assert_eq!(w.pending, Some(RoomRequest::Goto(g.room2)));
}

#[test]
fn warp_zero_axis_set_writes_a_zero_target_too() {
    // The default (LR, KOT, ANI roomChanger): x = warpX and y = warpY, a 0 included.
    let g = warp_game(|_| params(&[("warp_x", real(40.0))]));
    let (w, kid) = warp_a_moving_kid(&g, 5.0, 70.0, -4.0);
    assert_eq!(pos_speed(&w, kid), (r(40.0), r(0.0), r(3.0), r(-4.0)));
}

#[test]
fn warp_zero_axis_keep_leaves_a_zero_axis_and_keeps_speed_and_djump() {
    // NY roomChanger (KID-A4): x only if XX is nonzero, y only if YY is nonzero; the kid
    // keeps his speed and djump.
    let g = warp_game(|_| params(&[("warp_x", real(40.0)), ("zero_axis", word("keep"))]));
    let (w, kid) = warp_a_moving_kid(&g, 5.0, 70.0, -4.0);
    assert_eq!(pos_speed(&w, kid), (r(40.0), r(70.0), r(3.0), r(-4.0)));
    assert!(!djump(&w, kid), "a spent djump stays spent");
    let g = warp_game(|_| params(&[("warp_y", real(90.0)), ("zero_axis", word("keep"))]));
    let (w, kid) = warp_a_moving_kid(&g, 5.0, 70.0, 2.0);
    assert_eq!(pos_speed(&w, kid), (r(5.0), r(90.0), r(3.0), r(2.0)));
}

#[test]
fn warp_screen_shift_moves_a_zero_axis_by_screens() {
    // KAE roomChanger (KID-B): x = warpX, or x + screens * 800 when warpX is 0; the same
    // for y with 608.
    let g = warp_game(|_| {
        params(&[
            ("warp_y", real(90.0)),
            ("zero_axis", word("keep")),
            ("screen_shift", real(-1.0)),
        ])
    });
    let (w, kid) = warp_a_moving_kid(&g, 805.0, 70.0, 2.0);
    assert_eq!(pos_speed(&w, kid), (r(5.0), r(90.0), r(3.0), r(2.0)));
    let g = warp_game(|_| {
        params(&[
            ("warp_x", real(40.0)),
            ("zero_axis", word("keep")),
            ("screen_shift", real(1.0)),
        ])
    });
    let (w, kid) = warp_a_moving_kid(&g, 5.0, 70.0, 2.0);
    assert_eq!(pos_speed(&w, kid), (r(40.0), r(678.0), r(3.0), r(2.0)));
}

#[test]
fn warp_clear_rising_zeroes_only_a_negative_vspeed() {
    // KAE (KID-B): a negative vspeed becomes 0, not adjusted for reversed gravity.
    let p = |_| {
        params(&[
            ("warp_x", real(40.0)),
            ("warp_y", real(90.0)),
            ("clear_rising", yes()),
        ])
    };
    let g = warp_game(p);
    let (w, kid) = warp_a_moving_kid(&g, 5.0, 70.0, -4.0);
    assert_eq!(pos_speed(&w, kid), (r(40.0), r(90.0), r(3.0), r(0.0)));
    let g = warp_game(p);
    let (w, kid) = warp_a_moving_kid(&g, 5.0, 70.0, 4.0);
    assert_eq!(pos_speed(&w, kid), (r(40.0), r(90.0), r(3.0), r(4.0)));
}

#[test]
fn warp_to_zero_zero_destroys_the_kid_whatever_the_move_rule() {
    let g = warp_game(|_| {
        params(&[
            ("zero_axis", word("keep")),
            ("screen_shift", real(1.0)),
            ("clear_rising", yes()),
        ])
    });
    let mut w = World::new(g.room, 1);
    let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
    fire(&g, &mut w, warp, &touch(kid));
    assert!(alive(&w, g.kid).is_empty());
}

#[test]
fn warp_boss_gate_opens_at_the_step_after_the_flag() {
    // LR warpToBoss: once global.boss[bossNumber] is set, Step turns on onCollide and
    // visible. A single number is a list of one.
    let (g, mut w, warp, kid) = gated(vec![("gate_bosses", real(3.0))]);
    assert!(!w.get(warp).unwrap().visible, "starts closed and hidden");
    fire(&g, &mut w, warp, &EventKind::Step);
    assert_eq!(fire(&g, &mut w, warp, &touch(kid)), Flow::Continue);
    assert_eq!(w.pending, None);
    w.engine.mech.set_boss(3);
    assert_eq!(
        fire(&g, &mut w, warp, &touch(kid)),
        Flow::Continue,
        "a flag set this tick opens the gate at the next Step"
    );
    fire(&g, &mut w, warp, &EventKind::Step);
    assert!(w.get(warp).unwrap().visible);
    assert_eq!(fire(&g, &mut w, warp, &touch(kid)), Flow::AbortTick);
    assert_eq!(w.pending, Some(RoomRequest::Goto(g.room2)));
}

#[test]
fn warp_count_gate_counts_listed_bosses_and_items() {
    // KAE warpCount: countBoss and countItem, open (visible, usable) while the count reaches
    // total.
    let all = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
    let (g, mut w, warp, kid) = gated(vec![
        ("gate_bosses", list(&all)),
        ("gate_items", list(&all)),
        ("gate_need", real(3.0)),
    ]);
    w.engine.mech.set_boss(0);
    w.engine.mech.set_boss(5);
    w.engine.mech.set_boss(9); // not listed
    fire(&g, &mut w, warp, &EventKind::Step);
    assert!(!w.get(warp).unwrap().visible, "2 of 3");
    assert_eq!(fire(&g, &mut w, warp, &touch(kid)), Flow::Continue);
    w.engine.mech.set_item(7);
    fire(&g, &mut w, warp, &EventKind::Step);
    assert!(w.get(warp).unwrap().visible, "3 of 3");
    assert_eq!(fire(&g, &mut w, warp, &touch(kid)), Flow::AbortTick);
}

#[test]
fn warp_destroy_gate_destroys_itself_unless_both_flags_are_set() {
    // XIA objWarpfin: destroys itself in Step unless boss items 1 and 2 are both taken.
    let p = || {
        vec![
            ("gate_bosses", list(&[1.0, 2.0])),
            ("gate_closed", word("destroy")),
            ("visible", yes()),
        ]
    };
    let (g, mut w, warp, _) = gated(p());
    assert!(
        w.get(warp).unwrap().visible,
        "a destroying gate keeps its own visible"
    );
    w.engine.mech.set_boss(1);
    fire(&g, &mut w, warp, &EventKind::Step);
    assert!(!w.get(warp).is_some_and(|i| i.alive));
    let (g, mut w, warp, kid) = gated(p());
    w.engine.mech.set_boss(1);
    w.engine.mech.set_boss(2);
    assert_eq!(fire(&g, &mut w, warp, &EventKind::Step), Flow::Continue);
    assert!(w.get(warp).unwrap().alive);
    assert_eq!(fire(&g, &mut w, warp, &touch(kid)), Flow::AbortTick);
}

#[test]
fn warp_gate_params_are_checked_and_add_step() {
    let room = ("room_to", ParamValue::Room(RoomId(1)));
    let bad: Vec<Vec<(&str, ParamValue)>> = vec![
        vec![("gate_bosses", list(&[16.0]))],
        vec![("gate_bosses", list(&[1.0, 1.0]))],
        vec![("gate_items", list(&[0.5]))],
        vec![("gate_bosses", list(&[1.0])), ("gate_need", real(2.0))],
        vec![("gate_need", real(1.0))],
        vec![("gate_closed", word("destroy"))],
        vec![("gate_bosses", list(&[1.0])), ("visible", yes())],
        vec![("gate_bosses", list(&[1.0])), ("gate_closed", word("vanish"))],
    ];
    for b in bad {
        let mut p = params(&b);
        p.0.insert(room.0.to_owned(), room.1.clone());
        assert!(builtins::init_state(Builtin::Warp, &p).is_err(), "{b:?}");
    }
    let ok = params(&[
        room.clone(),
        ("gate_items", list(&[0.0, 15.0])),
        ("gate_need", real(0.0)),
    ]);
    assert!(builtins::init_state(Builtin::Warp, &ok).is_ok());
    let mut kind = KindDef::new("w", Builtin::Warp);
    assert!(!builtins::event_set(Builtin::Warp, &kind).step);
    kind.params = params(&[("gate_items", list(&[2.0]))]);
    assert!(builtins::event_set(Builtin::Warp, &kind).step);
    assert!(builtins::event_set_params(Builtin::Warp).contains(&"gate_bosses"));
}

#[test]
fn warp_with_on_collide_off_does_nothing_and_a_flipped_kid_also_triggers_it() {
    let g = warp_game(|_| params(&[("on_collide", ParamValue::Bool(false))]));
    let mut w = World::new(g.room, 1);
    let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
    assert_eq!(fire(&g, &mut w, warp, &touch(kid)), Flow::Continue);
    assert_eq!(w.pending, None);
    let g = warp_game(|_| Params::default());
    let mut w = World::new(g.room, 1);
    let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.flipped, 5.0, 5.0);
    let ev = EventKind::Collision {
        with: ClassMask::KID_FLIPPED,
        other: kid,
    };
    assert_eq!(fire(&g, &mut w, warp, &ev), Flow::AbortTick);
    // The handler is declared for both kid classes.
    let set = builtins::event_set(
        Builtin::Warp,
        &g.def.kinds[usize::try_from(kind_id(&g, "warp").0).unwrap()],
    );
    assert!(set.collisions[0].contains(ClassMask::KID) && set.collisions[0].contains(ClassMask::KID_FLIPPED));
}

#[test]
fn warp_transition_goes_to_the_client() {
    let g = warp_game(|_| params(&[("transition", real(21.0)), ("transition_steps", real(30.0))]));
    let mut w = World::new(g.room, 1);
    let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
    fire(&g, &mut w, warp, &touch(kid));
    assert!(
        w.output
            .contains(&OutputEvent::Room(RoomOutput::Transition { kind: 21, steps: 30 }))
    );
}

#[test]
fn warp_params_are_checked() {
    assert!(
        builtins::init_state(Builtin::Warp, &Params::default()).is_err(),
        "room_to is required"
    );
    let p = params(&[
        ("room_to", ParamValue::Room(RoomId(1))),
        ("transition", real(-1.0)),
    ]);
    assert!(builtins::init_state(Builtin::Warp, &p).is_err());
    let p = params(&[
        ("room_to", ParamValue::Room(RoomId(1))),
        ("transition", real(300.0)),
    ]);
    assert!(builtins::init_state(Builtin::Warp, &p).is_err());
    let room = ("room_to", ParamValue::Room(RoomId(1)));
    let p = params(&[room.clone(), ("screen_shift", real(1.0))]);
    assert!(
        builtins::init_state(Builtin::Warp, &p).is_err(),
        "screen_shift needs zero_axis = keep"
    );
    let p = params(&[room.clone(), ("zero_axis", word("both"))]);
    assert!(builtins::init_state(Builtin::Warp, &p).is_err());
    let p = params(&[room, ("zero_axis", word("set")), ("screen_shift", real(0.0))]);
    assert!(builtins::init_state(Builtin::Warp, &p).is_ok());
}
