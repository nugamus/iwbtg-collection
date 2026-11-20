//! trap: the launch, the stop chain, move-to and step-toward, the stopper, depth reveal and
//! the path and chain requirements.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, KindId, ParamValue};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::World;

use crate::builtins_trap_block_support::*;

#[test]
fn trap_waits_then_launches_once() {
    let g = trap_game(&[("vspeed", real(6.0))]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
    step(&g, &mut w, t);
    assert_eq!(speeds(&w, t), (0.0, 0.0));
    w.engine.mech.trigger_flags.insert(1);
    step(&g, &mut w, t);
    assert_eq!(speeds(&w, t), (0.0, 6.0));
    // Stopped by hand: the speed guard alone would relaunch, the flag is not asked for.
    w.get_mut(t).unwrap().m.set_vspeed(r(0.0)).unwrap();
    step(&g, &mut w, t);
    assert_eq!(
        speeds(&w, t),
        (0.0, 6.0),
        "without flag_guard a stopped trap relaunches"
    );
}

#[test]
fn trap_flag_guard_launches_once() {
    let g = trap_game(&[("vspeed", real(6.0)), ("flag_guard", yes())]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
    w.engine.mech.trigger_flags.insert(1);
    step(&g, &mut w, t);
    w.get_mut(t).unwrap().m.set_vspeed(r(0.0)).unwrap();
    step(&g, &mut w, t);
    assert_eq!(speeds(&w, t), (0.0, 0.0));
}

#[test]
fn trap_with_id_zero_and_shared_trigger_launches_at_once() {
    let g = game(vec![
        kind(
            Builtin::Trigger,
            "trig",
            params(&[("model", word("shared_last"))]),
        ),
        kind(
            Builtin::Trap,
            "trap",
            params(&[
                ("trigger_model", word("shared_last")),
                ("trigger_kind", ParamValue::Kind(KindId(3))),
                ("vspeed", real(-8.0)),
            ]),
        ),
    ]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
    step(&g, &mut w, t);
    assert_eq!(speeds(&w, t), (0.0, -8.0), "uninitialized triggered reads as 0");
}

#[test]
fn trap_angle_launch() {
    let g = trap_game(&[
        ("launch", word("angle")),
        ("dir", real(90.0)),
        ("speed", real(4.0)),
    ]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
    w.engine.mech.trigger_flags.insert(1);
    step(&g, &mut w, t);
    let (h, v) = speeds(&w, t);
    assert!(h.abs() < 1e-9 && (v + 4.0).abs() < 1e-9, "{h} {v}");
}

#[test]
fn trap_stop_chain_walks_its_alarms() {
    let g = trap_game(&[
        ("vspeed", real(-5.0)),
        ("alm", real(3.0)),
        ("alm2", real(2.0)),
        ("v2", real(5.0)),
    ]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
    w.engine.mech.trigger_flags.insert(1);
    step(&g, &mut w, t);
    assert_eq!(speeds(&w, t), (0.0, -5.0));
    assert_eq!(w.get(t).unwrap().alarms[0], r(3.0));
    fire(&g, &mut w, t, &EventKind::Alarm(0));
    assert_eq!(speeds(&w, t), (0.0, 0.0));
    assert_eq!(w.get(t).unwrap().alarms[1], r(2.0));
    fire(&g, &mut w, t, &EventKind::Alarm(1));
    assert_eq!(speeds(&w, t), (0.0, 5.0));
    // Alarm 2 was unset (alm3 = 0), so the chain ends here.
    assert_eq!(w.get(t).unwrap().alarms[2], r(-1.0));
}

#[test]
fn trap_stop_chain_ends_when_the_trigger_is_gone() {
    let g = trap_game(&[
        ("vspeed", real(-5.0)),
        ("alm", real(3.0)),
        ("alm2", real(2.0)),
        ("v2", real(5.0)),
    ]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
    w.engine.mech.trigger_flags.insert(1);
    step(&g, &mut w, t);
    fire(&g, &mut w, t, &EventKind::Alarm(0));
    w.engine.mech.trigger_flags.clear();
    fire(&g, &mut w, t, &EventKind::Alarm(1));
    assert_eq!(speeds(&w, t), (0.0, 0.0));
}

#[test]
fn trap_move_to_runs_toward_its_target_and_snaps() {
    let g = trap_game(&[
        ("launch", word("move_to")),
        ("endx", real(111.0)),
        ("hspd", real(4.0)),
    ]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
    step(&g, &mut w, t);
    assert_eq!(w.get(t).unwrap().m.x(), r(100.0), "waits for global.stop");
    w.engine.mech.stop_flags.insert(1);
    step(&g, &mut w, t);
    step(&g, &mut w, t);
    assert_eq!(w.get(t).unwrap().m.x(), r(108.0));
    step(&g, &mut w, t);
    assert_eq!(w.get(t).unwrap().m.x(), r(111.0), "within half a step: snaps");
    step(&g, &mut w, t);
    assert_eq!(w.get(t).unwrap().m.x(), r(111.0), "step is 0 now");
}

#[test]
fn trap_step_toward_moves_one_px_x_first_while_the_switch_is_on() {
    let g = toward_game(&[("to_x", real(97.0)), ("to_y", real(103.0))]);
    let kind = g.def.kinds.iter().find(|k| k.name == "trap").unwrap();
    assert!(kind.events.end_step && !kind.events.step, "runs in End Step only");
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
    fire(&g, &mut w, t, &EventKind::EndStep);
    assert_eq!(pos(&w, t), (100.0, 100.0), "waits for the switch");
    w.engine.mech.switches.insert(1);
    let mut seen = Vec::new();
    for _ in 0..8 {
        fire(&g, &mut w, t, &EventKind::EndStep);
        seen.push(pos(&w, t));
    }
    assert_eq!(
        seen,
        vec![
            (99.0, 100.0),
            (98.0, 100.0),
            (97.0, 100.0),
            (97.0, 101.0),
            (97.0, 102.0),
            (97.0, 103.0),
            (97.0, 103.0),
            (97.0, 103.0),
        ],
        "1 px per tick, x first, never diagonal, then it rests"
    );
    // The switch off (world Room End clears it): it stops where it is.
    w.get_mut(t).unwrap().m.set_x(r(90.0)).unwrap();
    w.engine.mech.clear_switches();
    fire(&g, &mut w, t, &EventKind::EndStep);
    assert_eq!(pos(&w, t), (90.0, 103.0));
}

#[test]
fn trap_step_toward_target_defaults_to_its_own_position() {
    let g = toward_game(&[("to_y", real(98.0))]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
    w.engine.mech.switches.insert(1);
    fire(&g, &mut w, t, &EventKind::EndStep);
    assert_eq!(pos(&w, t), (100.0, 99.0), "to_x 0 is its own x: y moves at once");
    fire(&g, &mut w, t, &EventKind::EndStep);
    fire(&g, &mut w, t, &EventKind::EndStep);
    assert_eq!(pos(&w, t), (100.0, 98.0));
}

#[test]
fn trap_stopper_zero_speed_and_flush() {
    let g = trap_game(&[("vspeed", real(6.0)), ("stop_on_stopper", word("flush"))]);
    let mut w = World::new(g.room, 1);
    let stopper = spawn(&g, &mut w, KindId(2), 100.0, 200.0);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 165.0);
    w.get_mut(t).unwrap().m.set_vspeed(r(6.0)).unwrap();
    touch(&g, &mut w, t, stopper, ClassMask::TRAP_STOPPER);
    assert_eq!(speeds(&w, t), (0.0, 0.0));
    // Box 32 high: flush puts the bottom row against the stopper top row at 200.
    assert_eq!(w.get(t).unwrap().m.y(), r(168.0));
    let g2 = trap_game(&[("vspeed", real(6.0)), ("stop_on_stopper", word("zero_speed"))]);
    let mut w2 = World::new(g2.room, 1);
    let s2 = spawn(&g2, &mut w2, KindId(2), 100.0, 200.0);
    let t2 = spawn(&g2, &mut w2, kind_id(&g2, "trap"), 100.0, 165.0);
    w2.get_mut(t2).unwrap().m.set_vspeed(r(6.0)).unwrap();
    touch(&g2, &mut w2, t2, s2, ClassMask::TRAP_STOPPER);
    assert_eq!(w2.get(t2).unwrap().m.y(), r(165.0));
    assert_eq!(speeds(&w2, t2), (0.0, 0.0));
}

#[test]
fn trap_depth_reveal_after_32_px() {
    let g = trap_game(&[("vspeed", real(20.0)), ("depth_reveal", yes())]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
    assert_eq!(w.get(t).unwrap().depth, r(1_000_002.0));
    w.engine.mech.trigger_flags.insert(1);
    step(&g, &mut w, t);
    assert_eq!(w.get(t).unwrap().depth, r(1_000_002.0), "20 px is not far enough");
    step(&g, &mut w, t);
    step(&g, &mut w, t);
    assert_eq!(w.get(t).unwrap().depth, r(0.0));
}

#[test]
fn trap_destroyed_outside_the_room_when_asked() {
    let g = trap_game(&[("destroy_outside_room", yes())]);
    let mut w = World::new(g.room, 1);
    let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
    fire(&g, &mut w, t, &EventKind::OutsideRoom);
    assert!(!alive(&w, t));
}

#[test]
fn trap_path_launch_needs_a_path() {
    assert!(builtins::init_state(Builtin::Trap, &params(&[("launch", word("path"))])).is_err());
}

#[test]
fn trap_chain_needs_the_velocity_launch() {
    let p = params(&[("launch", word("angle")), ("alm", real(3.0))]);
    assert!(builtins::init_state(Builtin::Trap, &p).is_err());
}
