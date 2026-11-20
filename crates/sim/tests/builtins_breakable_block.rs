//! breakable_block: the shot and touch rules, the stage alarms and the counts.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask};
use iwbtg_sim::events::EventKind;

use crate::builtins_tier3_support::*;

#[test]
fn shot_breaks_the_block_and_a_kid_touch_does_not() {
    let mut g = game(vec![kind_of(
        "brk",
        Builtin::BreakableBlock,
        params(&[("break_on", word("shot"))]),
    )]);
    let kid = spawn(&mut g, "kid", 0.0, 0.0);
    let bullet = spawn(&mut g, "bullet", 0.0, 0.0);
    let b = spawn(&mut g, "brk", 100.0, 100.0);
    touch(&mut g, b, kid, ClassMask::KID);
    assert!(alive(&g, b));
    touch(&mut g, b, bullet, ClassMask::KID_BULLET);
    assert!(!alive(&g, b));
}

#[test]
fn touch_delay_runs_its_stages_then_destroys() {
    let p = params(&[("break_on", word("touch_delay")), ("stages", real(3.0))]);
    let mut g = game(vec![kind_of("brk", Builtin::BreakableBlock, p)]);
    let kid = spawn(&mut g, "kid", 0.0, 0.0);
    let b = spawn(&mut g, "brk", 100.0, 100.0);
    touch(&mut g, b, kid, ClassMask::KID);
    assert_eq!(g.w.get(b).unwrap().alarms[0], r(50.0));
    // A second touch does not re-arm it.
    g.w.get_mut(b).unwrap().alarms[0] = r(20.0);
    touch(&mut g, b, kid, ClassMask::KID);
    assert_eq!(g.w.get(b).unwrap().alarms[0], r(20.0));
    fire(&mut g, b, &EventKind::Alarm(0));
    assert_eq!(g.w.get(b).unwrap().img.index(), r(1.0));
    fire(&mut g, b, &EventKind::Alarm(0));
    assert_eq!(g.w.get(b).unwrap().img.index(), r(2.0));
    assert!(alive(&g, b));
    fire(&mut g, b, &EventKind::Alarm(0));
    assert!(!alive(&g, b), "destroyed after the third stage");
}

#[test]
fn one_stage_block_is_destroyed_after_the_first_alarm() {
    let p = params(&[("break_on", word("touch_delay"))]);
    let mut g = game(vec![kind_of("brk", Builtin::BreakableBlock, p)]);
    let kid = spawn(&mut g, "kid", 0.0, 0.0);
    let b = spawn(&mut g, "brk", 100.0, 100.0);
    touch(&mut g, b, kid, ClassMask::KID);
    fire(&mut g, b, &EventKind::Alarm(0));
    assert!(!alive(&g, b));
}

#[test]
fn breakable_checks_its_counts() {
    let bad = params(&[("stages", real(0.0))]);
    assert!(builtins::init_state(Builtin::BreakableBlock, &bad).is_err());
}
