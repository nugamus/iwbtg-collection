//! spring: the launch, the djump rules, the speed param and the outside-room destroy.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, Params};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::kid::api::{self};

use crate::builtins_kid_modifiers_support::*;

#[test]
fn spring_launches_straight_up_and_can_refresh() {
    let mut g = game(Builtin::Spring, params(&[("djump", word("set"))]));
    let kid = spawn(&mut g, "kid", 100.0, 100.0);
    let sp = spawn(&mut g, "thing", 100.0, 100.0);
    {
        let m = &mut g.w.get_mut(kid).unwrap().m;
        m.set_hspeed(r(3.0)).unwrap();
        m.set_vspeed(r(5.0)).unwrap();
    }
    touch(&mut g, sp, kid);
    let m = &g.w.get(kid).unwrap().m;
    assert_eq!((m.hspeed().get(), m.vspeed().get()), (0.0, -10.0));
    assert!(djump(&g, kid));
}

#[test]
fn spring_keeps_djump_by_default() {
    let mut g = game(Builtin::Spring, Params::default());
    let kid = spawn(&mut g, "kid", 0.0, 0.0);
    let sp = spawn(&mut g, "thing", 0.0, 0.0);
    touch(&mut g, sp, kid);
    assert!(!djump(&g, kid));
    with(&mut g, |c| api::set_air_jump(c, kid, true));
    touch(&mut g, sp, kid);
    assert!(djump(&g, kid), "keep leaves an existing djump alone");
}

#[test]
fn spring_clear_takes_the_air_jump_away_and_still_launches() {
    let mut g = game(Builtin::Spring, params(&[("djump", word("clear"))]));
    let kid = spawn(&mut g, "kid", 0.0, 0.0);
    let sp = spawn(&mut g, "thing", 0.0, 0.0);
    with(&mut g, |c| api::set_air_jump(c, kid, true));
    touch(&mut g, sp, kid);
    assert!(!djump(&g, kid));
    assert_eq!(g.w.get(kid).unwrap().m.vspeed(), r(-10.0));
    let bad = params(&[("djump", word("toggle"))]);
    assert!(builtins::init_state(Builtin::Spring, &bad).is_err());
}

#[test]
fn spring_speed_is_a_param() {
    let mut g = game(Builtin::Spring, params(&[("speed", real(12.0))]));
    let kid = spawn(&mut g, "kid", 0.0, 0.0);
    let sp = spawn(&mut g, "thing", 0.0, 0.0);
    touch(&mut g, sp, kid);
    assert_eq!(g.w.get(kid).unwrap().m.vspeed(), r(-12.0));
}

#[test]
fn spring_is_destroyed_outside_the_room() {
    let mut g = game(Builtin::Spring, Params::default());
    let sp = spawn(&mut g, "thing", 0.0, 0.0);
    fire(&mut g, sp, &EventKind::OutsideRoom);
    assert!(!g.w.get(sp).is_some_and(|i| i.alive));
}
