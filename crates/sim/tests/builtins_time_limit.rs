//! time_limit: the trigger zone, the per-second alarms, the borrow and the kid gate.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, ParamValue};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::rules::tick::ClockPolicy;

use crate::builtins_misc_support::*;

#[test]
fn touching_the_zone_arms_every_timer_and_destroys_the_zone() {
    let mut g = timer_game(&[("seconds", real(5.0)), ("on_expire", word("nothing"))]);
    let kid = spawn(&mut g, "kid", 0.0, 0.0);
    let t = spawn(&mut g, "timer", 100.0, 100.0);
    let z = spawn(&mut g, "zone", 200.0, 100.0);
    assert!(g.w.get(t).unwrap().alarms[0].get() < 0.0);
    touch(&mut g, z, kid, ClassMask::KID);
    assert!(!alive(&g, z));
    assert_eq!(g.w.get(t).unwrap().alarms[0].get(), 50.0);
}

#[test]
fn each_alarm_takes_a_second_and_expiry_comes_one_period_after_zero() {
    let mut g = timer_game(&[("seconds", real(2.0)), ("on_expire", word("nothing"))]);
    spawn(&mut g, "kid", 0.0, 0.0);
    let t = spawn(&mut g, "timer", 100.0, 100.0);
    second(&mut g, t);
    assert_eq!(left(&g, t), (0, 0, 1));
    second(&mut g, t);
    assert_eq!(left(&g, t), (0, 0, 0));
    assert!(
        alive(&g, t),
        "the display reads 0:00:00 and the timer is still there"
    );
    assert_eq!(g.w.get(t).unwrap().alarms[0].get(), 50.0, "it re-arms itself");
    second(&mut g, t);
    assert!(!alive(&g, t));
}

#[test]
fn negative_seconds_borrow_from_minutes_then_hours() {
    let mut g = timer_game(&[("hours", real(1.0)), ("seconds", real(0.0))]);
    spawn(&mut g, "kid", 0.0, 0.0);
    let t = spawn(&mut g, "timer", 100.0, 100.0);
    second(&mut g, t);
    assert_eq!(left(&g, t), (0, 59, 59));
    let mut g = timer_game(&[("minutes", real(2.0)), ("seconds", real(0.0))]);
    spawn(&mut g, "kid", 0.0, 0.0);
    let t = spawn(&mut g, "timer", 100.0, 100.0);
    second(&mut g, t);
    assert_eq!(left(&g, t), (0, 1, 59));
}

#[test]
fn the_clock_waits_while_no_kid_exists() {
    let mut g = timer_game(&[("seconds", real(5.0))]);
    let t = spawn(&mut g, "timer", 100.0, 100.0);
    second(&mut g, t);
    assert_eq!(left(&g, t), (0, 0, 5));
    assert_eq!(g.w.get(t).unwrap().alarms[0].get(), 50.0);
}

#[test]
fn one_second_follows_the_clock_policy() {
    // R30: the legacy clock reads the room's `room_speed`; `ClockPolicy::Fixed` reads the
    // package tick rate. The fixture's tick rate is 50 and its room speed is set below.
    let mut g = timer_game(&[("seconds", real(5.0))]);
    let t = spawn(&mut g, "timer", 100.0, 100.0);
    g.w.room_speed = r(30.0);
    second(&mut g, t);
    assert_eq!(
        g.w.get(t).unwrap().alarms[0].get(),
        30.0,
        "legacy: room_speed is one second"
    );

    let mut g = timer_game(&[("seconds", real(5.0))]);
    let t = spawn(&mut g, "timer", 100.0, 100.0);
    g.w.room_speed = r(30.0);
    let mut rules = RunnerRules::default();
    rules.tick.timing.clock = ClockPolicy::Fixed;
    with_rules(&mut g, &rules, |ctx| {
        builtins::on_event(ctx, t, &EventKind::Alarm(0)).expect("alarm");
    });
    assert_eq!(
        g.w.get(t).unwrap().alarms[0].get(),
        50.0,
        "fixed: the tick rate is one second"
    );
}

#[test]
fn time_limit_params_are_checked() {
    let bad =
        |items: &[(&str, ParamValue)]| builtins::init_state(Builtin::TimeLimit, &params(items)).is_err();
    assert!(bad(&[("on_expire", word("goto_room"))]));
    assert!(bad(&[("minutes", real(-1.0))]));
    assert!(bad(&[("seconds", real(1.5))]));
    assert!(bad(&[("on_expire", word("explode"))]));
    assert!(!bad(&[("hours", real(1.0)), ("on_expire", word("kill"))]));
}
