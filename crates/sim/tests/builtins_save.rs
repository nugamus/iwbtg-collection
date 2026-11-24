//! save: the touch, shoot and on-shot presses, the cooldown, the Create filters, the alarm and
//! the fixed point.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ClassMask, ParamValue, Params};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::Action;
use iwbtg_sim::world::World;

use crate::builtins_rely_b_support::*;

#[test]
fn touch_press_saves_only_with_s_or_z_pressed() {
    let g = save_game(Params::default());
    let (mut w, sp, kid) = save_world(&g);
    frame(&mut w, &[]);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_none(), "touching alone does nothing");
    frame(&mut w, &[Action::Shoot]);
    fire(&g, &mut w, sp, &touch(kid));
    let s = w.save.clone().unwrap();
    assert_eq!(
        (s.room, s.x, s.y),
        (g.room, r(10.0), r(10.0)),
        "base 100 bytes round 10.4 to 10"
    );
    w.save = None;
    frame(&mut w, &[Action::Skip]);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_some(), "S saves too");
}

#[test]
fn shoot_press_ignores_s() {
    let g = save_game(params(&[("on_touch", word("shoot_press"))]));
    let (mut w, sp, kid) = save_world(&g);
    frame(&mut w, &[Action::Skip]);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_none());
    frame(&mut w, &[Action::Shoot]);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_some());
}

#[test]
fn on_shot_saves_from_the_live_kid() {
    let g = save_game(params(&[("on_touch", word("never")), ("on_shot", yes())]));
    let (mut w, sp, kid) = save_world(&g);
    frame(&mut w, &[Action::Shoot]);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_none(), "touch is off");
    let bullet = spawn(&g, &mut w, g.bow, 0.0, 0.0);
    fire(
        &g,
        &mut w,
        sp,
        &EventKind::Collision {
            with: ClassMask::KID_BULLET,
            other: bullet,
        },
    );
    assert!(w.save.is_some());
    // The handlers follow the params.
    let set = builtins::event_set(
        Builtin::Save,
        &g.def.kinds[usize::try_from(kind_id(&g, "sp").0).unwrap()],
    );
    assert_eq!(set.collisions.as_slice(), &[ClassMask::KID_BULLET]);
}

#[test]
fn cooldown_gates_until_the_timer_runs_out() {
    let g = save_game(params(&[("on_touch", word("always")), ("cooldown", real(30.0))]));
    let (mut w, sp, kid) = save_world(&g);
    frame(&mut w, &[]);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_some());
    w.save = None;
    for _ in 0..30 {
        fire(&g, &mut w, sp, &EventKind::Step);
    }
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_none(), "timer is 0 after 30 steps: still blocked");
    fire(&g, &mut w, sp, &EventKind::Step);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_some(), "below 0: saves again");
    // rely's cooldown 0 never gates.
    let g = save_game(Params::default());
    let (mut w, sp, kid) = save_world(&g);
    frame(&mut w, &[Action::Shoot]);
    fire(&g, &mut w, sp, &touch(kid));
    w.save = None;
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_some());
}

#[test]
fn difficulty_filter_destroys_in_create() {
    let g = save_game(params(&[("min_difficulty_removed", real(1.0))]));
    let mut w = World::new(g.room, 1);
    w.engine.run.difficulty = 1;
    let kept = spawn(&g, &mut w, kind_id(&g, "sp"), 0.0, 0.0);
    assert!(w.get(kept).unwrap().alive, "removed only above the value");
    w.engine.run.difficulty = 2;
    let gone = spawn(&g, &mut w, kind_id(&g, "sp"), 0.0, 0.0);
    assert!(!w.get(gone).unwrap().alive);
}

#[test]
fn gravity_and_depth_requirements() {
    let g = save_game(params(&[
        ("on_touch", word("always")),
        ("requires_gravity", word("flipped")),
    ]));
    let (mut w, sp, kid) = save_world(&g);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_none(), "a normal kid is refused");
    w.engine.kid.grav = true;
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_some(), "global.grav matches");
    let g = save_game(params(&[
        ("on_touch", word("always")),
        ("requires_gravity", word("normal")),
    ]));
    let (mut w, sp, kid) = save_world(&g);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_some());
    let g = save_game(params(&[
        ("on_touch", word("always")),
        ("requires_kid_depth0", yes()),
    ]));
    let (mut w, sp, kid) = save_world(&g);
    w.get_mut(kid).unwrap().depth = r(5.0);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_none());
    w.get_mut(kid).unwrap().depth = r(0.0);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_some());
}

