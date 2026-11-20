//! conveyor: the h add on every collision tick, the top-edge rule and the param check.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, Params};
use iwbtg_sim::events::EventKind;

use crate::builtins_kid_modifiers_support::*;

#[test]
fn conveyor_adds_h_on_every_collision_tick() {
    let mut g = game(Builtin::Conveyor, params(&[("h", real(-2.0))]));
    let kid = spawn(&mut g, "kid", 100.0, 100.0);
    let cv = spawn(&mut g, "thing", 100.0, 108.0);
    g.w.get_mut(kid).unwrap().m.set_hspeed(r(1.0)).unwrap();
    touch(&mut g, cv, kid);
    touch(&mut g, cv, kid);
    assert_eq!(g.w.get(kid).unwrap().m.hspeed(), r(-3.0));
}

#[test]
fn conveyor_top_edge_sets_the_speed_only_on_the_top_line() {
    let p = params(&[("h", real(4.0)), ("mode", word("set_on_top_edge"))]);
    let mut g = game(Builtin::Conveyor, p);
    let k = &g.def.kinds[usize::try_from(kind(&g, "thing").0).unwrap()];
    assert!(k.events.step && k.events.collisions.is_empty());
    // The conveyor top edge is y = 200; the kid (8 x 8) stands on it.
    let cv = spawn(&mut g, "thing", 100.0, 200.0);
    let on = spawn(&mut g, "kid", 110.0, 193.0);
    fire(&mut g, cv, &EventKind::Step);
    assert_eq!(g.w.get(on).unwrap().m.hspeed(), r(4.0));
    // A kid far above is untouched.
    g.w.get_mut(on).unwrap().m.set_hspeed(r(0.0)).unwrap();
    g.w.get_mut(on).unwrap().m.set_y(r(100.0)).unwrap();
    fire(&mut g, cv, &EventKind::Step);
    assert_eq!(g.w.get(on).unwrap().m.hspeed(), r(0.0));
}

#[test]
fn conveyor_needs_h() {
    assert!(builtins::init_state(Builtin::Conveyor, &Params::default()).is_err());
}
