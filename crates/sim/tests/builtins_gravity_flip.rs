//! gravity_flip: the up/down flags, the outside-room destroy and kaenbyou's arrow.

use iwbtg_sim::builtins::{self};
use iwbtg_sim::def::{Builtin, ClassMask, Params};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

use crate::builtins_drop_flip_edge_support::*;

#[test]
fn gravity_up_sets_the_flag_for_the_normal_kid_only() {
    let g = flip_game("up");
    let mut w = World::new(g.room, 1);
    let f = spawn(&g, &mut w, kind_id(&g, "flip"), 0.0, 0.0);
    let flipped = spawn(&g, &mut w, g.flipped, 0.0, 0.0);
    touch(&g, &mut w, f, flipped, ClassMask::KID_FLIPPED);
    assert!(!w.engine.kid.grav);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    touch(&g, &mut w, f, kid, ClassMask::KID);
    assert!(w.engine.kid.grav);
}

#[test]
fn gravity_down_clears_the_flag_for_the_flipped_kid_only() {
    let g = flip_game("down");
    let mut w = World::new(g.room, 1);
    w.engine.kid.grav = true;
    let f = spawn(&g, &mut w, kind_id(&g, "flip"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    touch(&g, &mut w, f, kid, ClassMask::KID);
    assert!(w.engine.kid.grav);
    let flipped = spawn(&g, &mut w, g.flipped, 0.0, 0.0);
    touch(&g, &mut w, f, flipped, ClassMask::KID_FLIPPED);
    assert!(!w.engine.kid.grav);
}

#[test]
fn gravity_flip_is_destroyed_outside_the_room() {
    let g = flip_game("up");
    let mut w = World::new(g.room, 1);
    let f = spawn(&g, &mut w, kind_id(&g, "flip"), 0.0, 0.0);
    fire(&g, &mut w, f, &EventKind::OutsideRoom);
    assert!(!alive(&w, f));
    assert!(builtins::init_state(Builtin::GravityFlip, &Params::default()).is_err());
}

#[test]
fn kaenbyou_arrow_rotates_the_view_then_flips_the_kid() {
    let g = game(vec![kind(
        Builtin::GravityFlip,
        "flip",
        params(&[
            ("direction", word("up")),
            ("transition", word("yflag_with_view_rotation")),
        ]),
    )]);
    let mut w = World::new(g.room, 1);
    let f = spawn(&g, &mut w, kind_id(&g, "flip"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    touch(&g, &mut w, f, kid, ClassMask::KID);
    assert!(w.engine.kid.frozen2);
    assert_eq!(w.get(f).unwrap().alarms[0].get(), 1.0);
    for n in 1..=15 {
        fire(&g, &mut w, f, &EventKind::Alarm(0));
        assert_eq!(w.views[0].angle.get(), 12.0 * f64::from(n));
    }
    // The next Alarm 0 is past the end and arms Alarm 1.
    fire(&g, &mut w, f, &EventKind::Alarm(0));
    assert_eq!(w.get(f).unwrap().alarms[1].get(), 1.0);
    fire(&g, &mut w, f, &EventKind::Alarm(1));
    assert!(!w.engine.kid.frozen2);
}
