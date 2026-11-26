//! camera: screen snap and its offsets, the End Step clamp and the smooth follow.

use iwbtg_sim::builtins;
use iwbtg_sim::collide::Target;
use iwbtg_sim::def::{Builtin, ParamValue, Params};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::world::World;

use crate::builtins_tier1_support::*;

#[test]
fn screen_snap_puts_view_zero_on_the_screen_holding_the_kid() {
    let g = game(Builtin::Camera, Params::default());
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.kid, 1650.0, 700.0);
    let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
    fire(&g, &mut w, cam, &EventKind::Step);
    assert_eq!(
        view(&w),
        (1600.0, 608.0),
        "floor(1650/800)*800, floor(700/608)*608"
    );
    w.get_mut(w.kid(&RunnerRules::default().tick).unwrap())
        .unwrap()
        .m
        .set_x(r(799.0))
        .unwrap();
    fire(&g, &mut w, cam, &EventKind::Step);
    assert_eq!(view(&w).0, 0.0);
}

#[test]
fn screen_snap_y_offset_screen_size_and_events() {
    // machinary: +4, poronkusema: 960 x 540.
    let g = game(
        Builtin::Camera,
        params(&[
            ("y_offset", real(4.0)),
            ("screen_w", real(960.0)),
            ("screen_h", real(540.0)),
        ]),
    );
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.kid, 1000.0, 600.0);
    let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
    fire(&g, &mut w, cam, &EventKind::Step);
    assert_eq!(view(&w), (960.0, 544.0));
    // Room Start and Room End only snap when the game says so.
    let g = game(Builtin::Camera, Params::default());
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.kid, 900.0, 10.0);
    let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
    fire(&g, &mut w, cam, &EventKind::RoomStart);
    assert_eq!(view(&w), (0.0, 0.0));
    let e = kind_events(&g);
    assert!(e.step && !e.room_start && !e.room_end);
    let g = game(
        Builtin::Camera,
        params(&[
            ("on_room_start", ParamValue::Bool(true)),
            ("on_room_end", ParamValue::Bool(true)),
        ]),
    );
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.kid, 900.0, 10.0);
    let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
    fire(&g, &mut w, cam, &EventKind::RoomStart);
    assert_eq!(view(&w), (800.0, 0.0));
    w.views[0].x = r(0.0);
    fire(&g, &mut w, cam, &EventKind::RoomEnd);
    assert_eq!(view(&w), (800.0, 0.0));
    let e = kind_events(&g);
    assert!(e.step && e.room_start && e.room_end);
}

#[test]
fn screen_snap_follows_the_flipped_kid_and_waits_for_a_kid() {
    let g = game(Builtin::Camera, Params::default());
    let mut w = World::new(g.room, 1);
    let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
    w.views[0].x = r(1600.0);
    fire(&g, &mut w, cam, &EventKind::Step);
    assert_eq!(view(&w).0, 1600.0, "no kid: the view stays");
    spawn(&g, &mut w, g.flipped, 850.0, 20.0);
    fire(&g, &mut w, cam, &EventKind::Step);
    assert_eq!(view(&w).0, 800.0);
}

#[test]
fn end_step_clamp_uses_the_views_size_and_clamps_the_point() {
    let g = game(Builtin::Camera, params(&[("mode", word("end_step_clamp"))]));
    let mut w = World::new(g.room, 1);
    w.views[0].w = r(800.0);
    w.views[0].h = r(608.0);
    let kid = spawn(&g, &mut w, g.kid, 900.0, 700.0);
    let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
    assert_eq!(view(&w), (800.0, 608.0), "Create snaps too");
    // Past the room's edge: clamped to 3199, 1215 before the split.
    w.get_mut(kid).unwrap().m.set_x(r(4000.0)).unwrap();
    w.get_mut(kid).unwrap().m.set_y(r(-50.0)).unwrap();
    fire(&g, &mut w, cam, &EventKind::EndStep);
    assert_eq!(view(&w), (2400.0, 0.0), "3199 / 800 floors to 3");
    let e = kind_events(&g);
    assert!(e.end_step && !e.step);
}

#[test]
fn smooth_follow_create_becomes_the_followed_object_and_jumps_to_the_kid() {
    let g = game(Builtin::Camera, params(&[("mode", word("smooth_follow"))]));
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.kid, 500.0, 300.0);
    let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
    assert_eq!(w.views[0].follow, Some(Target::Inst(cam)));
    assert_eq!((w.views[0].hborder, w.views[0].vborder), (r(400.0), r(304.0)));
    let i = w.get(cam).unwrap();
    assert_eq!((i.m.x(), i.m.y()), (r(500.0), r(300.0)));
}

#[test]
fn smooth_follow_speeds_are_a_fraction_of_the_distance() {
    let g = game(Builtin::Camera, params(&[("mode", word("smooth_follow"))]));
    let mut w = World::new(g.room, 1);
    let kid = spawn(&g, &mut w, g.kid, 500.0, 300.0);
    let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
    w.get_mut(kid).unwrap().m.set_x(r(596.0)).unwrap();
    w.get_mut(kid).unwrap().m.set_y(r(336.0)).unwrap();
    fire(&g, &mut w, cam, &EventKind::Step);
    let i = w.get(cam).unwrap();
    assert_eq!(
        (i.m.hspeed(), i.m.vspeed()),
        (r(6.0), r(3.0)),
        "96 / 16 and 36 / 12"
    );
    // Within 2 px (inclusive) an axis stops; the other keeps going.
    w.get_mut(kid).unwrap().m.set_x(r(502.0)).unwrap();
    fire(&g, &mut w, cam, &EventKind::Step);
    let i = w.get(cam).unwrap();
    assert_eq!((i.m.hspeed(), i.m.vspeed()), (r(0.0), r(3.0)));
    w.get_mut(kid).unwrap().m.set_x(r(502.5)).unwrap();
    fire(&g, &mut w, cam, &EventKind::Step);
    assert!(w.get(cam).unwrap().m.hspeed().get() > 0.0, "2.5 px is outside");
    // No kid: both speeds are 0.
    w.get_mut(kid).unwrap().alive = false;
    fire(&g, &mut w, cam, &EventKind::Step);
    let i = w.get(cam).unwrap();
    assert_eq!((i.m.hspeed(), i.m.vspeed()), (r(0.0), r(0.0)));
}

#[test]
fn camera_params_are_checked() {
    assert!(builtins::init_state(Builtin::Camera, &params(&[("mode", word("zoom"))])).is_err());
    assert!(builtins::init_state(Builtin::Camera, &params(&[("screen_w", real(0.0))])).is_err());
    assert!(builtins::init_state(Builtin::Camera, &params(&[("h_div", real(0.0))])).is_err());
}
