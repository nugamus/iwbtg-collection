//! world: the Create and Room Start, the key-press retry, the Step triggers and the boot
//! countdown (the pause, mute and timer params are in builtins_world.rs).

use iwbtg_sim::def::{ParamValue, Params, RestartTrigger};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::world::{Flow, World};

use crate::builtins_rely_a_support::*;

#[test]
fn world_create_and_room_start() {
    let (g, mut w, id) = world_game(Params::default(), |_| {});
    assert_eq!(w.engine.run.slot, 1);
    w.room_speed = r(30.0);
    fire(&g, &mut w, id, &EventKind::RoomStart);
    assert_eq!(
        w.room_speed,
        r(50.0),
        "world's Room Start sets 50 after the file's 30"
    );
}

#[test]
fn world_key_press_runs_the_retry_when_the_model_says_so() {
    let (g, mut w, id) = world_game(Params::default(), |_| {});
    let flow = fire(&g, &mut w, id, &EventKind::KeyPress(Action::Restart));
    assert_eq!(
        (flow, w.pending),
        (Flow::AbortTick, Some(RoomRequest::GameRestart))
    );
    w.pending = None;
    let flow = fire(&g, &mut w, id, &EventKind::KeyPress(Action::Reset));
    assert_eq!(
        (flow, w.pending),
        (Flow::AbortTick, Some(RoomRequest::GameRestart))
    );
    // A game whose retry runs in Step ignores the key press event.
    let (g, mut w, id) = world_game(Params::default(), |d| {
        d.restart.trigger = RestartTrigger::StepHeld
    });
    let flow = fire(&g, &mut w, id, &EventKind::KeyPress(Action::Restart));
    assert_eq!((flow, w.pending), (Flow::Continue, None));
}

#[test]
fn world_step_runs_held_and_pressed_triggers() {
    let (g, mut w, id) = world_game(Params::default(), |d| {
        d.restart.trigger = RestartTrigger::StepHeld
    });
    frame(&mut w, &[], &[]);
    assert_eq!(fire(&g, &mut w, id, &EventKind::Step), Flow::Continue);
    frame(&mut w, &[Action::Restart], &[]);
    assert_eq!(
        fire(&g, &mut w, id, &EventKind::Step),
        Flow::AbortTick,
        "held, not pressed"
    );
    assert_eq!(w.pending, Some(RoomRequest::GameRestart));

    let (g, mut w, id) = world_game(Params::default(), |d| {
        d.restart.trigger = RestartTrigger::StepPressed {
            skip_while_paused: false,
        }
    });
    frame(&mut w, &[Action::Restart], &[]);
    assert_eq!(
        fire(&g, &mut w, id, &EventKind::Step),
        Flow::Continue,
        "held is not pressed"
    );
    frame(&mut w, &[Action::Restart], &[Action::Restart]);
    assert_eq!(fire(&g, &mut w, id, &EventKind::Step), Flow::AbortTick);
}

#[test]
fn world_step_counts_down_the_boot_load() {
    let (g, mut w, id) = world_game(Params::default(), |_| {});
    assert_eq!(
        fire(&g, &mut w, id, &EventKind::Step),
        Flow::Continue,
        "nothing waiting"
    );
    w.save = Some(iwbtg_sim::save::SaveSlot {
        room: g.room,
        x: r(49.0),
        y: r(567.0),
        flipped: false,
        facing: 1,
        difficulty: 0,
        deaths: 0,
        time: 0,
        bosses: 0,
        items: 0,
        clear: false,
    });
    w.engine.run.boot_load = Some(iwbtg_sim::run_globals::BootLoad { ticks_left: 1 });
    frame(&mut w, &[], &[]);
    assert_eq!(fire(&g, &mut w, id, &EventKind::Step), Flow::AbortTick);
    assert_eq!(w.pending, Some(RoomRequest::Goto(g.room)));
    assert_eq!(pos(&w, alive(&w, g.kid)[0]), (49.0, 567.0));
}

#[test]
fn world_latch_clearing_consumes_the_tap() {
    let tap = |w: &mut World| {
        // Right pressed and released inside one tick: latched, not held.
        w.input.latch(InputFrame {
            held: 0,
            pressed: Action::Right.bit(),
            released: Action::Right.bit(),
        });
    };
    // Without the param the latch survives the world's Step: the kid's later read sees it.
    let (g, mut w, id) = world_game(Params::default(), |_| {});
    tap(&mut w);
    fire(&g, &mut w, id, &EventKind::Step);
    assert!(w.input.direct(Action::Right));
    // With it the world's two reads eat it (kaenbyou, eat-the-lemon, xia).
    let p = params(&[("latch_cleared_by_world", ParamValue::Bool(true))]);
    let (g, mut w, id) = world_game(p, |_| {});
    tap(&mut w);
    fire(&g, &mut w, id, &EventKind::Step);
    assert!(!w.input.direct(Action::Right));
}
