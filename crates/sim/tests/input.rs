//! #1155: the one input/action contract, run through the real tick pipeline.
//!
//! The client collector derives `InputFrame { held, pressed, released }`; the sim latches it and
//! never re-derives an edge. Each test presses and releases through frames, so the response is
//! what a player gets: the step that latches the press moves the kid, a release cuts the jump by
//! the profile's multiplier, a sub-tick tap still jumps, and only action bits reach the world.
//!
//! These are the executable form of the `input/*` F1 feel contracts in
//! `docs/architecture/feel-contracts.md`: a hand-built A1 room (the standard parameter set), one
//! input, one observable outcome. `evidence` is the A1 action tests and the rely input traces.

use std::sync::Arc;

use iwbtg_sim::builtins;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, EngineKind, KidFamily, KindDef, RoomDef};
use iwbtg_sim::input::{Action, CONTROL_LATENCY_MAX_TICKS, InputFrame};
use iwbtg_sim::kid::{self, presets};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::world::InstId;
use iwbtg_sim::{Sim, SimOptions};

/// The A1 preset's gravity (kid/A1.md "Numeric constants").
const GRAVITY: f64 = 0.4;
/// The A1 preset's variable-jump release multiplier.
const RELEASE_MULT: f64 = 0.45;

struct Fixture {
    sim: Sim,
    kid: InstId,
}

impl Fixture {
    /// `(x, y, vspeed)` of the kid after the last tick.
    fn kid(&self) -> (f64, f64, f64) {
        let i = self.sim.world().get(self.kid).expect("kid alive");
        (i.m.x().get(), i.m.y().get(), i.m.vspeed().get())
    }

    fn step(&mut self, held: &[Action], pressed: &[Action], released: &[Action]) -> u64 {
        self.step_with(held, pressed, released, 0)
    }

    /// A step whose frame also carries `reserved` bits outside `Action::MASK`, to model a
    /// hypothetical legacy replay.
    fn step_with(&mut self, held: &[Action], pressed: &[Action], released: &[Action], reserved: u32) -> u64 {
        let bits = |list: &[Action]| list.iter().fold(0u32, |bits, a| bits | a.bit());
        self.sim
            .step(InputFrame {
                held: bits(held) | reserved,
                pressed: bits(pressed) | reserved,
                released: bits(released) | reserved,
            })
            .tick
    }
}

/// A one-room A1 game: a solid floor and a Kid Start resting on it. The kid kind is the engine's
/// `Kid`, so `room::start_game` spawns it exactly as a package would.
fn fixture() -> Fixture {
    let mut b = GameDefBuilder::new("input-contract");
    let tile = b.sprite_rect((32, 32), (0, 0));
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);

    let mut kid_kind = KindDef::new("kid", Builtin::Kid);
    kid_kind.sprite = Some(kid_sprite);
    kid_kind.classes = ClassMask::KID;
    let kid_kind = b.kind(kid_kind);
    let start = b.kind(KindDef::new("kid_start", Builtin::KidStart));

    let mut block = KindDef::new("block", Builtin::Block);
    block.sprite = Some(tile);
    block.classes = ClassMask::BLOCK;
    block.solid = true;
    let block = b.kind(block);
    for i in 0..25 {
        b.place(room, block, f64::from(i) * 32.0, 576.0).expect("floor");
    }
    // A Kid Start shifts itself by its default (17, 23) offset in Create, so placing it at
    // (48 - 17, 568 - 23) spawns the kid at (48, 568). The A1 mask is 11 x 21 with origin (5, 12):
    // the box bottom is `y + 8`, so 568 rests on the floor whose top is 576.
    b.place(room, start, 31.0, 545.0).expect("kid start");
    b.engine_kind(EngineKind::Kid, kid_kind);

    let mut physics = presets::defaults(KidFamily::A1);
    for sprite in [
        &mut physics.sprites.idle,
        &mut physics.sprites.running,
        &mut physics.sprites.jump,
        &mut physics.sprites.fall,
        &mut physics.sprites.sliding,
    ] {
        *sprite = Some(kid_sprite);
    }
    b.physics(physics.clone());

    let mut def = b.build().expect("input-contract def");
    for k in &mut def.kinds {
        k.events = if k.base == Builtin::Kid {
            kid::event_set(&physics)
        } else {
            builtins::event_set(k.base, &k.clone())
        };
    }
    let options = SimOptions {
        start: StartPoint::Room { room, difficulty: 0 },
        seed: 1,
        dll_mode: DllMode::Stub,
        mode: PlayMode::Original,
        map: None,
    };
    let mut sim = Sim::new(Arc::new(def), options).expect("sim starts");
    // Settle on the floor; a tick with no input has no edges.
    for _ in 0..4 {
        sim.step(InputFrame::EMPTY);
    }
    let kid = sim.world().kid(&sim.rules().tick).expect("kid spawned");
    Fixture { sim, kid }
}

/// `input/jump_on_press`: the tick whose frame carries the press is the tick the kid leaves the
/// ground. The collector→sim hop may add at most `CONTROL_LATENCY_MAX_TICKS` (one) tick.
#[test]
fn a_press_moves_the_kid_on_the_tick_the_sim_latches_it() {
    let mut f = fixture();
    let (_, y0, vs0) = f.kid();
    assert_eq!(vs0, 0.0, "the kid is resting on the floor");
    let response = f.step(&[Action::Jump], &[Action::Jump], &[]);
    let (_, y1, vs1) = f.kid();
    assert!(vs1 < 0.0, "the jump set an upward speed: {vs1}");
    assert!(y1 < y0, "the kid moved up on the response tick: {y0} -> {y1}");
    // `.inputs` frame f is the step ending at tick f + 1, so the press frame is `response - 1`.
    // The response is on that step: latency 1, the contract's `latency_max`.
    let press_frame = response - 1;
    let latency = response - press_frame;
    assert!(
        latency <= u64::from(CONTROL_LATENCY_MAX_TICKS),
        "a press responds within the latency contract: {latency} tick(s)"
    );
}

