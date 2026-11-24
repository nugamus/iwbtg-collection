//! hidden_block: appearing near the kid, stopping bullets, and xia's shot and blood
//! rules.

use iwbtg_sim::def::{ClassMask, KindId, ParamValue};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

use crate::builtins_trap_block_support::*;

#[test]
fn hidden_block_starts_invisible_and_shows_near_the_kid() {
    let g = hidden_game(&[]);
    let mut w = World::new(g.room, 1);
    let b = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
    assert!(!w.get(b).unwrap().visible);
    // Kid box 4 wide at x 132: next to the block (right edge 131).
    let kid = spawn(&g, &mut w, g.kid, 200.0, 100.0);
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(!w.get(b).unwrap().visible, "far away");
    w.get_mut(kid).unwrap().m.set_x(r(132.0)).unwrap();
    w.get_mut(kid).unwrap().m.set_y(r(100.0)).unwrap();
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(w.get(b).unwrap().visible);
}

#[test]
fn hidden_block_without_a_kid_destroys_itself() {
    let g = hidden_game(&[]);
    let mut w = World::new(g.room, 1);
    let b = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(!alive(&w, b));
    let g2 = hidden_game(&[("destroy_when_no_kid", ParamValue::Bool(false))]);
    let mut w2 = World::new(g2.room, 1);
    let b2 = spawn(&g2, &mut w2, kind_id(&g2, "hid"), 100.0, 100.0);
    fire(&g2, &mut w2, b2, &EventKind::Step);
    assert!(alive(&w2, b2));
}

#[test]
fn hidden_block_can_stop_bullets() {
    let g = hidden_game(&[("stops_bullets", yes())]);
    let mut w = World::new(g.room, 1);
    let b = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
    let bullet = spawn(&g, &mut w, KindId(1), 100.0, 100.0);
    touch(&g, &mut w, b, bullet, ClassMask::KID_BULLET);
    assert!(!alive(&w, bullet));
}

#[test]
fn xia_hidden_block_is_destroyed_by_a_shot_and_by_blood_only_while_invisible() {
    let g = hidden_game(&[("destroyed_by_bullet", yes()), ("destroyed_by_blood", yes())]);
    let mut w = World::new(g.room, 1);
    let bullet = spawn(&g, &mut w, KindId(1), 100.0, 100.0);
    let shot = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
    touch(&g, &mut w, shot, bullet, ClassMask::KID_BULLET);
    assert!(!alive(&w, shot));
    assert!(alive(&w, bullet), "stops_bullets is its own param");
    let bled = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
    touch(&g, &mut w, bled, bullet, ClassMask::BLOOD);
    assert!(!alive(&w, bled), "blood destroys it while invisible");
    let shown = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
    w.get_mut(shown).unwrap().visible = true;
    touch(&g, &mut w, shown, bullet, ClassMask::BLOOD);
    assert!(alive(&w, shown), "a revealed block ignores blood");
    let plain = hidden_game(&[]);
    let mut w2 = World::new(plain.room, 1);
    let b = spawn(&plain, &mut w2, kind_id(&plain, "hid"), 100.0, 100.0);
    let bullet2 = spawn(&plain, &mut w2, KindId(1), 100.0, 100.0);
    touch(&plain, &mut w2, b, bullet2, ClassMask::KID_BULLET);
    assert!(alive(&w2, b), "without the params a shot does nothing");
}
