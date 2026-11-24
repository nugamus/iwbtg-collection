//! item_viewer: showing only while its flag is set.

use iwbtg_sim::def::Builtin;
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

use crate::builtins_boss_item_support::*;

#[test]
fn item_viewer_shows_only_while_its_flag_is_set() {
    let g = game(vec![
        kind(
            Builtin::ItemViewer,
            "boss_view",
            params(&[("kind", word("boss")), ("index", real(1.0))]),
        ),
        kind(
            Builtin::ItemViewer,
            "item_view",
            params(&[("kind", word("item")), ("index", real(1.0))]),
        ),
    ]);
    let mut w = World::new(g.room, 1);
    let bv = spawn(&g, &mut w, kind_id(&g, "boss_view"), 0.0, 0.0);
    let iv = spawn(&g, &mut w, kind_id(&g, "item_view"), 0.0, 0.0);
    assert!(!w.get(bv).unwrap().visible && !w.get(iv).unwrap().visible);
    w.engine.mech.set_boss(1);
    fire(&g, &mut w, bv, &EventKind::Step);
    fire(&g, &mut w, iv, &EventKind::Step);
    assert!(w.get(bv).unwrap().visible, "boss flag 1 set");
    assert!(!w.get(iv).unwrap().visible, "item flag 1 not set");
}
