//! trigger: the shared/per-object/global last models and the flag, switch and key
//! variants.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, KindId};
use iwbtg_sim::world::World;

use crate::builtins_trap_block_support::*;

#[test]
fn shared_last_writes_every_instance_of_the_kind() {
    let g = trig_game("shared_last", &[]);
    let mut w = World::new(g.room, 1);
    let t = kind_id(&g, "trig");
    let a = spawn(&g, &mut w, t, 0.0, 0.0);
    let b = spawn(&g, &mut w, t, 100.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    assert!(!with(&g, &mut w, |c| builtins::trigger::fired(
        c,
        builtins::trigger::Model::SharedLast,
        r(3.0),
        &[t]
    )
    .unwrap()));
    // The second instance is touched; the trap reads the first one.
    touch(&g, &mut w, b, kid, ClassMask::KID);
    assert!(with(&g, &mut w, |c| builtins::trigger::fired(
        c,
        builtins::trigger::Model::SharedLast,
        r(3.0),
        &[t]
    )
    .unwrap()));
    let _ = a;
}

#[test]
fn global_last_sets_the_global_and_destroys_itself() {
    let g = trig_game("global_last", &[]);
    let mut w = World::new(g.room, 1);
    w.engine.mech.global_triggered = r(9.0);
    let t = spawn(&g, &mut w, kind_id(&g, "trig"), 0.0, 0.0);
    assert_eq!(
        w.engine.mech.global_triggered,
        r(0.0),
        "Create resets the global id"
    );
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    touch(&g, &mut w, t, kid, ClassMask::KID);
    assert_eq!(w.engine.mech.global_triggered, r(3.0));
    assert!(!alive(&w, t));
}

#[test]
fn global_last_key_variant_needs_the_key() {
    let g = trig_game("global_last", &[("need_key", real(7.0))]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trig"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    touch(&g, &mut w, t, kid, ClassMask::KID);
    assert!(alive(&w, t));
    assert_eq!(w.engine.mech.global_triggered, r(0.0));
    w.engine.mech.global_triggered = r(7.0);
    touch(&g, &mut w, t, kid, ClassMask::KID);
    assert_eq!(w.engine.mech.global_triggered, r(3.0));
    assert!(!alive(&w, t));
}

#[test]
fn flag_array_sets_and_resets() {
    let g = trig_game("flag_array", &[]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trig"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    touch(&g, &mut w, t, kid, ClassMask::KID);
    assert!(w.engine.mech.trigger_flags.contains(&3));
    let g2 = trig_game("flag_array", &[("reset", yes())]);
    let mut w2 = World::new(g2.room, 1);
    w2.engine.mech.trigger_flags.insert(3);
    let t2 = spawn(&g2, &mut w2, kind_id(&g2, "trig"), 0.0, 0.0);
    let kid2 = spawn(&g2, &mut w2, g2.kid, 0.0, 0.0);
    touch(&g2, &mut w2, t2, kid2, ClassMask::KID);
    assert!(!w2.engine.mech.trigger_flags.contains(&3));
}

#[test]
fn flag_array_key_variant_and_stop_array() {
    let g = trig_game("flag_array", &[("need_key", real(1.0))]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trig"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    touch(&g, &mut w, t, kid, ClassMask::KID);
    assert!(w.engine.mech.trigger_flags.is_empty());
    w.engine.mech.trigger_flags.insert(1);
    touch(&g, &mut w, t, kid, ClassMask::KID);
    assert!(w.engine.mech.trigger_flags.contains(&3));
    let g2 = trig_game("flag_array", &[("array", word("stop"))]);
    let mut w2 = World::new(g2.room, 1);
    let t2 = spawn(&g2, &mut w2, kind_id(&g2, "trig"), 0.0, 0.0);
    let kid2 = spawn(&g2, &mut w2, g2.kid, 0.0, 0.0);
    touch(&g2, &mut w2, t2, kid2, ClassMask::KID);
    assert!(w2.engine.mech.stop_flags.contains(&3));
    assert!(w2.engine.mech.trigger_flags.is_empty());
}

#[test]
fn switch_array_and_one_shot_and_frame() {
    let g = trig_game(
        "switch_array",
        &[("one_shot_destroy", yes()), ("sets_frame", yes())],
    );
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trig"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    touch(&g, &mut w, t, kid, ClassMask::KID);
    assert!(w.engine.mech.switches.contains(&3));
    assert_eq!(w.get(t).unwrap().img.index(), r(1.0));
    assert!(!alive(&w, t));
    w.engine.mech.clear_switches();
    assert!(w.engine.mech.switches.is_empty());
}

#[test]
fn per_object_flag_reaches_the_kind_only() {
    let g = game(vec![
        kind(Builtin::Trigger, "t1", params(&[("model", word("per_object"))])),
        kind(Builtin::Trigger, "t2", params(&[("model", word("per_object"))])),
    ]);
    let mut w = World::new(g.room, 1);
    let (k1, k2) = (kind_id(&g, "t1"), kind_id(&g, "t2"));
    let a = spawn(&g, &mut w, k1, 0.0, 0.0);
    spawn(&g, &mut w, k2, 100.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    touch(&g, &mut w, a, kid, ClassMask::KID);
    let seen = |w: &mut World, k: KindId| {
        with(&g, w, |c| {
            builtins::trigger::fired(c, builtins::trigger::Model::PerObject, r(0.0), &[k]).unwrap()
        })
    };
    assert!(seen(&mut w, k1));
    assert!(!seen(&mut w, k2));
}

#[test]
fn trigger_key_needs_a_matching_model() {
    let p = params(&[("model", word("switch_array")), ("need_key", real(1.0))]);
    assert!(builtins::init_state(Builtin::Trigger, &p).is_err());
}
