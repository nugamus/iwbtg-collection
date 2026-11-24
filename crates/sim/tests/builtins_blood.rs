//! blood: the Create frame and gravity, the outside-room destroy, the block stick and the
//! no-op write of an at-rest stick (#1139).

/// The engine hooks without the sound model (WP10) and timing triggers (WP8b), which are
/// not implemented yet.
use crate::builtins_rely_c_support::*;
use iwbtg_sim::def::{ClassMask, ParamValue, Params};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::rng::{Rng, RngStreams};
use iwbtg_sim::rules::rng::StreamLayout;
use iwbtg_sim::world::World;

#[test]
fn blood_create_draws_the_frame_then_the_gravity() {
    let g = game(Params::default(), Vec::new);
    let mut w = World::new(g.room, 7);
    let mut mirror = Rng::new(7);
    let frame = mirror.choose(3).unwrap();
    let gravity = r(0.1) + mirror.random(r(0.2));
    let id = spawn(&g, &mut w, g.blood, 10.0, 10.0);
    let i = w.get(id).unwrap();
    assert_eq!(i.img.index(), r(f64::from(frame)));
    assert_eq!(i.img.speed(), r(0.0), "blood never animates");
    assert_eq!(i.m.gravity(), gravity);
    assert_eq!(w.rng, mirror, "exactly two draws");
}

#[test]
fn blood_create_draws_the_effects_stream_in_a_split_layout() {
    let g = game(Params::default(), Vec::new);
    let mut w = World::new(g.room, 7);
    w.streams = RngStreams::seeded(7, StreamLayout::Split);
    let gameplay = w.rng.clone();
    let mut effects = w.streams.effects.clone();
    let _ = spawn(&g, &mut w, g.blood, 10.0, 10.0);
    assert_eq!(w.rng, gameplay, "the gameplay stream is untouched");
    effects.choose(3).unwrap();
    effects.random(r(0.2));
    assert_eq!(
        w.streams.effects, effects,
        "the frame then the gravity, from the effects stream"
    );
}

#[test]
fn blood_flipped_has_negative_gravity() {
    let g = game(Params::default(), Vec::new);
    let mut w = World::new(g.room, 7);
    let mut mirror = Rng::new(7);
    mirror.choose(3).unwrap();
    let gravity = r(-0.1) + mirror.random(r(-0.2));
    let id = spawn_at(&g, &mut w, g.blood, 0.0, 0.0, params(&[("flipped", yes())]));
    assert_eq!(w.get(id).unwrap().m.gravity(), gravity);
    assert!(gravity.get() < -0.1 && gravity.get() > -0.3);
}

#[test]
fn blood_is_destroyed_outside_the_room() {
    let g = game(Params::default(), Vec::new);
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, g.blood, 10.0, 10.0);
    fire(&g, &mut w, id, &EventKind::OutsideRoom);
    assert!(!w.get(id).unwrap().alive);
}

#[test]
fn blood_sticks_to_a_block_at_contact() {
    let g = game(Params::default(), Vec::new);
    let mut w = World::new(g.room, 1);
    let blk = spawn(&g, &mut w, g.block, 100.0, 0.0);
    // Moving right at 6 a tick: it was at x 90 (4 px box, reaches 93) and is now at 96,
    // inside the block at 100.
    let id = spawn(&g, &mut w, g.blood, 90.0, 10.0);
    {
        let i = w.get_mut(id).unwrap();
        i.m.motion_set(r(0.0), r(6.0)).unwrap();
        i.m.capture_previous();
        i.m.set_x(r(96.0)).unwrap();
    }
    fire(
        &g,
        &mut w,
        id,
        &EventKind::Collision {
            with: ClassMask::BLOCK,
            other: blk,
        },
    );
    let i = w.get(id).unwrap();
    assert_eq!(
        (i.m.x(), i.m.hspeed(), i.m.vspeed(), i.m.gravity()),
        (r(96.0), r(0.0), r(0.0), r(0.0))
    );
    // A block with blood_sticks = false does not stop it.
    let slick = spawn_at(
        &g,
        &mut w,
        g.block,
        200.0,
        0.0,
        params(&[("blood_sticks", ParamValue::Bool(false))]),
    );
    let id = spawn(&g, &mut w, g.blood, 190.0, 10.0);
    w.get_mut(id).unwrap().m.motion_set(r(0.0), r(6.0)).unwrap();
    fire(
        &g,
        &mut w,
        id,
        &EventKind::Collision {
            with: ClassMask::BLOCK,
            other: slick,
        },
    );
    assert_eq!(w.get(id).unwrap().m.hspeed(), r(6.0));
}

/// #1139: a drop already at rest at its stuck place is not written again, so the grid is not
/// marked moved and the pair walk need not re-gather the cell after its pair.
#[test]
fn a_resting_stick_does_not_mark_the_drop_moved() {
    let g = game(Params::default(), Vec::new);
    let mut w = World::new(g.room, 1);
    let blk = spawn(&g, &mut w, g.block, 100.0, 0.0);
    // Contact, as in `blood_sticks_to_a_block_at_contact`.
    let id = spawn(&g, &mut w, g.blood, 90.0, 10.0);
    let ev = EventKind::Collision {
        with: ClassMask::BLOCK,
        other: blk,
    };
    {
        let i = w.get_mut(id).unwrap();
        i.m.motion_set(r(0.0), r(6.0)).unwrap();
        i.m.capture_previous();
        i.m.set_x(r(96.0)).unwrap();
    }
    fire(&g, &mut w, id, &ev);
    // A later tick at rest: capture_previous, then the same collision. Nothing changes, so
    // nothing is marked moved (the grid starts clean).
    w.get_mut(id).unwrap().m.capture_previous();
    w.grid = iwbtg_sim::collide::grid::Grid::default();
    fire(&g, &mut w, id, &ev);
    assert!(
        !w.grid.moved().contains(&id),
        "an at-rest stick writes nothing, so the grid is not marked moved"
    );
}
