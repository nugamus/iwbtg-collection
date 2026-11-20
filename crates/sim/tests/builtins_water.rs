//! water: the angle step, drifting and the outside-room destroy, the fall cap, the area
//! form and the djump params.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, Params};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::tick;
use iwbtg_sim::world::{SpawnInit, World};

use crate::builtins_playerface_water_support::*;

/// showdown hidariSuityuu02 (kid/A3.md, Spinning water): Create's `image_angle = 1` is the
/// instance angle, and each tick's Step adds `angle_step` with no wrap. The kind has no sprite
/// here: a sprite turned off the quarter turns is refused when its mask is placed (C3).
#[test]
fn water_angle_step_turns_the_sprite_every_step() {
    for (step, sign) in [(1.0, 1.0), (-1.0, -1.0)] {
        let mut g = water_game(params(&[("angle_step", real(step))]));
        let k = kind_id(&g, "water");
        g.def.kinds[k.0 as usize].sprite = None;
        assert!(g.def.kinds[k.0 as usize].events.step);
        let mut w = World::new(g.room, 1);
        let id = with(&g, &mut w, |c| {
            let init = SpawnInit {
                angle: r(1.0),
                ..SpawnInit::default()
            };
            c.spawn(k, r(100.0), r(100.0), init).unwrap()
        });
        for n in 1..=400 {
            with(&g, &mut w, |c| tick::run_tick(c, InputFrame::EMPTY).unwrap());
            assert_eq!(
                w.get(id).unwrap().img.angle(),
                r(1.0 + sign * f64::from(n)),
                "after {n} steps"
            );
        }
    }
    let g = water_game(Params::default());
    assert!(
        !g.def.kinds[kind_id(&g, "water").0 as usize].events.step,
        "no Step without a turn"
    );
}

/// showdown mizu01 Outside Room (kid/A3.md): a run-time copy with hspeed 2 is destroyed once its
/// box is completely outside the room (`bbox_left > room_width`), not before.
#[test]
fn water_destroy_outside_room_removes_drifting_water() {
    let g = water_game(params(&[("destroy_outside_room", yes())]));
    let k = kind_id(&g, "water");
    assert!(g.def.kinds[k.0 as usize].events.outside_room);
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, k, 790.0, 100.0);
    w.get_mut(id).unwrap().m.set_hspeed(r(2.0)).unwrap();
    // The 32 x 32 mask has its origin at the top left, so bbox_left is x; the room is 800 wide.
    for n in 1..=5 {
        with(&g, &mut w, |c| tick::run_tick(c, InputFrame::EMPTY).unwrap());
        assert!(w.exists(id), "bbox_left {} is not past 800", 790 + 2 * n);
    }
    with(&g, &mut w, |c| tick::run_tick(c, InputFrame::EMPTY).unwrap());
    assert!(!w.exists(id), "bbox_left 802 is past 800");
    let g = water_game(Params::default());
    assert!(!g.def.kinds[kind_id(&g, "water").0 as usize].events.outside_room);
}

#[test]
fn water_caps_the_fall_at_two_by_default() {
    let g = water_game(Params::default());
    let mut w = World::new(g.room, 1);
    let water = spawn(&g, &mut w, kind_id(&g, "water"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    set_v(&mut w, kid, 5.0);
    touch(&g, &mut w, water, kid, ClassMask::KID);
    assert_eq!(vspeed(&w, kid), r(2.0));
    set_v(&mut w, kid, 1.0);
    touch(&g, &mut w, water, kid, ClassMask::KID);
    assert_eq!(vspeed(&w, kid), r(1.0), "slower falls are kept");
    set_v(&mut w, kid, -5.0);
    touch(&g, &mut w, water, kid, ClassMask::KID);
    assert_eq!(vspeed(&w, kid), r(-5.0), "rising is kept");
}

#[test]
fn water_caps_the_flipped_kid_upward() {
    let g = water_game(params(&[("fall_cap", real(2.0))]));
    let mut w = World::new(g.room, 1);
    let water = spawn(&g, &mut w, kind_id(&g, "water"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.flipped, 0.0, 0.0);
    set_v(&mut w, kid, -5.0);
    touch(&g, &mut w, water, kid, ClassMask::KID_FLIPPED);
    assert_eq!(vspeed(&w, kid), r(-2.0));
    set_v(&mut w, kid, 5.0);
    touch(&g, &mut w, water, kid, ClassMask::KID_FLIPPED);
    assert_eq!(vspeed(&w, kid), r(5.0));
}

#[test]
fn water_without_a_cap_leaves_the_speed() {
    let g = water_game(params(&[("fall_cap", real(-1.0))]));
    let mut w = World::new(g.room, 1);
    let water = spawn(&g, &mut w, kind_id(&g, "water"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    set_v(&mut w, kid, 9.0);
    touch(&g, &mut w, water, kid, ClassMask::KID);
    assert_eq!(vspeed(&w, kid), r(9.0));
}

#[test]
fn water_with_an_area_acts_in_step_on_kids_strictly_inside() {
    let g = area_water();
    let k = &g.def.kinds[usize::try_from(kind_id(&g, "water").0).unwrap()];
    assert!(
        k.events.step && k.events.collisions.is_empty(),
        "no collision event"
    );
    let mut w = World::new(g.room, 1);
    let water = spawn(&g, &mut w, kind_id(&g, "water"), 500.0, 500.0);
    let inside = spawn(&g, &mut w, g.kid, 10.0, 60.0);
    let edge = spawn(&g, &mut w, g.kid, 0.0, 60.0);
    let below = spawn(&g, &mut w, g.kid, 10.0, 200.0);
    for id in [inside, edge, below] {
        set_v(&mut w, id, 6.0);
    }
    fire(&g, &mut w, water, &EventKind::Step);
    assert_eq!(vspeed(&w, inside), r(2.0));
    assert_eq!(vspeed(&w, edge), r(6.0), "x = left is not strictly inside");
    assert_eq!(vspeed(&w, below), r(6.0), "y = bottom is not strictly inside");
}

#[test]
fn water_with_an_area_ignores_collision_events() {
    let g = area_water();
    let mut w = World::new(g.room, 1);
    let water = spawn(&g, &mut w, kind_id(&g, "water"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 10.0, 60.0);
    set_v(&mut w, kid, 6.0);
    touch(&g, &mut w, water, kid, ClassMask::KID);
    assert_eq!(vspeed(&w, kid), r(6.0));
}

#[test]
fn water_area_needs_all_four_edges() {
    let p = params(&[("area_left", real(0.0)), ("area_right", real(1.0))]);
    assert!(builtins::init_state(Builtin::Water, &p).is_err());
}

#[test]
fn water_djump_ignoring_flag_loads_and_cur_jumps_must_be_whole() {
    let p = params(&[("air_jump", word("djump_ignoring_flag"))]);
    assert!(builtins::init_state(Builtin::Water, &p).is_ok());
    let p = params(&[("cur_jumps", real(2.5))]);
    assert!(builtins::init_state(Builtin::Water, &p).is_err());
}
