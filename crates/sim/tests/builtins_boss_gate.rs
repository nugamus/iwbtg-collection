//! boss_gate: staying until its boss is down.

use iwbtg_sim::def::Builtin;
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

use crate::builtins_boss_item_support::*;

#[test]
fn boss_gate_stays_until_its_boss_is_down() {
    let g = game(vec![kind(
        Builtin::BossGate,
        "gate",
        params(&[("boss_index", real(1.0))]),
    )]);
    let mut w = World::new(g.room, 1);
    let gate = spawn(&g, &mut w, kind_id(&g, "gate"), 0.0, 0.0);
    fire(&g, &mut w, gate, &EventKind::Step);
    assert!(alive(&w, gate));
    w.engine.mech.set_boss(1);
    fire(&g, &mut w, gate, &EventKind::Step);
    assert!(!alive(&w, gate));
    let late = spawn(&g, &mut w, kind_id(&g, "gate"), 0.0, 0.0);
    assert!(!alive(&w, late), "Create checks too");
}
