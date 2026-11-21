//! Restart and reset: the boot-room timeline, saved-room goto, recreate in place, reset and
//! start from a save.

use iwbtg_sim::def::{
    DeathOnRestart, ResetKind, RestartKind, RoleSet, RoomRole, SoundDef, SoundId, SoundKind,
};
use iwbtg_sim::num::Real;
use iwbtg_sim::restart::{boot_step, run_reset, run_restart};
use iwbtg_sim::room::{RoomRequest, StartPoint, perform_change, start_game};
use iwbtg_sim::run_globals::BootLoad;
use iwbtg_sim::world::{Flow, SimStatus, World};

use crate::state_support::*;

#[test]
fn restart_via_boot_runs_the_n_n1_n2_timeline() {
    let g = game(|d| d.restart.death_on_restart = DeathOnRestart::SaveCounters);
    let mut w = World::new(g.play, 1);
    w.save = Some(slot(g.play));
    w.engine.run.deaths = 5;
    let kid = spawn(&g, &mut w, g.kid, 300.0, 300.0);
    // Tick N: the R handler.
    let flow = with(&g, &mut w, |c| run_restart(c, kid).unwrap());
    assert_eq!(flow, Flow::AbortTick);
    assert_eq!(w.pending, Some(RoomRequest::GameRestart));
    assert_eq!(w.engine.run.deaths, 6, "a retry counts a death");
    assert_eq!(
        w.save.as_ref().unwrap().deaths,
        6,
        "and writes the death-only save"
    );
    assert_eq!(w.engine.run.boot_load, Some(BootLoad { ticks_left: 1 }));
    let k = w.get(kid).unwrap();
    assert_eq!(
        (k.m.x(), k.m.y()),
        (r(49.0), r(119.0)),
        "the old kid already stands at the saved spot"
    );
    // The restart: everything goes, the temp marker stays, boot instances come back.
    with(&g, &mut w, |c| {
        perform_change(c, RoomRequest::GameRestart).unwrap()
    });
    assert_eq!(w.room, g.boot);
    assert!(alive(&w, g.kid).is_empty());
    assert_eq!(w.engine.run.deaths, 0, "globals do not survive game_restart");
    assert_eq!(w.engine.run.boot_load, Some(BootLoad { ticks_left: 1 }));
    // Tick N+1: the boot tick loads.
    let flow = with(&g, &mut w, |c| boot_step(c).unwrap());
    assert_eq!(flow, Flow::AbortTick);
    assert_eq!(w.pending, Some(RoomRequest::Goto(g.play)));
    assert_eq!(w.engine.run.boot_load, None);
    let kids = alive(&w, g.kid);
    assert_eq!(kids.len(), 1);
    assert_eq!(w.get(kids[0]).unwrap().m.xprevious(), r(0.0));
    assert_eq!(
        w.engine.run.deaths, 6,
        "the load restores the counters from the save"
    );
    // Tick N+2: the saved room, kid carried in.
    with(&g, &mut w, |c| {
        perform_change(c, RoomRequest::Goto(g.play)).unwrap()
    });
    assert_eq!(w.room, g.play);
    assert_eq!(alive(&w, g.kid), kids);
}

#[test]
fn boot_ticks_count_down_and_zero_is_unsupported() {
    let g = game(|d| d.restart.boot_ticks = 3);
    let mut w = World::new(g.play, 1);
    w.save = Some(slot(g.play));
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    with(&g, &mut w, |c| run_restart(c, kid).unwrap());
    for left in [2, 1] {
        let flow = with(&g, &mut w, |c| boot_step(c).unwrap());
        assert_eq!(flow, Flow::Continue);
        assert_eq!(w.engine.run.boot_load, Some(BootLoad { ticks_left: left }));
    }
    assert_eq!(with(&g, &mut w, |c| boot_step(c).unwrap()), Flow::AbortTick);

    let g0 = game(|d| d.restart.boot_ticks = 0);
    let mut w = World::new(g0.play, 1);
    w.save = Some(slot(g0.play));
    let kid = spawn(&g0, &mut w, g0.kid, 0.0, 0.0);
    assert!(with(&g0, &mut w, |c| run_restart(c, kid)).is_err());
}

