//! game_clear: the flag, the save and the run end.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ParamValue, Params};
use iwbtg_sim::world::World;

use crate::builtins_boss_item_support::*;

#[test]
fn game_clear_sets_the_flag_saves_and_raises_run_end() {
    let g = game(vec![kind(Builtin::GameClear, "clear", Params::default())]);
    let mut w = World::new(g.room, 1);
    let c = spawn(&g, &mut w, kind_id(&g, "clear"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 100.0, 200.0);
    touch(&g, &mut w, c, kid);
    assert!(w.engine.run.clear);
    assert!(w.timing.run_end_raised);
    let slot = w.save.as_ref().expect("saved");
    assert!(slot.clear);
    assert_eq!(slot.room, g.room);
}

#[test]
fn game_clear_without_position_updates_only_an_existing_slot() {
    let p = params(&[("save_with_position", ParamValue::Bool(false))]);
    let g = game(vec![kind(Builtin::GameClear, "clear", p)]);
    let mut w = World::new(g.room, 1);
    let c = spawn(&g, &mut w, kind_id(&g, "clear"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 100.0, 200.0);
    touch(&g, &mut w, c, kid);
    assert!(w.engine.run.clear && w.timing.run_end_raised);
    assert!(w.save.is_none(), "no slot to update");
}

#[test]
fn game_clear_needs_its_fixed_point() {
    let p = params(&[("saved_position", word("fixed_point"))]);
    assert!(builtins::init_state(Builtin::GameClear, &p).is_err());
}
