//! secret_item: setting its flag and being removed when taken.

use iwbtg_sim::def::Builtin;
use iwbtg_sim::world::World;

use crate::builtins_boss_item_support::*;

#[test]
fn secret_item_sets_its_flag_and_is_removed_when_taken() {
    let g = game(vec![kind(
        Builtin::SecretItem,
        "secret",
        params(&[("item_index", real(2.0))]),
    )]);
    let mut w = World::new(g.room, 1);
    let it = spawn(&g, &mut w, kind_id(&g, "secret"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    touch(&g, &mut w, it, kid);
    assert!(w.engine.mech.item(2));
    assert!(!alive(&w, it));
    let again = spawn(&g, &mut w, kind_id(&g, "secret"), 0.0, 0.0);
    assert!(!alive(&w, again), "Create removes a taken item");
}
