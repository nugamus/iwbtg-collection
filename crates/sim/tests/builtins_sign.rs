//! sign: the up-press and overlap modes, and the text and mode checks.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, ParamValue};
use iwbtg_sim::events::EventKind;

use crate::builtins_misc_support::*;

#[test]
fn an_up_press_sign_speaks_only_on_an_up_press_while_a_kid_overlaps() {
    let mut g = game(vec![kind_of(
        "sign",
        Builtin::Sign,
        params(&[("text", word("hello"))]),
    )]);
    spawn(&mut g, "kid", 100.0, 100.0);
    let s = spawn(&mut g, "sign", 100.0, 100.0);
    fire(&mut g, s, &EventKind::Step);
    assert!(texts(&mut g).is_empty(), "no press, no text");
    up_pressed(&mut g);
    fire(&mut g, s, &EventKind::Step);
    assert_eq!(texts(&mut g), vec!["hello".to_owned()]);
    let far = spawn(&mut g, "sign", 500.0, 500.0);
    fire(&mut g, far, &EventKind::Step);
    assert!(texts(&mut g).is_empty(), "a sign the kid is not on stays quiet");
}

#[test]
fn an_overlap_sign_speaks_every_tick_a_kid_touches_it_and_ignores_the_step() {
    let mut g = game(vec![kind_of(
        "sign",
        Builtin::Sign,
        params(&[("text", word("hi")), ("mode", word("while_overlapping"))]),
    )]);
    let kid = spawn(&mut g, "kid", 100.0, 100.0);
    let s = spawn(&mut g, "sign", 100.0, 100.0);
    up_pressed(&mut g);
    fire(&mut g, s, &EventKind::Step);
    assert!(texts(&mut g).is_empty());
    touch(&mut g, s, kid, ClassMask::KID);
    touch(&mut g, s, kid, ClassMask::KID);
    assert_eq!(texts(&mut g).len(), 2);
}

#[test]
fn a_sign_needs_its_text_and_a_known_mode() {
    let bad = |items: &[(&str, ParamValue)]| builtins::init_state(Builtin::Sign, &params(items)).is_err();
    assert!(bad(&[]));
    assert!(bad(&[("text", word("x")), ("mode", word("always"))]));
    assert!(!bad(&[("text", word("x"))]));
}