#[test]
fn restart_without_a_save_is_a_plain_game_restart() {
    let g = game(|_| {});
    let mut w = World::new(g.play, 1);
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    with(&g, &mut w, |c| run_restart(c, kid).unwrap());
    assert_eq!(w.pending, Some(RoomRequest::GameRestart));
    assert_eq!(w.engine.run.boot_load, None);
}

#[test]
fn restart_does_nothing_in_an_excluded_role() {
    let g = game(|d| d.restart.excluded_roles = RoleSet(RoomRole::Menu.bit()));
    let mut w = World::new(g.menu, 1);
    w.save = Some(slot(g.play));
    let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    let flow = with(&g, &mut w, |c| run_restart(c, kid).unwrap());
    assert_eq!((flow, w.pending), (Flow::Continue, None));
}

#[test]
fn room_goto_saved_recreates_the_kid_and_drops_the_bow() {
    let g = game(|d| d.restart.kind = RestartKind::RoomGotoSaved);
    let mut w = World::new(g.play, 1);
    w.save = Some(slot(g.boot));
    let old = spawn(&g, &mut w, g.kid, 300.0, 300.0);
    let bow = spawn(&g, &mut w, g.bow, 300.0, 300.0);
    with(&g, &mut w, |c| run_restart(c, old).unwrap());
    // The ids destroyed this tick travel with the change: the restart does not create placed
    // instances with them again (#670).
    match w.pending {
        Some(RoomRequest::GotoAfterRemovals { room, removed }) => {
            assert_eq!(room, g.boot);
            assert!(removed.contains(old) && removed.contains(bow), "{removed:?}");
        }
        ref other => panic!("expected GotoAfterRemovals, got {other:?}"),
    }
    assert!(alive(&w, g.bow).is_empty());
    let kids = alive(&w, g.kid);
    assert_eq!(kids.len(), 1);
    assert_ne!(kids[0], old);
    assert_eq!(w.get(kids[0]).unwrap().m.x(), r(49.0));
}

#[test]
fn room_goto_saved_skips_the_goto_in_a_menu_role() {
    let g = game(|d| d.restart.kind = RestartKind::RoomGotoSaved);
    let mut w = World::new(g.menu, 1);
    w.save = Some(slot(g.play));
    let old = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    with(&g, &mut w, |c| run_restart(c, old).unwrap());
    assert_eq!(w.pending, None);
    assert_eq!(alive(&w, g.kid).len(), 1);
}

#[test]
fn showdown_load_never_restarts_the_game() {
    // R26 (settled): showdown's R has no game_restart.
    let g = game(|d| d.restart.kind = RestartKind::ShowdownLoad);
    let mut w = World::new(g.play, 1);
    w.save = Some(slot(g.play));
    let old = spawn(&g, &mut w, g.kid, 5.0, 5.0);
    with(&g, &mut w, |c| run_restart(c, old).unwrap());
    match w.pending {
        Some(RoomRequest::GotoAfterRemovals { room, removed }) => {
            assert_eq!(room, g.play);
            assert!(removed.contains(old), "{removed:?}");
        }
        ref other => panic!("expected GotoAfterRemovals, got {other:?}"),
    }
    assert_eq!(w.engine.run.boot_load, None);
    assert_eq!(alive(&w, g.kid).len(), 1);
    assert_ne!(alive(&w, g.kid)[0], old);
}

