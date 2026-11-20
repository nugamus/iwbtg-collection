//! platform: no events and the param checks.

use iwbtg_sim::builtins;
use iwbtg_sim::def::Builtin;
use iwbtg_sim::events::{EventKind, EventSet};
use iwbtg_sim::world::{Flow, World};

use crate::builtins_tier1_support::*;

#[test]
fn platform_has_no_events_and_checks_its_params() {
    let g = game(
        Builtin::Platform,
        params(&[("width", real(64.0)), ("parent_kind", word("solidblock"))]),
    );
    let e = kind_events(&g);
    assert_eq!(e, EventSet::default());
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, g.thing, 0.0, 0.0);
    assert_eq!(fire(&g, &mut w, id, &EventKind::Step), Flow::Continue);
    assert!(w.get(id).unwrap().alive);
    assert!(builtins::init_state(Builtin::Platform, &params(&[("width", real(0.0))])).is_err());
    assert!(builtins::init_state(Builtin::Platform, &params(&[("parent_kind", word("wall"))])).is_err());
}
