//! block: the overrides, the visual collider and the slip and blood sticks.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ParamValue};
use iwbtg_sim::world::World;

use crate::builtins_rely_a_support::*;

#[test]
fn block_create_applies_overrides() {
    let g = game(|_, _| vec![], |_| {});
    let mut w = World::new(g.room, 1);
    let p = params(&[
        ("solid", ParamValue::Bool(false)),
        ("visible", ParamValue::Bool(true)),
        ("depth", real(-5.0)),
    ]);
    let id = spawn_at(&g, &mut w, g.block, 64.0, 96.0, p);
    let i = w.get(id).unwrap();
    assert!(!i.solid && i.visible);
    assert_eq!(i.depth, r(-5.0));
    // Without params the kind's defaults stay.
    let id = spawn(&g, &mut w, g.block, 0.0, 0.0);
    assert!(w.get(id).unwrap().solid);
}

#[test]
fn block_visual_spawns_the_collider_at_its_position() {
    let g = game(|_, _| vec![], |_| {});
    let mut w = World::new(g.room, 1);
    let p = params(&[
        ("visual_spawns_collider", ParamValue::Bool(true)),
        ("collider", ParamValue::Kind(g.block)),
        ("visible", ParamValue::Bool(true)),
    ]);
    spawn_at(&g, &mut w, g.block, 32.0, 576.0, p);
    let blocks = alive(&w, g.block);
    assert_eq!(blocks.len(), 2, "the visual and its collider");
    assert_eq!(pos(&w, blocks[1]), (32.0, 576.0));
    assert!(w.get(blocks[1]).unwrap().solid);
}

#[test]
fn block_collider_param_is_required_with_visual_spawns_collider() {
    let p = params(&[("visual_spawns_collider", ParamValue::Bool(true))]);
    assert!(builtins::init_state(Builtin::Block, &p).is_err());
    let p = params(&[("slip", ParamValue::Bool(true))]);
    assert!(
        builtins::init_state(Builtin::Block, &p).is_err(),
        "a flag is not a real"
    );
    let p = params(&[("mask", word("huge"))]);
    assert!(
        builtins::init_state(Builtin::Block, &p).is_err(),
        "not in the enum"
    );
}

#[test]
fn block_slip_and_blood_sticks_are_readable() {
    let g = game(|_, _| vec![], |_| {});
    let mut w = World::new(g.room, 1);
    let ice = spawn_at(&g, &mut w, g.block, 0.0, 0.0, params(&[("slip", real(0.2))]));
    let plain = spawn_at(
        &g,
        &mut w,
        g.block,
        32.0,
        0.0,
        params(&[("blood_sticks", ParamValue::Bool(false))]),
    );
    assert_eq!(builtins::block::slip_of(&w, ice), r(0.2));
    assert_eq!(builtins::block::slip_of(&w, plain), r(0.0));
    assert!(builtins::block::blood_sticks(&w, ice));
    assert!(!builtins::block::blood_sticks(&w, plain));
}
