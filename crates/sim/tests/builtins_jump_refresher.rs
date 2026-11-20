//! jump_refresher: the air jump, the hide time and the tjump rules.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, Params};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::kid::api::{self};

use crate::builtins_kid_modifiers_support::*;

#[test]
fn refresher_gives_the_air_jump_and_hides_for_fifty_ticks() {
    let mut g = game(Builtin::JumpRefresher, Params::default());
    let kid = spawn(&mut g, "kid", 100.0, 100.0);
    let rf = spawn(&mut g, "thing", 120.0, 90.0);
    assert!(!djump(&g, kid));
    touch(&mut g, rf, kid);
    assert!(djump(&g, kid));
    assert_eq!(pos(&g, rf), (0.0, 0.0), "hidden at the origin");
    assert_eq!(g.w.get(rf).unwrap().alarms[0], r(50.0));
    fire(&mut g, rf, &EventKind::Alarm(0));
    assert_eq!(pos(&g, rf), (120.0, 90.0), "back at its start");
}

#[test]
fn refresher_hide_time_is_a_param() {
    let mut g = game(Builtin::JumpRefresher, params(&[("hide_ticks", real(7.0))]));
    let kid = spawn(&mut g, "kid", 0.0, 0.0);
    let rf = spawn(&mut g, "thing", 50.0, 50.0);
    touch(&mut g, rf, kid);
    assert_eq!(g.w.get(rf).unwrap().alarms[0], r(7.0));
    let ok = params(&[("tjump", word("none"))]);
    assert!(builtins::init_state(Builtin::JumpRefresher, &ok).is_ok());
    let bad = params(&[("tjump", word("maybe"))]);
    assert!(builtins::init_state(Builtin::JumpRefresher, &bad).is_err());
}

#[test]
fn refresher_sets_or_clears_tjump_and_none_leaves_it() {
    for (mode, start, want) in [("set", false, true), ("clear", true, false), ("none", true, true)] {
        let mut g = game(Builtin::JumpRefresher, params(&[("tjump", word(mode))]));
        let kid = spawn(&mut g, "kid", 100.0, 100.0);
        let rf = spawn(&mut g, "thing", 120.0, 90.0);
        with(&mut g, |c| api::set_triple_jump(c, kid, start));
        touch(&mut g, rf, kid);
        assert_eq!(read(&g, kid, "tjump") > 0.0, want, "tjump = {mode}");
        assert!(djump(&g, kid), "the air jump is refreshed either way");
    }
}
