//! kid_start: creating the kid at Room Start, the grav gate, the player shifts and the
//! offset param.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, ParamValue, Params};
use iwbtg_sim::events::EventKind;

use crate::builtins_rely_a_support::*;

#[test]
fn kid_start_moves_itself_and_creates_the_kid_at_room_start() {
    let (g, mut w, start) = start_game_with(Params::default());
    // Create added (17, 23): rely room1's start (32, 544) gives the kid (49, 567).
    assert_eq!(pos(&w, start), (49.0, 567.0));
    fire(&g, &mut w, start, &EventKind::RoomStart);
    let kids = alive(&w, g.kid);
    assert_eq!(kids.len(), 1);
    assert_eq!(pos(&w, kids[0]), (49.0, 567.0));
    // A second start finds the kid and does nothing.
    fire(&g, &mut w, start, &EventKind::RoomStart);
    assert_eq!(alive(&w, g.kid).len(), 1);
}

#[test]
fn kid_start_with_grav_creates_nothing_when_gated() {
    let (g, mut w, start) = start_game_with(Params::default());
    w.engine.kid.grav = true;
    fire(&g, &mut w, start, &EventKind::RoomStart);
    assert!(alive(&w, g.kid).is_empty() && alive(&w, g.flipped).is_empty());
    // Ungated games (RORI, showdown) create the normal kid anyway.
    let (g, mut w, start) = start_game_with(params(&[("grav_gated", ParamValue::Bool(false))]));
    w.engine.kid.grav = true;
    fire(&g, &mut w, start, &EventKind::RoomStart);
    assert_eq!(alive(&w, g.kid).len(), 1);
}

#[test]
fn kid_start_player_or_player2_shifts_in_room_start() {
    let p = params(&[
        ("creates", word("player_or_player2")),
        ("shift_at", word("room_start")),
    ]);
    let (g, mut w, start) = start_game_with(p.clone());
    assert_eq!(pos(&w, start), (32.0, 544.0), "Create does not shift");
    fire(&g, &mut w, start, &EventKind::RoomStart);
    assert_eq!(pos(&w, alive(&w, g.kid)[0]), (49.0, 567.0));
    let (g, mut w, start) = start_game_with(p);
    w.engine.kid.grav = true;
    fire(&g, &mut w, start, &EventKind::RoomStart);
    let flipped = alive(&w, g.flipped);
    assert_eq!(flipped.len(), 1);
    assert_eq!(
        pos(&w, flipped[0]),
        (55.0, 561.0),
        "animus shifts by (23, 17) with grav 1"
    );
    // The gravity-dependent shift only works in Room Start.
    let bad = params(&[("creates", word("player_or_player2"))]);
    assert!(builtins::init_state(Builtin::KidStart, &bad).is_err());
}

#[test]
fn kid_start_offset_is_a_param() {
    let (g, mut w, start) = start_game_with(params(&[("offset_x", real(16.0))]));
    fire(&g, &mut w, start, &EventKind::RoomStart);
    assert_eq!(
        pos(&w, alive(&w, g.kid)[0]),
        (48.0, 567.0),
        "machinary shifts by (16, 23)"
    );
}
