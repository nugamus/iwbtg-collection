//! moving_platform: its speeds, the solid bounce and carry rules, the touch wait,
//! the marker and the family B two-line form.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{ClassMask, Params};
use iwbtg_sim::world::World;

use crate::builtins_playerface_water_support::*;

#[test]
fn platform_moves_at_its_start_speed() {
    let g = plat_game(params(&[("hspeed", real(2.0))]));
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    step(&g, &mut w, id);
    step(&g, &mut w, id);
    assert_eq!(pos(&w, id), (104.0, 100.0));
}

#[test]
fn platform_reverses_at_a_solid() {
    let g = plat_game(params(&[("hspeed", real(2.0))]));
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.block, 133.0, 100.0);
    let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    step(&g, &mut w, id);
    assert_eq!(w.get(id).unwrap().m.hspeed(), r(-2.0));
}

#[test]
fn platform_without_bounce_drives_into_the_solid() {
    let g = plat_game(params(&[("hspeed", real(2.0)), ("bounce", no())]));
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.block, 133.0, 100.0);
    let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    step(&g, &mut w, id);
    assert_eq!(w.get(id).unwrap().m.hspeed(), r(2.0));
}

#[test]
fn platform_carries_a_kid_standing_on_it() {
    let g = plat_game(params(&[("hspeed", real(2.0))]));
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 110.0, 97.0);
    step(&g, &mut w, id);
    assert_eq!(pos(&w, kid), (112.0, 97.0));
}

#[test]
fn platform_ignores_a_kid_far_above() {
    let g = plat_game(params(&[("hspeed", real(2.0))]));
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    let kid = spawn(&g, &mut w, g.kid, 110.0, 60.0);
    step(&g, &mut w, id);
    assert_eq!(pos(&w, kid), (110.0, 60.0));
}

#[test]
fn platform_upward_motion_goes_through_yspeed() {
    let g = plat_game(params(&[("vspeed", real(-2.0))]));
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    step(&g, &mut w, id);
    // Step 5 moved vspeed into yspeed, so the automatic motion adds nothing yet.
    assert_eq!(vspeed(&w, id), r(0.0));
    assert_eq!(pos(&w, id), (100.0, 100.0));
    step(&g, &mut w, id);
    assert_eq!(pos(&w, id), (100.0, 98.0));
}

#[test]
fn platform_waits_for_the_first_touch() {
    let g = plat_game(params(&[("hspeed", real(3.0)), ("start_on_touch", yes())]));
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    step(&g, &mut w, id);
    assert_eq!(pos(&w, id), (100.0, 100.0));
    let kid = spawn(&g, &mut w, g.kid, 110.0, 97.0);
    touch(&g, &mut w, id, kid, ClassMask::KID);
    step(&g, &mut w, id);
    assert_eq!(pos(&w, id), (103.0, 100.0));
}

#[test]
fn platform_marker_reverses_per_rule() {
    for (rule, want_h, want_v) in [("both", -2.0, 1.0), ("vertical", 2.0, 1.0), ("none", 2.0, -1.0)] {
        let g = plat_game(params(&[("hspeed", real(2.0)), ("marker_reverse", word(rule))]));
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        set_v(&mut w, id, -1.0);
        with(&g, &mut w, |c| {
            builtins::moving_platform::reverse_on_marker(c, id).unwrap()
        });
        let m = &w.get(id).unwrap().m;
        assert_eq!((m.hspeed().get(), m.vspeed().get()), (want_h, want_v), "{rule}");
    }
}

#[test]
fn platform_b_lines_runs_in_end_step_only() {
    let g = b_game(&[("hspeed", real(2.0))]);
    let k = &g.def.kinds[usize::try_from(kind_id(&g, "plat").0).unwrap()];
    assert!(k.events.end_step && !k.events.step);
    let y = plat_game(Params::default());
    let k = &y.def.kinds[usize::try_from(kind_id(&y, "plat").0).unwrap()];
    assert!(k.events.step && !k.events.end_step);
}

#[test]
fn platform_b_lines_carries_a_kid_touching_both_lines() {
    let g = b_game(&[("hspeed", real(2.0))]);
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    let kid = spawn(&g, &mut w, kind_id(&g, "tall"), 110.0, 70.0);
    end_step(&g, &mut w, id);
    assert_eq!(pos(&w, kid), (112.0, 70.0));
    assert_eq!(pos(&w, id), (102.0, 100.0), "the platform moved by its own speed");
}

#[test]
fn platform_b_lines_needs_both_lines_to_touch() {
    let g = b_game(&[("hspeed", real(2.0))]);
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    let high = spawn(&g, &mut w, kind_id(&g, "tall"), 110.0, 40.0);
    end_step(&g, &mut w, id);
    assert_eq!(pos(&w, high), (110.0, 40.0), "touches neither line");
    let low = spawn(&g, &mut w, kind_id(&g, "tall"), 110.0, 95.0);
    w.get_mut(high).unwrap().alive = false;
    end_step(&g, &mut w, id);
    assert_eq!(pos(&w, low), (110.0, 95.0), "only the lower line");
}

#[test]
fn platform_b_lines_moves_x_and_y_only_where_free() {
    // A block just beside the kid stops the x move but not the y carry.
    let g = b_game(&[("hspeed", real(2.0)), ("vspeed", real(3.0)), ("bounce", no())]);
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.block, 143.0, 70.0);
    let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    let kid = spawn(&g, &mut w, kind_id(&g, "tall"), 110.0, 70.0);
    end_step(&g, &mut w, id);
    assert_eq!(pos(&w, kid), (110.0, 73.0), "x blocked, y carried by vspeed 3");
}

#[test]
fn platform_b_lines_with_no_kid_just_moves() {
    let g = b_game(&[("hspeed", real(2.0))]);
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
    end_step(&g, &mut w, id);
    assert_eq!(pos(&w, id), (102.0, 100.0));
}