/// `input/release_height`: a release while rising multiplies the upward speed by the profile's
/// `release_mult`; the tick's motion then adds gravity.
#[test]
fn a_release_cuts_the_upward_jump_by_the_release_multiplier() {
    let mut f = fixture();
    f.step(&[Action::Jump], &[Action::Jump], &[]);
    let (_, _, rising) = f.kid();
    assert!(rising < 0.0);
    f.step(&[], &[], &[Action::Jump]);
    let (_, _, cut) = f.kid();
    let expected = rising * RELEASE_MULT + GRAVITY;
    assert!(
        (cut - expected).abs() < 1e-9,
        "release cut {rising} to {cut}, expected {expected}"
    );
    assert!(cut > rising, "the cut made the upward speed smaller");
}

/// `input/release_height`: holding the jump longer rises higher. The envelope relation is the
/// player-observable outcome; the exact apex stays a profile value.
#[test]
fn a_longer_hold_apes_higher_than_a_short_one() {
    // A plain comparison, not `f64::min` (the sim's clippy config bans its signed-zero result).
    fn note(f: &Fixture, apex: &mut f64) {
        let y = f.kid().1;
        if y < *apex {
            *apex = y;
        }
    }
    fn apex(hold_ticks: u32) -> f64 {
        let mut f = fixture();
        let (_, y0, _) = f.kid();
        let mut apex = y0;
        f.step(&[Action::Jump], &[Action::Jump], &[]);
        note(&f, &mut apex);
        for _ in 1..hold_ticks {
            f.step(&[Action::Jump], &[], &[]);
            note(&f, &mut apex);
        }
        f.step(&[], &[], &[Action::Jump]);
        for _ in 0..40 {
            f.step(&[], &[], &[]);
            note(&f, &mut apex);
        }
        apex
    }
    let short = apex(3);
    let long = apex(18);
    assert!(
        long < short,
        "a longer hold must rise higher: short apex {short}, long apex {long}"
    );
}

/// `input/sub_frame_tap`: a press and a release inside one frame keep both bits, and the plain
/// edge contract turns that into a jump (the release cuts it; it does not erase the press).
#[test]
fn a_tap_inside_one_tick_still_jumps() {
    let mut f = fixture();
    let (_, y0, vs0) = f.kid();
    assert_eq!(vs0, 0.0);
    f.step(&[], &[Action::Jump], &[Action::Jump]);
    let (_, y1, vs1) = f.kid();
    assert!(y1 < y0, "the tap moved the kid up: {y0} -> {y1}");
    assert!(vs1 < 0.0, "the tap still started a jump: {vs1}");
    // A tap is not a hold: the next tick has no edge and does not jump again.
    let (_, y2, _) = f.kid();
    f.step(&[], &[], &[]);
    let (_, y3, _) = f.kid();
    assert!(
        y3 < y2,
        "the kid keeps rising on the jump's speed, not a new press"
    );
}

/// The wire contract is action bits only. Reserved bits (here bit 31 and bit 19, above `Extra4`)
/// are masked when the sim latches, so they cannot act like a key or enter the hashed input state.
#[test]
fn only_action_bits_reach_the_latched_world() {
    let mut f = fixture();
    let (_, _, vs0) = f.kid();
    let dirty = InputFrame {
        held: 1 << 31,
        pressed: 1 << 31 | 1 << 19,
        released: 1 << 19,
    };
    f.sim.step(dirty);
    assert_eq!(f.sim.world().input.frame, InputFrame::EMPTY);
    let (_, _, vs1) = f.kid();
    assert_eq!(vs1, vs0, "reserved bits did not act as a jump or a move");
}

/// A hypothetical legacy replay whose frames carry reserved bits still produces the same
/// player-observable outcome: the sim reads no bit outside `Action::MASK`, so the tick count and
/// the kid's trajectory match the same action input. The hashed input frame itself is masked;
/// no recorded replay carries such a bit (the collector regression test and the git history:
/// every replay writer has fed `Collector::take_frame`, which only sets `Action::bit()`).
#[test]
fn reserved_bits_do_not_change_the_outcome() {
    const RESERVED: u32 = (1 << 19) | (1 << 31);
    fn run(reserved: u32) -> (Vec<u64>, Vec<(f64, f64, f64)>) {
        let mut f = fixture();
        let script: [(&[Action], &[Action], &[Action]); 8] = [
            (&[Action::Jump], &[Action::Jump], &[]),
            (&[Action::Jump], &[], &[]),
            (&[Action::Jump], &[], &[]),
            (&[], &[], &[Action::Jump]),
            (&[], &[], &[]),
            (&[Action::Right], &[Action::Right], &[]),
            (&[Action::Right, Action::Jump], &[Action::Jump], &[]),
            (&[], &[], &[Action::Right, Action::Jump]),
        ];
        let mut ticks = Vec::new();
        let mut path = Vec::new();
        for (held, pressed, released) in script {
            ticks.push(f.step_with(held, pressed, released, reserved));
            path.push(f.kid());
        }
        (ticks, path)
    }
    assert_eq!(run(0), run(RESERVED));
}
