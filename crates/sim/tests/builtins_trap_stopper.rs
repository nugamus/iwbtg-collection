//! trap_stopper: no events, and the destroy_stopper behaviour in Step.

use iwbtg_sim::def::Builtin;
use iwbtg_sim::events::EventKind;

use crate::builtins_tier3_support::*;

#[test]
fn stop_stopper_has_no_events_and_destroy_stopper_runs_in_step() {
    let g = game(vec![
        kind_of("stop", Builtin::TrapStopper, params(&[("mode", word("stop"))])),
        kind_of("kill", Builtin::TrapStopper, params(&[("mode", word("destroy"))])),
    ]);
    let ev = |name: &str| {
        g.def.kinds[usize::try_from(kind(&g, name).0).unwrap()]
            .events
            .clone()
    };
    assert!(!ev("stop").step);
    assert!(ev("kill").step);
}

#[test]
fn destroy_stopper_removes_the_traps_it_overlaps() {
    let trap = params(&[("trigger_id", real(1.0))]);
    let mut g = game(vec![
        kind_of("kill", Builtin::TrapStopper, params(&[("mode", word("destroy"))])),
        kind_of("trap", Builtin::Trap, trap),
    ]);
    let stopper = spawn(&mut g, "kill", 100.0, 100.0);
    let over = spawn(&mut g, "trap", 110.0, 110.0);
    let away = spawn(&mut g, "trap", 400.0, 400.0);
    fire(&mut g, stopper, &EventKind::Step);
    assert!(!alive(&g, over));
    assert!(alive(&g, away));
}
