//! The golden corpus: small engine worlds we build ourselves (no creator content), each run for a
//! fixed number of ticks on a fixed input, with one pinned state hash per case
//! (docs/plans/sim-core.md 10.6, docs/architecture/testing.md).
//!
//! Two programs include this file with `#[path]`, so it uses only `iwbtg_sim`'s public API and
//! `std`:
//!
//! - `tests/golden_corpus.rs` checks every case against `expected.txt` on every CI target;
//! - `iwbtg determinism report <corpus>` (crates/tools) prints the same lines, and CI compares the
//!   output of Windows, Linux and wasm32-wasip1 byte for byte.
//!
//! A deliberate change to the sim or to `HASH_LAYOUT` re-pins `expected.txt` in the same PR
//! (`UPDATE_GOLDEN=1 cargo test -p iwbtg-sim --test golden_corpus`).
//!
//! Every case drives probe kinds (`Builtin::Probe`) through the real tick pipeline: real motion,
//! collision, rooms and room changes.
#![allow(
    dead_code,
    reason = "the two programs that include this file use different parts"
)]

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, GameDef, KindDef, KindId, RoomDef, RoomId, ScriptDef, ScriptId, VarId,
};
use iwbtg_sim::events::{ProbeAct, ProbeOn, ProbeState, ProbeStep, probe_event_set};
use iwbtg_sim::hash;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::num::Real;
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::tick;
use iwbtg_sim::world::{Ctx, InstId, KindState, SimStatus, SpawnInit, Value, World};

/// The file with the pinned lines, next to this module.
pub const EXPECTED_FILE: &str = "expected.txt";

/// What a case produced.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Outcome {
    /// Case name.
    pub name: &'static str,
    /// Ticks the world ran (fewer than asked when it halted).
    pub ticks: u64,
    /// `running`, or `halted: <error>`.
    pub status: String,
    /// `hash::state_hash` after the last tick, lowercase hex.
    pub hash: String,
}

impl Outcome {
    /// `name ticks status hash`, the line the report prints and `expected.txt` pins.
    pub fn line(&self) -> String {
        format!(
            "{} {} {} {}",
            self.name,
            self.ticks,
            self.status.replace(' ', "_"),
            self.hash
        )
    }
}

/// One case: a name and a function that builds the world and says how long to run.
pub struct Case {
    /// Unique, lowercase, no spaces.
    pub name: &'static str,
    /// Ticks to run.
    pub ticks: u64,
    /// Builds the game and puts the first instances into room 0.
    pub build: fn() -> Fixture,
    /// The input of tick `t`.
    pub input: fn(u64) -> InputFrame,
}

/// All cases, in report order.
pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "spawn_alarms_rooms",
            ticks: 10_000,
            build: spawn_alarms_rooms,
            input: pattern_input,
        },
        Case {
            name: "motion_fractions",
            ticks: 4_000,
            build: motion_fractions,
            input: no_input,
        },
        Case {
            name: "keys_random",
            ticks: 6_000,
            build: keys,
            input: random_input,
        },
        Case {
            name: "scripted_object",
            ticks: 3_000,
            build: scripted_object,
            input: no_input,
        },
        Case {
            name: "non_finite_halts",
            ticks: 100,
            build: non_finite,
            input: no_input,
        },
    ]
}

/// Runs one case.
pub fn run(case: &Case) -> Outcome {
    let (fx, ran) = run_fixture(case);
    Outcome {
        name: case.name,
        ticks: ran,
        status: match &fx.w.status {
            SimStatus::Running => "running".to_owned(),
            SimStatus::Halted(e) => format!("halted:{e}"),
            other => format!("{other:?}"),
        },
        hash: hex(&hash::state_hash(&fx.w)),
    }
}

/// Runs one case and returns the finished fixture and the ticks it ran.
pub fn run_fixture(case: &Case) -> (Fixture, u64) {
    let mut fx = (case.build)();
    let mut ran = 0;
    for t in 0..case.ticks {
        if fx.w.status != SimStatus::Running {
            break;
        }
        let input = (case.input)(t);
        let mut ctx = fx.ctx();
        // A halting tick returns the error and leaves the world `Halted`; the status says so.
        let _ = tick::run_tick(&mut ctx, input);
        ran = fx.w.tick;
    }
    (fx, ran)
}

