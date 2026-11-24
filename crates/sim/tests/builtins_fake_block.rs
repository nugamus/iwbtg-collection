//! fake_block: solid and visible params, the touch and approach vanish, group fade, the
//! blood rule, the tile delete, and the marker that reverses a moving platform.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, KindId, ParamValue};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

use crate::builtins_trap_block_support::*;

#[test]
fn fake_block_applies_solid_and_visible() {
    let g = fake_game(&[("solid", yes()), ("visible", ParamValue::Bool(false))]);
    let mut w = World::new(g.room, 1);
    let b = spawn(&g, &mut w, kind_id(&g, "fake"), 100.0, 100.0);
    let i = w.get(b).unwrap();
    assert!(i.solid && !i.visible);
}

#[test]
fn fake_block_vanishes_on_touch() {
    let g = fake_game(&[("vanish_on", word("touch"))]);
    let mut w = World::new(g.room, 1);
    let b = spawn(&g, &mut w, kind_id(&g, "fake"), 100.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 100.0, 100.0);
    touch(&g, &mut w, b, kid, ClassMask::KID);
    assert!(!alive(&w, b));
}

#[test]
fn fake_block_vanishes_on_approach() {
    let g = fake_game(&[("vanish_on", word("approach"))]);
    let mut w = World::new(g.room, 1);
    let b = spawn(&g, &mut w, kind_id(&g, "fake"), 100.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 300.0, 100.0);
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(alive(&w, b));
    w.get_mut(kid).unwrap().m.set_x(r(132.0)).unwrap();
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(!alive(&w, b));
}

#[test]
fn fake_block_group_fades_out_after_twenty_steps() {
    let g = fake_game(&[("vanish_on", word("group_fade"))]);
    let mut w = World::new(g.room, 1);
    let k = kind_id(&g, "fake");
    let a = spawn(&g, &mut w, k, 100.0, 100.0);
    let b = spawn(&g, &mut w, k, 300.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 100.0, 100.0);
    fire(&g, &mut w, b, &EventKind::Step);
    assert_eq!(w.get(b).unwrap().img.alpha(), r(1.0), "not active yet");
    touch(&g, &mut w, a, kid, ClassMask::KID);
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(
        (w.get(b).unwrap().img.alpha().get() - 0.95).abs() < 1e-12,
        "both fade"
    );
    for _ in 0..25 {
        fire(&g, &mut w, b, &EventKind::Step);
    }
    assert!(!alive(&w, b));
}

#[test]
fn fake_block_is_solid_for_blood_when_no_kid_exists() {
    let g = fake_game(&[("vanish_on", word("never")), ("blood_solid_when_no_kid", yes())]);
    let mut w = World::new(g.room, 1);
    let b = spawn(&g, &mut w, kind_id(&g, "fake"), 100.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 300.0, 100.0);
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(!w.get(b).unwrap().solid);
    with(&g, &mut w, |c| c.destroy(kid));
    fire(&g, &mut w, b, &EventKind::Step);
    assert!(w.get(b).unwrap().solid);
}

#[test]
fn fake_block_tile_delete_is_refused() {
    let p = params(&[("vanish_on", word("tile_delete"))]);
    assert!(builtins::init_state(Builtin::FakeBlock, &p).is_err());
}

#[test]
fn a_marker_reverses_a_moving_platform() {
    let g = game(vec![kind(
        Builtin::MovingPlatform,
        "plat",
        params(&[("hspeed", real(2.0))]),
    )]);
    let mut w = World::new(g.room, 1);
    let p = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    let marker = spawn(&g, &mut w, KindId(2), 100.0, 100.0);
    touch(&g, &mut w, p, marker, ClassMask::FAKE_BLOCK);
    assert_eq!(speeds(&w, p), (-2.0, 0.0));
}
