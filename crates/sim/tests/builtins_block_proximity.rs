//! block: the proximity_frame box edges, the kid follow and the step param.

/// The engine hooks without the sound model (WP10) and timing triggers (WP8b), which are
/// not implemented yet.
use crate::builtins_rely_c_support::*;
use iwbtg_sim::builtins::{self};
use iwbtg_sim::def::Builtin;
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

#[test]
fn proximity_frame_box_edges_are_strict() {
    let g = block02_game();
    // dx in (-8, 40), dy in (-16, 40), from the block's origin (100, 200).
    assert_eq!(frame_with_kid_at(&g, 92.0, 210.0), r(0.0), "dx = -8");
    assert_eq!(frame_with_kid_at(&g, 92.5, 210.0), r(1.0), "dx = -7.5");
    assert_eq!(frame_with_kid_at(&g, 139.5, 210.0), r(1.0), "dx = 39.5");
    assert_eq!(frame_with_kid_at(&g, 140.0, 210.0), r(0.0), "dx = 40");
    assert_eq!(frame_with_kid_at(&g, 110.0, 184.0), r(0.0), "dy = -16");
    assert_eq!(frame_with_kid_at(&g, 110.0, 184.5), r(1.0), "dy = -15.5");
    assert_eq!(frame_with_kid_at(&g, 110.0, 239.5), r(1.0), "dy = 39.5");
    assert_eq!(frame_with_kid_at(&g, 110.0, 240.0), r(0.0), "dy = 40");
}

#[test]
fn proximity_frame_follows_the_kid_and_is_zero_without_one() {
    let g = block02_game();
    let mut w = World::new(g.room, 1);
    let blk = spawn(&g, &mut w, kind_id(&g, "block02"), 100.0, 200.0);
    assert_eq!(
        w.get(blk).unwrap().img.speed(),
        r(0.0),
        "Create sets image_speed 0"
    );
    fire(&g, &mut w, blk, &EventKind::Step);
    assert_eq!(frame(&w, blk), r(0.0), "no kid");
    let kid = spawn(&g, &mut w, g.kid, 110.0, 210.0);
    fire(&g, &mut w, blk, &EventKind::Step);
    assert_eq!(frame(&w, blk), r(1.0), "kid near");
    w.get_mut(kid).unwrap().m.set_x(r(300.0)).unwrap();
    fire(&g, &mut w, blk, &EventKind::Step);
    assert_eq!(frame(&w, blk), r(0.0), "kid gone away");
    w.get_mut(kid).unwrap().m.set_x(r(110.0)).unwrap();
    fire(&g, &mut w, blk, &EventKind::Step);
    assert_eq!(frame(&w, blk), r(1.0));
    with(&g, &mut w, |c| c.destroy(kid));
    fire(&g, &mut w, blk, &EventKind::Step);
    assert_eq!(frame(&w, blk), r(0.0), "kid destroyed");
}

#[test]
fn proximity_frame_adds_step_only_when_set() {
    let g = block02_game();
    let on = builtins::event_set(
        Builtin::Block,
        &g.def.kinds[usize::try_from(kind_id(&g, "block02").0).unwrap()],
    );
    assert!(on.step);
    let off = builtins::event_set(Builtin::Block, &g.def.kinds[usize::try_from(g.block.0).unwrap()]);
    assert!(!off.step);
    // A plain block keeps its frame.
    let mut w = World::new(g.room, 1);
    let blk = spawn(&g, &mut w, g.block, 100.0, 200.0);
    spawn(&g, &mut w, g.kid, 110.0, 210.0);
    fire(&g, &mut w, blk, &EventKind::Step);
    assert_eq!(frame(&w, blk), r(0.0));
}