/// The report: one line per case.
pub fn report() -> String {
    let mut out = String::new();
    for case in cases() {
        out.push_str(&run(&case).line());
        out.push('\n');
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------------------------
// Fixture

/// A game and a world, driven with the engine's own hooks.
pub struct Fixture {
    def: GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    /// The world.
    pub w: World,
}

impl Fixture {
    fn new(def: GameDef, seed: u32) -> Fixture {
        let start = def.start_room;
        Fixture {
            def,
            rules: RunnerRules::default(),
            scripts: ScriptRuntime::default(),
            w: World::new(start, seed),
        }
    }

    fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            def: &self.def,
            rules: &self.rules,
            w: &mut self.w,
            scripts: &self.scripts,
            dll_mode: DllMode::Stub,
        }
    }

    /// Adds a probe instance with `state`, without running its Create.
    fn probe(&mut self, kind: KindId, x: f64, y: f64, state: ProbeState) -> InstId {
        let mut c = self.ctx();
        let init = SpawnInit {
            run_create: false,
            ..SpawnInit::default()
        };
        let id = c
            .spawn(kind, Real::new(x), Real::new(y), init)
            .expect("golden spawn");
        c.w.get_mut(id).expect("just spawned").state = KindState::Probe(state);
        id
    }
}

fn probe_kind(b: &mut GameDefBuilder, name: &str, sprite: bool) -> KindId {
    let mut k = KindDef::new(name, Builtin::Probe);
    k.events = probe_event_set();
    k.classes = ClassMask::NONE;
    if sprite {
        k.sprite = Some(b.sprite_rect((16, 16), (8, 8)));
    }
    b.kind(k)
}

fn builder() -> GameDefBuilder {
    let mut b = GameDefBuilder::new("golden");
    b.room(RoomDef::new("room_0", 800, 608));
    b.room(RoomDef::new("room_1", 800, 608));
    b
}

fn r(v: f64) -> Real {
    Real::new(v)
}

fn vr(v: f64) -> Value {
    Value::Real(r(v))
}

const COUNTER: VarId = VarId(10);

// ---------------------------------------------------------------------------------------------
// Inputs

fn no_input(_: u64) -> InputFrame {
    InputFrame::default()
}