#[test]
fn once_destroys_after_the_save() {
    let g = save_game(params(&[("on_touch", word("always")), ("once", yes())]));
    let (mut w, sp, kid) = save_world(&g);
    fire(&g, &mut w, sp, &touch(kid));
    assert!(w.save.is_some() && !w.get(sp).unwrap().alive);
}

#[test]
fn blink_shows_frame_one_until_animation_end() {
    let g = save_game(Params::default());
    let (mut w, sp, kid) = save_world(&g);
    frame(&mut w, &[Action::Shoot]);
    fire(&g, &mut w, sp, &touch(kid));
    let i = w.get(sp).unwrap();
    assert_eq!((i.img.index(), i.img.speed()), (r(1.0), r(0.017)));
    fire(&g, &mut w, sp, &EventKind::AnimationEnd);
    let i = w.get(sp).unwrap();
    assert_eq!((i.img.index(), i.img.speed()), (r(0.0), r(0.0)));
}

/// kaenbyou Key S (kid/B.md Saving, famb-save-restart-b.csv): saveTimer is -1 and Step does not
/// count; a held tick saves and sets 30; the next held ticks count 29 ... 1 and the 30th held
/// tick (timer 1 to 0) saves again.
#[test]
fn skip_held_timer_counts_only_on_held_ticks() {
    let g = save_game(params(&[
        ("on_touch", word("skip_held")),
        ("cooldown", real(30.0)),
    ]));
    let (mut w, sp, _kid) = save_world(&g);
    for _ in 0..40 {
        fire(&g, &mut w, sp, &EventKind::Step);
    }
    fire(&g, &mut w, sp, &EventKind::KeyHeld(Action::Skip));
    assert!(w.save.is_some(), "Step never counted the timer below its start");
    w.save = None;
    for _ in 0..50 {
        fire(&g, &mut w, sp, &EventKind::Step);
    }
    for held in 1..30 {
        fire(&g, &mut w, sp, &EventKind::KeyHeld(Action::Skip));
        assert!(w.save.is_none(), "held tick {held}: timer above 0");
    }
    fire(&g, &mut w, sp, &EventKind::KeyHeld(Action::Skip));
    assert!(w.save.is_some(), "timer reached 0: saves again");
}

#[test]
fn held_s_saves_only_while_overlapping_the_kid() {
    let g = save_game(params(&[
        ("on_touch", word("skip_held")),
        ("saved_position", word("xy_previous")),
    ]));
    let (mut w, sp, kid) = save_world(&g);
    w.get_mut(kid).unwrap().m.set_xprevious(r(9.25)).unwrap();
    w.get_mut(kid).unwrap().m.set_yprevious(r(8.5)).unwrap();
    fire(&g, &mut w, sp, &EventKind::KeyHeld(Action::Skip));
    let s = w.save.clone().unwrap();
    assert_eq!(
        (s.x, s.y),
        (r(9.25), r(8.5)),
        "kaenbyou keeps the exact previous position"
    );
    w.save = None;
    w.get_mut(kid).unwrap().m.set_x(r(300.0)).unwrap();
    fire(&g, &mut w, sp, &EventKind::KeyHeld(Action::Skip));
    assert!(w.save.is_none(), "not overlapping");
    let set = builtins::event_set(
        Builtin::Save,
        &g.def.kinds[usize::try_from(kind_id(&g, "sp").0).unwrap()],
    );
    assert_eq!(set.key_held, Action::Skip.bit());
    assert!(set.collisions.is_empty());
}

