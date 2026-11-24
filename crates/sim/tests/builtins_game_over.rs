//! game_over: placement relative to view 0, the alarm delay and the fade.

/// The engine hooks without the sound model (WP10) and timing triggers (WP8b), which are
/// not implemented yet.
use crate::builtins_rely_c_support::*;
use iwbtg_sim::builtins::{self, game_over};
use iwbtg_sim::def::{Builtin, ParamValue, Params};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

#[test]
fn game_over_is_placed_relative_to_view_zero() {
    let g = over_game(Params::default());
    let mut w = World::new(g.room, 1);
    w.views[0].x = r(800.0);
    w.views[0].y = r(608.0);
    let id = spawn(&g, &mut w, kind_id(&g, "over"), 5.0, 5.0);
    let i = w.get(id).unwrap();
    assert_eq!((i.m.x(), i.m.y()), (r(1200.0), r(912.0)), "view + (400, 304)");
    assert_eq!(i.img.alpha(), r(0.0));
    assert!(i.visible, "no delay: visible at once");
    let g = over_game(params(&[("offset_x", real(480.0)), ("offset_y", real(270.0))]));
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "over"), 5.0, 5.0);
    assert_eq!(
        (w.get(id).unwrap().m.x(), w.get(id).unwrap().m.y()),
        (r(480.0), r(270.0))
    );
    let g = over_game(params(&[("view_relative", ParamValue::Bool(false))]));
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "over"), 5.0, 6.0);
    assert_eq!(
        (w.get(id).unwrap().m.x(), w.get(id).unwrap().m.y()),
        (r(5.0), r(6.0))
    );
}

#[test]
fn game_over_delay_hides_it_until_alarm_zero() {
    let g = over_game(params(&[("delay", real(30.0))]));
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "over"), 0.0, 0.0);
    assert!(!w.get(id).unwrap().visible);
    assert_eq!(w.get(id).unwrap().alarms[0], r(30.0));
    fire(&g, &mut w, id, &EventKind::Alarm(0));
    assert!(w.get(id).unwrap().visible);
}

#[test]
fn game_over_fades_in_the_draw_slot() {
    let g = over_game(Params::default());
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "over"), 0.0, 0.0);
    for _ in 0..10 {
        fire(&g, &mut w, id, &EventKind::Late);
    }
    let a = w.get(id).unwrap().img.alpha().get();
    assert!((a - 0.1).abs() < 1e-12, "alpha rises 0.01 a tick: {a}");
    assert!(
        (game_over::overlay_alpha(&w, id).get() - 0.15).abs() < 1e-12,
        "overlay 0.015 a tick"
    );
    for _ in 0..200 {
        fire(&g, &mut w, id, &EventKind::Late);
    }
    assert_eq!(w.get(id).unwrap().img.alpha(), r(1.0));
    assert_eq!(
        game_over::overlay_alpha(&w, id),
        r(0.600_000_000_000_000_1),
        "the overlay stops at the GML literal 0.6, read as 0.6000000000000001 (N14)"
    );
    // The bob is 32 * sin(phase), phase += pi/100: 210 ticks is 2.1 pi.
    let want = 32.0 * iwbtg_sim::num::sin(r(210.0 * std::f64::consts::PI / 100.0)).get();
    assert!((game_over::bob_offset(&w, id).get() - want).abs() < 1e-9);
    let set = builtins::event_set(
        Builtin::GameOver,
        &g.def.kinds[usize::try_from(kind_id(&g, "over").0).unwrap()],
    );
    assert!(set.late);
}
