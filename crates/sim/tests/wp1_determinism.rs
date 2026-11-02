//! WP1 done-when: a probe world runs 10,000 ticks with identical hashes (docs/plans/sim-core.md
//! WP1, 10.6, 12).
//!
//! The world spawns, destroys, sets alarms, reads keys, writes globals and changes rooms, so every
//! part of the state hash moves. Two runs in one process must match each other. Both must also
//! match `FINAL_HASH`, which pins the result so the same test checks identity across the four
//! targets of 10.6 when CI runs it on each one (#12).

#[path = "wp1_support.rs"]
mod support;

use iwbtg_sim::def::{RoomId, VarId};
use iwbtg_sim::events::{ProbeAct, ProbeOn, ProbeState, ProbeStep};
use iwbtg_sim::hash::{self, CHECKPOINT_INTERVAL};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::world::Value;
use support::*;

const TICKS: u64 = 10_000;
const COUNTER: VarId = VarId(10);

/// `state_hash` after the last tick, as lowercase hex. Recorded on Windows x86_64. A deliberate
/// change to the sim or to `HASH_LAYOUT` updates it in the same PR.
const FINAL_HASH: &str = "d7d079952f600ba8d5aba68747968f1aaff9a52cfe395fd77269d31f8399fc10";

fn child(tag: &str) -> ProbeState {
    let mut s = ProbeState::new(tag, None);
    s.steps = vec![
        ProbeStep::on(ProbeOn::Create, ProbeAct::SetAlarm(0, r(7.0))),
        ProbeStep::on(ProbeOn::Alarm(0), ProbeAct::DestroySelf),
        ProbeStep::on(ProbeOn::Step, ProbeAct::SetGlobal(COUNTER, vr(1.0))).every(3),
    ];
    s
}

fn world() -> Fx {
    let mut b = builder();
    let parent = probe_kind(&mut b, "parent");
    let kid = probe_kind(&mut b, "child");
    let mut fx = Fx::new(b.build().expect("def"));
    let mut s = ProbeState::new("P", None);
    s.steps = vec![
        ProbeStep::on(
            ProbeOn::Step,
            ProbeAct::Spawn {
                kind: kid,
                x: r(100.5),
                y: r(-3.25),
                child: Some(Box::new(child("C"))),
            },
        )
        .every(5),
        ProbeStep::on(ProbeOn::BeginStep, ProbeAct::DirectRead(Action::Jump.bit())).every(2),
        ProbeStep::on(ProbeOn::KeyPress, ProbeAct::SetGlobal(COUNTER, vr(2.0))),
        ProbeStep::on(
            ProbeOn::EndStep,
            ProbeAct::RequestRoom(RoomRequest::Goto(RoomId(1))),
        )
        .every(1000),
        ProbeStep::on(
            ProbeOn::EndStep,
            ProbeAct::RequestRoom(RoomRequest::Goto(RoomId(0))),
        )
        .every(1500),
    ];
    fx.probe(parent, 0.0, 0.0, s);
    fx
}

/// The input of tick `t`: Jump held in a 7-tick pattern, Right in a 13-tick pattern.
fn input(t: u64) -> InputFrame {
    let held = |t: u64| -> u32 {
        let mut h = 0;
        if t % 7 < 3 {
            h |= Action::Jump.bit();
        }
        if t % 13 < 9 {
            h |= Action::Right.bit();
        }
        h
    };
    InputFrame::from_held(if t == 0 { 0 } else { held(t - 1) }, held(t))
}

fn run() -> (Vec<[u8; 16]>, [u8; 32]) {
    let mut fx = world();
    let mut checkpoints = Vec::new();
    for t in 0..TICKS {
        fx.tick(input(t));
        if let Some(c) = hash::checkpoint(&fx.w) {
            checkpoints.push(c);
        }
    }
    assert_eq!(fx.w.tick, TICKS, "every tick ran without halting");
    assert!(fx.w.iter_alive().count() > 1, "children are alive at the end");
    assert!(
        matches!(fx.w.globals.get(&ROOM_CHANGES), Some(Value::Real(n)) if n.get() > 0.0),
        "the room changed"
    );
    (checkpoints, hash::state_hash(&fx.w))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// On wasm32-unknown-unknown the libtest harness does not exist; wasm-bindgen-test runs the test
// under node (#206). Everywhere else this is a plain #[test].
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;

#[test]
fn wp1_probe_world_10000_ticks_hashes_are_identical() {
    let (a_points, a_final) = run();
    let (b_points, b_final) = run();
    assert_eq!(
        a_points.len() as u64,
        TICKS / CHECKPOINT_INTERVAL,
        "a checkpoint every 500 ticks"
    );
    assert_eq!(a_points, b_points, "checkpoint hashes differ between two runs");
    assert_eq!(a_final, b_final, "final hash differs between two runs");
    assert_eq!(
        hex(&a_final),
        FINAL_HASH,
        "final hash differs from the recorded one"
    );
}