/// Jump held in a 7-tick pattern, Right in a 13-tick pattern.
fn pattern_input(t: u64) -> InputFrame {
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

/// A fixed pseudo-random stream (a 64-bit LCG), with sub-tick taps: edges that are not in the
/// held bits.
fn random_input(t: u64) -> InputFrame {
    let mut s = t
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut next = || {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (s >> 33) as u32
    };
    let held = next() & Action::MASK;
    let pressed = (next() & Action::MASK) | (held & next());
    let released = next() & Action::MASK & !held;
    InputFrame {
        held,
        pressed,
        released,
    }
}

// ---------------------------------------------------------------------------------------------
// Cases

/// A parent probe that spawns short-lived children, sets alarms, reads keys and changes rooms.
fn spawn_alarms_rooms() -> Fixture {
    let mut b = builder();
    let parent = probe_kind(&mut b, "parent", false);
    let kid = probe_kind(&mut b, "child", true);
    let mut fx = Fixture::new(b.build().expect("golden def"), 1);
    let mut child = ProbeState::new("C", None);
    child.steps = vec![
        ProbeStep::on(ProbeOn::Create, ProbeAct::SetAlarm(0, r(7.0))),
        ProbeStep::on(ProbeOn::Alarm(0), ProbeAct::DestroySelf),
        ProbeStep::on(ProbeOn::Step, ProbeAct::SetGlobal(COUNTER, vr(1.0))).every(3),
        ProbeStep::on(ProbeOn::Step, ProbeAct::Move(r(0.75), r(-0.5))),
    ];
    let mut s = ProbeState::new("P", None);
    s.steps = vec![
        ProbeStep::on(
            ProbeOn::Step,
            ProbeAct::Spawn {
                kind: kid,
                x: r(100.5),
                y: r(-3.25),
                child: Some(Box::new(child)),
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

/// Moves by steps that are not exactly representable, so rounding is exercised every tick.
fn motion_fractions() -> Fixture {
    let mut b = builder();
    let mover = probe_kind(&mut b, "mover", true);
    let mut fx = Fixture::new(b.build().expect("golden def"), 7);
    for (i, (dx, dy)) in [(0.1, 0.2), (-0.3, 0.7), (1.0 / 3.0, -2.0 / 3.0), (0.0, 0.05)]
        .into_iter()
        .enumerate()
    {
        let mut s = ProbeState::new(format!("M{i}"), None);
        s.steps = vec![
            ProbeStep::on(ProbeOn::Step, ProbeAct::Move(r(dx), r(dy))),
            ProbeStep::on(ProbeOn::Step, ProbeAct::SetImageSpeed(r(0.25 * (i as f64 + 1.0)))),
        ];
        fx.probe(mover, 400.0 + 17.0 * i as f64, 300.0, s);
    }
    fx
}

/// Reads the keys directly and by events under a random input with sub-tick taps.
fn keys() -> Fixture {
    let mut b = builder();
    let reader = probe_kind(&mut b, "reader", false);
    let mut fx = Fixture::new(b.build().expect("golden def"), 3);
    let mut s = ProbeState::new("K", Some(VarId(0)));
    s.steps = vec![
        ProbeStep::on(ProbeOn::BeginStep, ProbeAct::DirectRead(Action::Jump.bit())),
        ProbeStep::on(ProbeOn::EndStep, ProbeAct::DirectRead(Action::Shoot.bit())).every(2),
        ProbeStep::on(ProbeOn::KeyPress, ProbeAct::SetGlobal(COUNTER, vr(1.0))),
        ProbeStep::on(ProbeOn::KeyRelease, ProbeAct::SetGlobal(COUNTER, vr(2.0))),
        ProbeStep::on(ProbeOn::KeyHeld, ProbeAct::SetAlarm(1, r(3.0))),
        ProbeStep::on(ProbeOn::Alarm(1), ProbeAct::SetGlobal(COUNTER, vr(3.0))),
    ];
    fx.probe(reader, 10.0, 10.0, s);
    fx
}

/// One probe reaches the non-finite path (docs/plans/sim-core.md 10.2): the sim halts with
/// `NonFinite` and freezes, on every target.
fn non_finite() -> Fixture {
    let mut b = builder();
    let bad = probe_kind(&mut b, "bad", true);
    let mut fx = Fixture::new(b.build().expect("golden def"), 5);
    let mut s = ProbeState::new("X", None);
    s.steps = vec![
        ProbeStep::on(ProbeOn::Step, ProbeAct::Move(r(1.5), r(0.5))),
        ProbeStep::on(ProbeOn::Step, ProbeAct::Move(r(f64::NAN), r(0.0))).at(12),
    ];
    fx.probe(bad, 50.0, 50.0, s);
    fx
}

/// Scripted objects (WP9): a walker script with create, step, alarm and collide bounces between
/// two walls and spawns sparks (another script, destroyed by its alarm) from alarm draws of the
/// one RNG. The scripts are the files `scripted_walker.rhai` and `scripted_spark.rhai`; the
/// format crate's loader test loads the same text through a package.
fn scripted_object() -> Fixture {
    let mut b = builder();
    let sprite = b.sprite_rect((16, 16), (8, 8));
    let mut walker = KindDef::new("walker", Builtin::Block);
    walker.script = Some(ScriptId(0));
    walker.sprite = Some(sprite);
    let mut spark = KindDef::new("spark", Builtin::Block);
    spark.script = Some(ScriptId(1));
    let mut wall = KindDef::new("wall", Builtin::Block);
    wall.sprite = Some(sprite);
    wall.classes = ClassMask::package(0).expect("class");
    b.kind(walker);
    b.kind(spark);
    b.kind(wall);
    let mut def = b.build().expect("golden def");
    def.scripts.push(ScriptDef {
        name: "scripts/walker.rhai".into(),
        source: include_str!("scripted_walker.rhai").into(),
    });
    def.scripts.push(ScriptDef {
        name: "scripts/spark.rhai".into(),
        source: include_str!("scripted_spark.rhai").into(),
    });
    def.var_names.insert("hp".into(), VarId(1));
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::new(&def, &rules).expect("golden scripts compile");
    let mut events = scripts.event_set(ScriptId(0));
    events.collisions.push(ClassMask::package(0).expect("class"));
    def.kinds[0].events = events;
    def.kinds[1].events = scripts.event_set(ScriptId(1));
    let mut fx = Fixture::new(def, 9);
    fx.scripts = scripts;
    let mut c = fx.ctx();
    for (kind, x, y) in [
        (2, 300.0, 300.0),
        (2, 600.0, 300.0),
        (0, 150.0, 300.0),
        (0, 450.0, 300.0),
    ] {
        c.spawn(KindId(kind), r(x), r(y), SpawnInit::default())
            .expect("golden spawn");
    }
    fx
}