#[test]
fn recreate_in_place_puts_the_kid_at_the_saved_spot_now() {
    let g = game(|d| d.restart.kind = RestartKind::RecreateInPlace);
    let mut w = World::new(g.play, 1);
    w.save = Some(slot(g.boot));
    let old = spawn(&g, &mut w, g.kid, 5.0, 5.0);
    with(&g, &mut w, |c| run_restart(c, old).unwrap());
    let kids = alive(&w, g.kid);
    assert_eq!(kids.len(), 1);
    assert_eq!(w.room, g.play, "still in the current room");
    assert_eq!(w.get(kids[0]).unwrap().m.x(), r(49.0));
    match w.pending {
        Some(RoomRequest::GotoAfterRemovals { room, removed }) => {
            assert_eq!(room, g.boot);
            assert!(removed.contains(old), "{removed:?}");
        }
        ref other => panic!("expected GotoAfterRemovals, got {other:?}"),
    }
}

#[test]
fn reset_deletes_the_marker_and_restarts_or_goes_to_the_boot_room() {
    let g = game(|_| {});
    let mut w = World::new(g.play, 1);
    w.engine.run.boot_load = Some(BootLoad { ticks_left: 1 });
    let any = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    with(&g, &mut w, |c| run_reset(c, any).unwrap());
    assert_eq!(
        (w.pending, w.engine.run.boot_load),
        (Some(RoomRequest::GameRestart), None)
    );

    let g = game(|d| d.reset.kind = ResetKind::GameReset);
    let mut w = World::new(g.play, 1);
    w.engine.run.deaths = 9;
    let any = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    with(&g, &mut w, |c| run_reset(c, any).unwrap());
    assert_eq!(w.pending, Some(RoomRequest::Goto(g.boot)));
    assert_eq!(w.engine.run.deaths, 0);
}

#[test]
fn start_from_save_still_works_with_the_real_load() {
    let g = game(|_| {});
    let mut w = World::new(g.boot, 1);
    let s = slot(g.play);
    let r = with(&g, &mut w, |c| start_game(c, &StartPoint::FromSave(s.clone())));
    r.unwrap();
    assert_eq!(w.status, SimStatus::Running);
    assert_eq!(w.room, g.play);
    assert_eq!(alive(&w, g.kid).len(), 1);
}

/// F2 in Groups A, B and C stops every sound before game_restart (CLONE_SPEC rooms; kid/A4.md).
#[test]
fn f2_stops_all_sounds_before_game_restart() {
    let g = game(|d| {
        d.sounds.push(SoundDef {
            name: "track".into(),
            kind: SoundKind::Multimedia,
            length_ticks: 100,
            volume: Real::ONE,
            pan: Real::ZERO,
        });
    });
    let mut w = World::new(g.play, 1);
    let any = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    with(&g, &mut w, |c| {
        iwbtg_sim::sound::play(c, SoundId(0), true).unwrap();
    });
    assert_eq!(w.sound.copies.len(), 1);
    with(&g, &mut w, |c| run_reset(c, any).unwrap());
    assert!(
        w.sound.copies.is_empty(),
        "the F2 handler stops all sounds before game_restart"
    );
    assert_eq!(w.pending, Some(RoomRequest::GameRestart));
}

/// The kaenbyou / eat-the-lemon gameReset is recorded as "resets globals and calls
/// room_goto(rInit)" only, with no sound stop; #225 asks research for its full list.
#[test]
fn game_reset_keeps_sounds_until_its_script_is_traced() {
    let g = game(|d| {
        d.reset.kind = ResetKind::GameReset;
        d.sounds.push(SoundDef {
            name: "track".into(),
            kind: SoundKind::Multimedia,
            length_ticks: 100,
            volume: Real::ONE,
            pan: Real::ZERO,
        });
    });
    let mut w = World::new(g.play, 1);
    let any = spawn(&g, &mut w, g.kid, 0.0, 0.0);
    with(&g, &mut w, |c| {
        iwbtg_sim::sound::play(c, SoundId(0), true).unwrap();
    });
    with(&g, &mut w, |c| run_reset(c, any).unwrap());
    assert_eq!(
        w.sound.copies.len(),
        1,
        "gameReset has no recorded sound stop; a trace request is filed (#225)"
    );
    assert_eq!(w.pending, Some(RoomRequest::Goto(g.boot)));
}
