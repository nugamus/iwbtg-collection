//! boss_item: appearing when the boss is gone, and the flag that removes it.

use iwbtg_sim::def::KindId;
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

use crate::builtins_boss_item_support::*;

#[test]
fn boss_item_appears_when_the_boss_is_gone_and_is_taken_by_touch() {
    let g = boss_item_game();
    let mut w = World::new(g.room, 1);
    let boss = spawn(&g, &mut w, KindId(1), 100.0, 100.0);
    let item = spawn(&g, &mut w, kind_id(&g, "reward"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    assert!(!w.get(item).unwrap().visible);
    fire(&g, &mut w, item, &EventKind::Step);
    assert!(!w.get(item).unwrap().visible, "the boss still lives");
    touch(&g, &mut w, item, kid);
    assert!(alive(&w, item) && !w.engine.mech.boss(1), "invisible: no touch");
    with(&g, &mut w, |c| c.destroy(boss));
    fire(&g, &mut w, item, &EventKind::Step);
    assert!(w.get(item).unwrap().visible);
    touch(&g, &mut w, item, kid);
    assert!(w.engine.mech.boss(1));
    assert!(!alive(&w, item));
}

#[test]
fn boss_item_with_the_flag_set_destroys_itself() {
    let g = boss_item_game();
    let mut w = World::new(g.room, 1);
    w.engine.mech.set_boss(1);
    let item = spawn(&g, &mut w, kind_id(&g, "reward"), 0.0, 0.0);
    fire(&g, &mut w, item, &EventKind::Step);
    assert!(!alive(&w, item));
}
