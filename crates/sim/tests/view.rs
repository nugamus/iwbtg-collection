//! View follow: the recorder traces, the speed limits and the disabled or missing target.

use iwbtg_sim::collide::Target;
use iwbtg_sim::view::follow_update;
use iwbtg_sim::world::{InstId, World};

use crate::state_support::*;

// ---- views ------------------------------------------------------------------------------

/// A view 800 wide on a 3200 room, following the kid kind.
fn follow_setup(g: &G, hborder: f64, hspeed: f64) -> World {
    let mut w = World::new(g.play, 1);
    let v = &mut w.views[0];
    v.enabled = true;
    v.visible = true;
    v.w = r(800.0);
    v.h = r(608.0);
    v.hborder = r(hborder);
    v.vborder = r(32.0);
    v.hspeed = r(hspeed);
    v.vspeed = r(-1.0);
    v.follow = Some(Target::Kind(g.kid));
    w
}

fn view_x_after(g: &G, w: &mut World, kid: InstId, x: f64) -> f64 {
    with(g, w, |c| {
        c.w.get_mut(kid).unwrap().m.set_x(r(x)).unwrap();
        follow_update(c).unwrap();
    });
    w.views[0].x.get()
}

#[test]
fn view_follow_matches_the_animus_traces() {
    // Confirmed (recorder): hborder 600 acts as 400 on an 800 view; view = clamp(x - 400).
    let g = game(|_| {});
    let mut w = follow_setup(&g, 600.0, -1.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    for (x, want) in [(403.0, 3.0), (1342.0, 942.0), (100.0, 0.0), (3100.0, 2400.0)] {
        // Walk toward x so the view follows the border, as the trace does.
        let mut cur = w.get(kid).unwrap().m.x().get();
        let step = if x > cur { 1.0 } else { -1.0 };
        while (cur - x).abs() > 0.5 {
            cur += step
                * if (x - cur).abs() < 50.0 {
                    (x - cur).abs()
                } else {
                    50.0
                };
            view_x_after(&g, &mut w, kid, cur);
        }
        assert!(
            (w.views[0].x.get() - want).abs() < 1e-9,
            "x {x}: {}",
            w.views[0].x.get()
        );
    }
}

#[test]
fn view_speed_limits_and_freezes() {
    let g = game(|_| {});
    let mut w = follow_setup(&g, 32.0, 4.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    // Far right: the target jumps, the view moves 4 a tick.
    assert!((view_x_after(&g, &mut w, kid, 1000.0) - 4.0).abs() < 1e-9);
    assert!((view_x_after(&g, &mut w, kid, 1000.0) - 8.0).abs() < 1e-9);
    // Speed 0 freezes the axis.
    let mut w = follow_setup(&g, 32.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    assert!((view_x_after(&g, &mut w, kid, 1000.0)).abs() < 1e-9);
}

#[test]
fn view_follow_ignores_disabled_views_and_missing_targets() {
    let g = game(|_| {});
    let mut w = follow_setup(&g, 32.0, -1.0);
    // No kid: nothing moves, no error.
    with(&g, &mut w, |c| follow_update(c).unwrap());
    assert_eq!(w.views[0].x, r(0.0));
    let kid = spawn(&g, &mut w, g.kid, 2000.0, 0.0);
    w.views[0].enabled = false;
    with(&g, &mut w, |c| follow_update(c).unwrap());
    assert_eq!(w.views[0].x, r(0.0));
    w.views[0].enabled = true;
    w.views[0].visible = false;
    with(&g, &mut w, |c| follow_update(c).unwrap());
    assert_eq!(w.views[0].x, r(0.0));
    w.views[0].visible = true;
    with(&g, &mut w, |c| follow_update(c).unwrap());
    assert!(w.views[0].x.get() > 0.0);
    let _ = kid;
}
