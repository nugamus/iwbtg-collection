//! spike: the auto-rotate and shift, the visible and depth overrides and the bullet rule.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, ParamValue};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

use crate::builtins_rely_a_support::*;

#[test]
fn spike_auto_rotate_sets_angle_and_shift() {
    let g = spike_game();
    let mut w = World::new(g.room, 1);
    let spike = kind_id(&g, "spike");
    // kid/B.md autoSpikeSprite.
    for (dir, size, angle, dx, dy) in [
        ("up", 32.0, 0.0, 16.0, 16.0),
        ("down", 32.0, 180.0, 15.0, 15.0),
        ("left", 32.0, 90.0, 16.0, 15.0),
        ("right", 32.0, 270.0, 15.0, 16.0),
        ("up", 16.0, 0.0, 8.0, 8.0),
        ("down", 16.0, 180.0, 7.0, 7.0),
        ("left", 16.0, 90.0, 8.0, 7.0),
        ("right", 16.0, 270.0, 7.0, 8.0),
    ] {
        let p = params(&[
            ("dir", word(dir)),
            ("size", real(size)),
            ("auto_rotate", ParamValue::Bool(true)),
        ]);
        let id = spawn_at(&g, &mut w, spike, 100.0, 200.0, p);
        assert_eq!(pos(&w, id), (100.0 + dx, 200.0 + dy), "{dir} {size}");
        assert_eq!(w.get(id).unwrap().img.angle(), r(angle), "{dir} {size}");
    }
    // Without auto_rotate nothing moves.
    let id = spawn_at(&g, &mut w, spike, 5.0, 6.0, params(&[("dir", word("up"))]));
    assert_eq!(pos(&w, id), (5.0, 6.0));
    // auto_rotate needs dir and a known size.
    let p = params(&[("auto_rotate", ParamValue::Bool(true))]);
    assert!(builtins::init_state(Builtin::Spike, &p).is_err());
    let p = params(&[
        ("auto_rotate", ParamValue::Bool(true)),
        ("dir", word("up")),
        ("size", real(24.0)),
    ]);
    assert!(builtins::init_state(Builtin::Spike, &p).is_err());
}

#[test]
fn spike_overrides_visible_and_depth() {
    let g = spike_game();
    let mut w = World::new(g.room, 1);
    let p = params(&[("visible", ParamValue::Bool(false)), ("depth", real(7.0))]);
    let id = spawn_at(&g, &mut w, kind_id(&g, "spike"), 0.0, 0.0, p);
    assert!(!w.get(id).unwrap().visible);
    assert_eq!(w.get(id).unwrap().depth, r(7.0));
}

#[test]
fn spike_destroys_bullets_only_when_asked() {
    let g = spike_game();
    let set = |name: &str| builtins::event_set(Builtin::Spike, kind_of(&g, name));
    assert!(set("spike").collisions.is_empty());
    assert_eq!(set("bulletspike").collisions.as_slice(), &[ClassMask::KID_BULLET]);
    let mut w = World::new(g.room, 1);
    let spike = spawn(&g, &mut w, kind_id(&g, "bulletspike"), 0.0, 0.0);
    let victim = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    fire(
        &g,
        &mut w,
        spike,
        &EventKind::Collision {
            with: ClassMask::KID_BULLET,
            other: victim,
        },
    );
    assert!(!w.get(victim).unwrap().alive);
}
