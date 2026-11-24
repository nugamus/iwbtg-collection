//! path_platform: the Create start, the touch mode, carrying a kid on both lines and
//! the path index check.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, ParamValue, Params, PathId};
use iwbtg_sim::events::EventKind;

use crate::builtins_tier3_support::*;

#[test]
fn path_platform_starts_its_path_in_create() {
    let mut g = path_game(&[]);
    let pp = spawn(&mut g, "pp", 50.0, 50.0);
    let run = g.w.get(pp).unwrap().path.clone().expect("path started");
    assert_eq!(run.path, PathId(0));
    assert_eq!(run.speed, r(2.0));
}

#[test]
fn touch_mode_waits_and_starts_slightly_faster() {
    let mut g = path_game(&[("start_on_touch", ParamValue::Bool(true)), ("scale", real(2.0))]);
    let kid = spawn(&mut g, "kid", 0.0, 0.0);
    let pp = spawn(&mut g, "pp", 50.0, 50.0);
    assert!(g.w.get(pp).unwrap().path.is_none());
    touch(&mut g, pp, kid, ClassMask::KID);
    let run = g.w.get(pp).unwrap().path.clone().expect("started");
    assert_eq!(run.speed, r(2.0001));
    assert_eq!(run.scale, r(2.0));
}

#[test]
fn path_platform_carries_a_kid_on_both_lines_when_it_moved() {
    let mut g = path_game(&[("start_on_touch", ParamValue::Bool(true))]);
    let pp = spawn(&mut g, "pp", 100.0, 100.0);
    // A 20 x 32 kid whose body spans both lines (y 83 and 98 for a platform at y 100).
    let kid = spawn(&mut g, "kid", 105.0, 70.0);
    {
        let i = g.w.get_mut(pp).unwrap();
        i.m.set_x(r(103.0)).unwrap();
        let mut run =
            iwbtg_sim::path::start(&i.m, PathId(0), r(2.0), iwbtg_sim::path::PathEnd::Stop, false).unwrap();
        run.position = r(0.5);
        i.path = Some(run);
    }
    // xprevious is 100: the platform moved 3 px this tick.
    fire(&mut g, pp, &EventKind::EndStep);
    assert_eq!(pos(&g, kid), (108.0, 70.0));
    // The position did not change again: no carry.
    fire(&mut g, pp, &EventKind::EndStep);
    assert_eq!(pos(&g, kid), (108.0, 70.0));
}

#[test]
fn path_platform_needs_a_path_index() {
    assert!(builtins::init_state(Builtin::PathPlatform, &Params::default()).is_err());
    let bad = params(&[("path", real(1.5))]);
    assert!(builtins::init_state(Builtin::PathPlatform, &bad).is_err());
}
