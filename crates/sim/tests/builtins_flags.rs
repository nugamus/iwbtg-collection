//! The built-in flag helpers: one bit per index, and the index checks.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, Params};

use crate::builtins_boss_item_support::*;

#[test]
fn flag_helpers_use_one_bit_per_index() {
    let mut m = builtins::MechGlobals::default();
    assert!(!m.boss(3));
    m.set_boss(3);
    m.set_item(15);
    assert!(m.boss(3) && !m.boss(4) && m.item(15));
    assert_eq!((m.bosses, m.items), (8, 0x8000));
}

#[test]
fn flag_indexes_are_checked() {
    for bad in [real(16.0), real(-1.0), real(1.5)] {
        let p = params(&[("boss_index", bad)]);
        assert!(builtins::init_state(Builtin::BossGate, &p).is_err());
    }
    assert!(builtins::init_state(Builtin::BossGate, &Params::default()).is_err());
}
