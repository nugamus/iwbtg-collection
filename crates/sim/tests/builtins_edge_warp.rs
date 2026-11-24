//! edge_warp: the single-target and per-edge forms, and room_assign on the edge.

use iwbtg_sim::builtins::{self, BuiltinState, edge_warp::Edge};
use iwbtg_sim::def::{Builtin, ClassMask, ParamValue, Params, RoomId};
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::world::{Flow, KindState, World};

use crate::builtins_drop_flip_edge_support::*;

#[test]
fn single_target_edge_warp_keeps_a_warp_state_for_the_kid() {
    let p = params(&[
        ("room_to", room(1)),
        ("warp_x", ParamValue::Real(r(32.0))),
        ("warp_y", ParamValue::Real(r(64.0))),
    ]);
    match builtins::init_state(Builtin::EdgeWarp, &p).unwrap() {
        BuiltinState::Warp(s) => {
            assert_eq!((s.room_to, s.warp_x, s.warp_y), (RoomId(1), r(32.0), r(64.0)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn per_edge_table_answers_by_edge() {
    let p = params(&[("left_room", room(1)), ("down_room", room(2))]);
    match builtins::init_state(Builtin::EdgeWarp, &p).unwrap() {
        BuiltinState::EdgeTable(t) => {
            assert_eq!(t.target(Edge::Left), Some(RoomId(1)));
            assert_eq!(t.target(Edge::Down), Some(RoomId(2)));
            assert_eq!(t.target(Edge::Up), None);
            assert_eq!(t.target(Edge::Right), None);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn edge_warp_needs_exactly_one_form() {
    assert!(builtins::init_state(Builtin::EdgeWarp, &Params::default()).is_err());
    let both = params(&[("room_to", room(1)), ("up_room", room(2))]);
    assert!(builtins::init_state(Builtin::EdgeWarp, &both).is_err());
}

#[test]
fn room_assign_queues_the_room_and_moves_the_kid() {
    let p = params(&[
        ("mode", word("room_assign")),
        ("room_to", room(1)),
        ("warp_x", ParamValue::Real(r(8.0))),
    ]);
    let g = game(vec![kind(Builtin::EdgeWarp, "sw", p)]);
    let mut w = World::new(g.room, 1);
    let s = spawn(&g, &mut w, kind_id(&g, "sw"), 800.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 797.0, 503.4);
    let flow = touch(&g, &mut w, s, kid, ClassMask::KID);
    assert_eq!(flow, Flow::AbortTick);
    assert_eq!(w.pending, Some(RoomRequest::Goto(RoomId(1))));
    assert_eq!(pos(&w, kid), (8.0, 503.4), "y is kept when warp_y is 0");
    assert!(matches!(
        w.get(s).map(|i| &i.state),
        Some(KindState::Builtin(BuiltinState::RoomAssign(_)))
    ));
}

#[test]
fn room_assign_without_a_target_does_nothing() {
    let g = game(vec![kind(
        Builtin::EdgeWarp,
        "sw",
        params(&[("mode", word("room_assign"))]),
    )]);
    let mut w = World::new(g.room, 1);
    let s = spawn(&g, &mut w, kind_id(&g, "sw"), 800.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 797.0, 503.0);
    assert_eq!(touch(&g, &mut w, s, kid, ClassMask::KID), Flow::Continue);
    assert_eq!(w.pending, None);
}
