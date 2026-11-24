//! bow: following the kid, staying without one, and the difficulty and select-room destroy.

/// The engine hooks without the sound model (WP10) and timing triggers (WP8b), which are
/// not implemented yet.
use crate::builtins_rely_c_support::*;
use iwbtg_sim::builtins::{self};
use iwbtg_sim::def::{Builtin, ParamValue, Params};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

#[test]
fn bow_copies_the_kid_in_its_follow_phase_only() {
    let g = bow_game(Params::default());
    let mut w = World::new(g.room, 1);
    let kid = spawn(&g, &mut w, g.kid, 120.0, 340.0);
    {
        let i = w.get_mut(kid).unwrap();
        i.img.set_xscale(r(-1.0)).unwrap();
        i.img.set_alpha(r(0.5)).unwrap();
    }
    let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 0.0, 0.0);
    fire(&g, &mut w, bow, &EventKind::BeginStep);
    assert_eq!(w.get(bow).unwrap().m.x(), r(0.0), "not its phase");
    fire(&g, &mut w, bow, &EventKind::Step);
    let i = w.get(bow).unwrap();
    assert_eq!(
        (i.m.x(), i.m.y(), i.img.xscale(), i.img.alpha()),
        (r(120.0), r(340.0), r(-1.0), r(0.5))
    );
    // begin_step games (wanna): the copy is in Begin Step.
    let g = bow_game(params(&[("follow", word("begin_step"))]));
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.kid, 7.0, 8.0);
    let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 0.0, 0.0);
    fire(&g, &mut w, bow, &EventKind::Step);
    assert_eq!(w.get(bow).unwrap().m.x(), r(0.0));
    fire(&g, &mut w, bow, &EventKind::BeginStep);
    assert_eq!(w.get(bow).unwrap().m.x(), r(7.0));
}

#[test]
fn bow_keeps_its_place_without_a_kid() {
    let g = bow_game(Params::default());
    let mut w = World::new(g.room, 1);
    let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 11.0, 12.0);
    fire(&g, &mut w, bow, &EventKind::Step);
    let i = w.get(bow).unwrap();
    assert!(i.alive);
    assert_eq!((i.m.x(), i.m.y()), (r(11.0), r(12.0)));
}

#[test]
fn bow_is_destroyed_above_difficulty_zero_and_in_select_rooms() {
    let g = bow_game(Params::default());
    let mut w = World::new(g.room, 1);
    w.engine.run.difficulty = 1;
    let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 0.0, 0.0);
    fire(&g, &mut w, bow, &EventKind::Step);
    assert!(!w.get(bow).unwrap().alive);
    let mut w = World::new(g.select, 1);
    let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 0.0, 0.0);
    fire(&g, &mut w, bow, &EventKind::Step);
    assert!(!w.get(bow).unwrap().alive, "rSelectStage");
    // Games that keep it on every difficulty say so.
    let g = bow_game(params(&[("medium_only", ParamValue::Bool(false))]));
    let mut w = World::new(g.room, 1);
    w.engine.run.difficulty = 2;
    let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 0.0, 0.0);
    fire(&g, &mut w, bow, &EventKind::Step);
    assert!(w.get(bow).unwrap().alive);
    let set = builtins::event_set(
        Builtin::Bow,
        &g.def.kinds[usize::try_from(kind_id(&g, "bow").0).unwrap()],
    );
    assert!(set.begin_step && set.step && set.end_step);
}
