//! killer: the cherry settings, the outside-room destroy and the kill outside the view.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, ParamValue, Params};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

use crate::builtins_tier1_support::*;

#[test]
fn killer_create_applies_the_cherry_settings() {
    let g = game(
        Builtin::Killer,
        params(&[
            ("image_speed", real(1.0 / 15.0)),
            ("scale", real(9.0)),
            ("vspeed", real(8.0)),
            ("visible", ParamValue::Bool(false)),
        ]),
    );
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, g.thing, 100.0, 100.0);
    let i = w.get(id).unwrap();
    assert_eq!(i.img.speed(), r(1.0 / 15.0));
    assert_eq!((i.img.xscale(), i.img.yscale()), (r(9.0), r(9.0)));
    assert_eq!((i.m.hspeed(), i.m.vspeed()), (r(0.0), r(8.0)));
    assert!(!i.visible);
    // A plain killer changes nothing.
    let g = game(Builtin::Killer, Params::default());
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, g.thing, 5.0, 6.0);
    let i = w.get(id).unwrap();
    assert_eq!(
        (i.img.speed(), i.img.xscale(), i.m.speed()),
        (r(1.0), r(1.0), r(0.0))
    );
}

#[test]
fn killer_outside_room_destroys_only_when_asked() {
    let g = game(
        Builtin::Killer,
        params(&[("destroy_outside_room", ParamValue::Bool(true))]),
    );
    assert!(kind_events(&g).outside_room);
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, g.thing, 0.0, 0.0);
    fire(&g, &mut w, id, &EventKind::OutsideRoom);
    assert!(!w.get(id).unwrap().alive);
    let g = game(Builtin::Killer, Params::default());
    assert!(!kind_events(&g).outside_room);
}

/// `kill_outside_view` (kaenbyou borderKiller): no `killer` class, a kid collision event, and
/// the kid dies only when its origin is outside view 0 (each side strict).
#[test]
fn killer_kill_outside_view_kills_only_outside_the_view() {
    let g = game(
        Builtin::Killer,
        params(&[("kill_outside_view", ParamValue::Bool(true))]),
    );
    assert_eq!(kind_events(&g).collisions.len(), 1);
    let touch = |g: &G, kx: f64, ky: f64| {
        let mut w = World::new(g.room, 1);
        w.views[0].x = r(100.0);
        w.views[0].y = r(50.0);
        w.views[0].w = r(800.0);
        w.views[0].h = r(608.0);
        let kid = spawn(g, &mut w, g.kid, kx, ky);
        let id = spawn(g, &mut w, g.thing, 0.0, 0.0);
        let kids = ClassMask::KID;
        // The stand-in kid is inert, so reaching the kill is an invariant error ("not a kid").
        let ev = EventKind::Collision {
            with: kids,
            other: kid,
        };
        with(g, &mut w, |c| builtins::on_event(c, id, &ev)).is_err()
    };
    assert!(!touch(&g, 100.0, 50.0), "on the corner: inside");
    assert!(!touch(&g, 900.0, 658.0), "on the far corner: inside");
    assert!(touch(&g, 99.0, 300.0), "left of the view");
    assert!(touch(&g, 901.0, 300.0), "right of the view");
    assert!(touch(&g, 300.0, 49.0), "above the view");
    assert!(touch(&g, 300.0, 659.0), "below the view");
}