/// eat-the-lemon autosave (kid/B.md lemon item 7; traces/eat-the-lemon/famb-multikid-restart.md):
/// Create arms Alarm 0 for 1 tick, Alarm 0 saves the first kid's xprevious/yprevious, and the
/// point stays. With no kid there is no save.
#[test]
fn alarm_save_saves_the_first_kid_and_stays() {
    let g = save_game(params(&[
        ("on_touch", word("never")),
        ("saved_position", word("xy_previous")),
        ("alarm_save", real(1.0)),
        ("blink", ParamValue::Bool(false)),
    ]));
    let (mut w, sp, kid) = save_world(&g);
    let lower = spawn(&g, &mut w, g.kid, 48.0, 566.0);
    fire(&g, &mut w, sp, &EventKind::Create);
    assert_eq!(w.get(sp).unwrap().alarms[0], r(1.0));
    w.get_mut(kid).unwrap().m.set_xprevious(r(48.0)).unwrap();
    w.get_mut(kid).unwrap().m.set_yprevious(r(278.0)).unwrap();
    w.get_mut(lower).unwrap().m.set_xprevious(r(60.0)).unwrap();
    fire(&g, &mut w, sp, &EventKind::Alarm(0));
    let s = w.save.clone().unwrap();
    assert_eq!((s.x, s.y), (r(48.0), r(278.0)), "the lowest-id kid");
    assert!(w.get(sp).unwrap().alive, "the autosave stays");
    w.save = None;
    w.get_mut(kid).unwrap().alive = false;
    w.get_mut(lower).unwrap().alive = false;
    fire(&g, &mut w, sp, &EventKind::Alarm(0));
    assert!(w.save.is_none(), "no kid, no save");
    assert!(
        builtins::init_state(
            Builtin::Save,
            &params(&[("alarm_save", real(1.0)), ("self_destroy_alarm", real(5.0))])
        )
        .is_err()
    );
}

/// xia edgeDeath (kid/C.md save point User Event 0 and objautosave): with
/// `requires_kid_in_room` a touch save and the alarm save both need the kid's origin inside
/// the 800 by 608 room; the edges (x 800, y 0) are inside, x 800.5 and y -0.5 are not.
#[test]
fn requires_kid_in_room_gates_on_the_origin() {
    let g = save_game(params(&[
        ("on_touch", word("always")),
        ("requires_kid_in_room", yes()),
    ]));
    let (mut w, sp, kid) = save_world(&g);
    for (x, y, saves) in [
        (800.5, 10.0, false),
        (10.0, -0.5, false),
        (-0.5, 10.0, false),
        (10.0, 608.5, false),
        (800.0, 0.0, true),
        (0.0, 608.0, true),
    ] {
        w.save = None;
        w.get_mut(kid).unwrap().m.set_x(r(x)).unwrap();
        w.get_mut(kid).unwrap().m.set_y(r(y)).unwrap();
        fire(&g, &mut w, sp, &touch(kid));
        assert_eq!(w.save.is_some(), saves, "kid at ({x}, {y})");
    }
    let g = save_game(params(&[
        ("on_touch", word("never")),
        ("alarm_save", real(1.0)),
        ("requires_kid_in_room", yes()),
        ("blink", ParamValue::Bool(false)),
    ]));
    let (mut w, sp, kid) = save_world(&g);
    w.get_mut(kid).unwrap().m.set_y(r(-20.0)).unwrap();
    fire(&g, &mut w, sp, &EventKind::Alarm(0));
    assert!(w.save.is_none(), "the autosave skips a kid above the room");
    w.get_mut(kid).unwrap().m.set_y(r(20.0)).unwrap();
    fire(&g, &mut w, sp, &EventKind::Alarm(0));
    assert!(w.save.is_some());
}

#[test]
fn fixed_point_and_self_destroy_alarm() {
    let g = save_game(params(&[
        ("on_touch", word("always")),
        ("saved_position", word("fixed_point")),
        ("fixed_x", real(100.0)),
        ("fixed_y", real(200.0)),
        ("self_destroy_alarm", real(5.0)),
    ]));
    let (mut w, sp, kid) = save_world(&g);
    fire(&g, &mut w, sp, &touch(kid));
    let s = w.save.clone().unwrap();
    assert_eq!((s.x, s.y), (r(100.0), r(200.0)));
    assert_eq!(w.get(sp).unwrap().alarms[0], r(5.0));
    fire(&g, &mut w, sp, &EventKind::Alarm(0));
    assert!(!w.get(sp).unwrap().alive);
    assert!(
        builtins::init_state(Builtin::Save, &params(&[("saved_position", word("fixed_point"))])).is_err()
    );
}
