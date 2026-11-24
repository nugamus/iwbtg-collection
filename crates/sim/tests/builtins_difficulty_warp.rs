//! difficulty_warp: the new game and the room_to and dif params.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, KindDef, ParamValue, Params};
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::world::{Flow, World};

use crate::builtins_rely_b_support::*;

#[test]
fn difficulty_warp_starts_a_new_game() {
    let g = game(|_| {
        let mut k = KindDef::new("dw", Builtin::DifficultyWarp);
        k.params = params(&[("dif", real(2.0))]);
        vec![k]
    });
    let mut w = World::new(g.room2, 1);
    w.save = Some(iwbtg_sim::save::SaveSlot {
        room: g.room2,
        x: r(1.0),
        y: r(1.0),
        precision: Default::default(),
        point: None,
        flipped: false,
        facing: 1,
        difficulty: 0,
        deaths: 4,
        time: 5,
        bosses: 0,
        items: 0,
        clear: false,
    });
    w.engine.run.deaths = 4;
    w.engine.run.time = 5;
    let dw = spawn(&g, &mut w, kind_id(&g, "dw"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
    spawn(&g, &mut w, g.bow, 5.0, 5.0);
    let flow = fire(&g, &mut w, dw, &touch(kid));
    assert_eq!(flow, Flow::AbortTick);
    assert_eq!(w.engine.run.difficulty, 2);
    assert!(w.save.is_none(), "the save is deleted");
    assert_eq!((w.engine.run.deaths, w.engine.run.time), (0, 0));
    assert_eq!(
        w.pending,
        Some(RoomRequest::Goto(g.room)),
        "the game's new-game room"
    );
    assert!(alive(&w, g.kid).is_empty() && alive(&w, g.bow).is_empty());
}

#[test]
fn difficulty_warp_room_to_overrides_and_dif_is_required() {
    let g = game(|room2| {
        let mut k = KindDef::new("dw", Builtin::DifficultyWarp);
        k.params = params(&[("dif", real(0.0)), ("room_to", ParamValue::Room(room2))]);
        vec![k]
    });
    let mut w = World::new(g.room, 1);
    let dw = spawn(&g, &mut w, kind_id(&g, "dw"), 0.0, 0.0);
    let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
    fire(&g, &mut w, dw, &touch(kid));
    assert_eq!(w.pending, Some(RoomRequest::Goto(g.room2)));
    assert!(builtins::init_state(Builtin::DifficultyWarp, &Params::default()).is_err());
    assert!(builtins::init_state(Builtin::DifficultyWarp, &params(&[("dif", real(1.5))])).is_err());
}
