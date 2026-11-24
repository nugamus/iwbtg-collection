//! bullet_blocker: destroying bullets, and sparing them while it overlaps the kid.

use iwbtg_sim::def::{Builtin, ClassMask, ParamValue, Params};
use iwbtg_sim::world::World;

use crate::builtins_drop_flip_edge_support::*;

#[test]
fn bullet_blocker_destroys_bullets() {
    let g = game(vec![kind(Builtin::BulletBlocker, "bb", Params::default())]);
    let mut w = World::new(g.room, 1);
    let b = spawn(&g, &mut w, kind_id(&g, "bb"), 100.0, 100.0);
    let bullet = spawn(&g, &mut w, g.bullet, 100.0, 100.0);
    touch(&g, &mut w, b, bullet, ClassMask::KID_BULLET);
    assert!(!alive(&w, bullet));
}

#[test]
fn bullet_blocker_can_spare_bullets_while_it_overlaps_the_kid() {
    let p = params(&[("except_when_overlapping_kid", ParamValue::Bool(true))]);
    let g = game(vec![kind(Builtin::BulletBlocker, "bb", p)]);
    let mut w = World::new(g.room, 1);
    let b = spawn(&g, &mut w, kind_id(&g, "bb"), 100.0, 100.0);
    let bullet = spawn(&g, &mut w, g.bullet, 100.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 110.0, 110.0);
    touch(&g, &mut w, b, bullet, ClassMask::KID_BULLET);
    assert!(alive(&w, bullet));
    w.get_mut(kid).unwrap().m.set_x(r(500.0)).unwrap();
    touch(&g, &mut w, b, bullet, ClassMask::KID_BULLET);
    assert!(!alive(&w, bullet));
}
