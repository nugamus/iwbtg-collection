//! #466: the first room's start runs inside `Sim::new`; its output is kept by the first step and
//! cleared by the second.

use iwbtg_sim::def::RoomId;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, EngineKind, KindDef, RoomDef};
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::room::RoomOutput;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::{Sim, SimOptions, StartPoint};
use std::sync::Arc;

fn sim() -> (Sim, RoomId) {
    let mut b = GameDefBuilder::new("first-step-output");
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    b.engine_kind(EngineKind::Kid, kid);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let def = b.build().expect("game builds");
    let options = SimOptions {
        start: StartPoint::Room { room, difficulty: 0 },
        seed: 1,
        dll_mode: DllMode::Stub,
        mode: iwbtg_sim::stripped::PlayMode::Original,
        map: None,
    };
    (Sim::new(Arc::new(def), options).expect("sim starts"), room)
}

#[test]
fn the_first_step_keeps_the_room_start_output() {
    let (mut sim, room) = sim();
    let entered = OutputEvent::Room(RoomOutput::Entered { from: None, to: room });
    assert!(
        sim.world().output.contains(&entered),
        "Sim::new leaves the room start in output"
    );
    sim.step(InputFrame::default());
    assert!(sim.world().output.contains(&entered), "the first step keeps it");
    sim.step(InputFrame::default());
    assert!(
        !sim.world().output.contains(&entered),
        "the second step starts clean"
    );
}
