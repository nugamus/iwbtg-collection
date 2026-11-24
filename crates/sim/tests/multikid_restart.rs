//! #670: R while a room's placed kids are alive leaves one kid (eat-the-lemon room13,
//! famb-multikid-restart). The original keeps instances destroyed earlier in the frame pending
//! removal, so the room restart does not create its placed kids again; R after those kids are
//! gone gives placed + 1.

use std::sync::Arc;

use iwbtg_sim::builtins;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, EngineKind, KidFamily, KindDef, RestartKind, RoomDef};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::{self, presets};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::world::{KindState, SimStatus};
use iwbtg_sim::{Sim, SimOptions};

fn bits(actions: &[Action]) -> u32 {
    actions.iter().fold(0u32, |acc, a| acc | a.bit())
}

fn frame(held: &[Action], pressed: &[Action], released: &[Action]) -> InputFrame {
    InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: bits(released),
    }
}

fn kids(sim: &Sim) -> usize {
    sim.world()
        .iter_alive()
        .filter(|i| matches!(i.state, KindState::Kid(_)))
        .count()
}

/// A room with `placed` kids (room-file id and position). The first kid's first-play save is the
/// checkpoint a retry returns to.
fn sim_with_placed(placed: &[(u32, f64, f64)]) -> Sim {
    let mut b = GameDefBuilder::new("multikid-restart");
    let mut kid = KindDef::new("kid", Builtin::Kid);
    kid.classes = ClassMask::KID;
    let kid_id = b.kind(kid);
    b.engine_kind(EngineKind::Kid, kid_id);
    let mut world = KindDef::new("world", Builtin::World);
    world.persistent = true;
    let world_id = b.kind(world);
    b.boot(world_id);
    let play = b.room(RoomDef::new("room13", 800, 608));
    b.start_room(play);
    for &(id, x, y) in placed {
        b.place(play, kid_id, x, y).unwrap().gm_id = Some(id);
    }
    let physics = presets::defaults(KidFamily::B);
    b.physics(physics.clone());
    let mut def = b.build().expect("game builds");
    // eat-the-lemon's retry: recreate the kid and go to the saved room, in this tick.
    def.restart.kind = RestartKind::RoomGotoSaved;
    for k in &mut def.kinds {
        k.events = if k.base == Builtin::Kid {
            kid::event_set(&physics)
        } else {
            builtins::event_set(k.base, k)
        };
    }
    Sim::new(
        Arc::new(def),
        SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed: 0,
            dll_mode: DllMode::Stub,
            mode: PlayMode::Original,
            map: None,
        },
    )
    .expect("sim starts")
}

/// Room13's two placed kids, with the room-file ids the trace records (104180 upper, 104190
/// lower).
fn sim() -> Sim {
    sim_with_placed(&[(104_180, 48.0, 278.0), (104_190, 48.0, 566.0)])
}

#[test]
fn r_while_the_placed_kids_are_alive_leaves_one_kid() {
    let mut sim = sim();
    sim.step(InputFrame::default());
    assert_eq!(kids(&sim), 2, "the room's two placed kids");
    let save = sim.world().save.clone().expect("the first-play save");
    assert_eq!(save.room, sim.world().room);

    // First R: both placed kids are alive. The retry destroys them and enters the saved room in
    // the same tick; the placed kids are pending removal and are not created again.
    sim.step(frame(&[], &[Action::Restart], &[]));
    sim.step(frame(&[], &[], &[Action::Restart]));
    assert_eq!(
        kids(&sim),
        1,
        "only the loaded kid: the placed kids were pending removal (#670)"
    );

    // Let the room settle with no placed kid alive.
    for _ in 0..3 {
        sim.step(InputFrame::default());
    }
    assert_eq!(kids(&sim), 1);

    // Second R: no placed kid is alive this time, so the restart creates both placed kids and
    // carries the loaded one: placed + 1.
    sim.step(frame(&[], &[Action::Restart], &[]));
    sim.step(frame(&[], &[], &[Action::Restart]));
    assert_eq!(kids(&sim), 3, "placed + 1 after a retry with no placed kid alive");
}

/// More destroyed instances than the request can carry is a checked halt, not a truncation that
/// would leave some placed kids behind.
#[test]
fn more_than_the_removal_limit_halts_instead_of_leaving_kids_behind() {
    let placed: Vec<(u32, f64, f64)> = (0..17)
        .map(|i| (104_180 + u32::try_from(i).unwrap(), 48.0, 278.0))
        .collect();
    let mut sim = sim_with_placed(&placed);
    sim.step(InputFrame::default());
    assert_eq!(kids(&sim), 17);
    sim.step(frame(&[], &[Action::Restart], &[]));
    let SimStatus::Halted(e) = &sim.world().status else {
        panic!("expected a halt, got {:?}", sim.world().status);
    };
    assert!(e.to_string().contains("at most 16"), "{e}");
}
