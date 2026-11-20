//! drop_platform: the falling platform, the rise block and its two kid-speed rules.

use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

use crate::builtins_drop_flip_edge_support::*;

#[test]
fn fall_platform_starts_falling_when_stood_on() {
    let g = drop_game("fall");
    let mut w = World::new(g.room, 1);
    let p = spawn(&g, &mut w, kind_id(&g, "drop"), 100.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 300.0, 96.0);
    fire(&g, &mut w, p, &EventKind::Step);
    assert_eq!(w.get(p).unwrap().m.vspeed(), r(0.0));
    w.get_mut(kid).unwrap().m.set_x(r(110.0)).unwrap();
    fire(&g, &mut w, p, &EventKind::Step);
    assert_eq!(w.get(p).unwrap().m.vspeed(), r(2.0));
}

#[test]
fn rise_platform_keeps_rising_after_the_kid_leaves() {
    let g = drop_game("rise");
    let mut w = World::new(g.room, 1);
    let p = spawn(&g, &mut w, kind_id(&g, "drop"), 100.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 110.0, 96.0);
    fire(&g, &mut w, p, &EventKind::Step);
    assert_eq!(pos(&w, kid).1, 94.0, "the kid is lifted by the speed");
    assert_eq!(pos(&w, p).1, 98.0);
    w.get_mut(kid).unwrap().m.set_x(r(300.0)).unwrap();
    fire(&g, &mut w, p, &EventKind::Step);
    assert_eq!(pos(&w, p).1, 96.0);
}

#[test]
fn kae_rise_block_takes_the_kid_fall_speed() {
    let g = drop_game("rise_kae");
    let mut w = World::new(g.room, 1);
    let p = spawn(&g, &mut w, kind_id(&g, "drop"), 100.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 110.0, 98.0);
    w.get_mut(kid).unwrap().m.set_vspeed(r(3.0)).unwrap();
    fire(&g, &mut w, p, &EventKind::Step);
    fire(&g, &mut w, p, &EventKind::EndStep);
    assert_eq!(pos(&w, kid).1, 101.0);
    assert_eq!(pos(&w, p).1, 103.0);
}

#[test]
fn kae_rise_block_ignores_a_rising_kid() {
    let g = drop_game("rise_kae");
    let mut w = World::new(g.room, 1);
    let p = spawn(&g, &mut w, kind_id(&g, "drop"), 100.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 110.0, 98.0);
    w.get_mut(kid).unwrap().m.set_vspeed(r(-3.0)).unwrap();
    fire(&g, &mut w, p, &EventKind::Step);
    fire(&g, &mut w, p, &EventKind::EndStep);
    assert_eq!(pos(&w, p).1, 100.0);
}
