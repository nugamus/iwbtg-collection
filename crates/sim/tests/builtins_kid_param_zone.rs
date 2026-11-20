//! kid_param_zone: the numbers it writes, the target rule and the value checks.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, Params};

use crate::builtins_kid_modifiers_support::*;

#[test]
fn zone_writes_the_kid_numbers_every_overlap_tick() {
    let p = params(&[
        ("run_speed", real(5.0)),
        ("jump", real(16.0)),
        ("jump2", real(13.0)),
    ]);
    let mut g = game(Builtin::KidParamZone, p);
    let kid = spawn(&mut g, "kid", 100.0, 100.0);
    let z = spawn(&mut g, "thing", 100.0, 100.0);
    touch(&mut g, z, kid);
    assert_eq!(read(&g, kid, "maxSpeed"), 5.0);
    assert_eq!(read(&g, kid, "jump"), 16.0);
    assert_eq!(read(&g, kid, "jump2"), 13.0);
}

#[test]
fn zone_target_player_acts_on_the_first_kid_whoever_touches() {
    let p = params(&[("run_speed", real(5.0)), ("target", word("player"))]);
    let mut g = game(Builtin::KidParamZone, p);
    let first = spawn(&mut g, "kid", 0.0, 0.0);
    let second = spawn(&mut g, "kid", 300.0, 0.0);
    let z = spawn(&mut g, "thing", 300.0, 0.0);
    let before = read(&g, second, "maxSpeed");
    touch(&mut g, z, second);
    assert_eq!(read(&g, first, "maxSpeed"), 5.0);
    assert_eq!(read(&g, second, "maxSpeed"), before, "the toucher is not changed");
}

#[test]
fn zone_needs_a_number_and_checks_values() {
    assert!(builtins::init_state(Builtin::KidParamZone, &Params::default()).is_err());
    let bad = params(&[("jump", word("high"))]);
    assert!(builtins::init_state(Builtin::KidParamZone, &bad).is_err());
}
