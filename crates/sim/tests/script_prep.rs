//! Measurement harness for #1021: how much of a retry is immutable script preparation.
//!
//! A quick retry builds a new `Sim` (`crates/client/src/view.rs`, `restart_run`).
//! `Sim::new_or_halted` prepares the artifact and then builds a run from it, so a caller without
//! a cache compiles every script again, registers a whole Rhai engine again and keeps the
//! `GameDef` shared; `Sim::new_or_halted_prepared` is the cached retry the client and the trace
//! path use. This harness builds a synthetic script-heavy package and records, for each step of
//! both paths, the allocations (`stats_alloc`) and, where the target has a clock, the wall time.
//!
//! It is a measurement, not a gate: the assertions only pin facts that must hold on every
//! target (script count and bytes, a retry reproduces a fresh run, and a prepared retry stays an
//! order of magnitude below an unprepared one). Run it explicitly:
//!
//! ```text
//! cargo test -p iwbtg-sim --test script_prep -- --ignored --nocapture
//! cargo test -p iwbtg-sim --test script_prep --target wasm32-unknown-unknown -- --include-ignored --nocapture
//! ```
//!
//! `wasm32-unknown-unknown` runs under node through the repository's `wasm-bindgen-test-runner`;
//! a real browser needs `WASM_BINDGEN_USE_BROWSER=1` and a WebDriver. The browser target has no
//! clock through `std`, so its table has allocation columns only.
//!
//! ## What the measurement says, and what #1093 changed
//!
//! Retry setup was almost entirely compilation on both targets: an unprepared scripted retry
//! and a preparation allocate nearly the same blocks (~145k), while the per-run engine is ~1.3k
//! and the `GameDef` deep clone ~600. #1093 splits the path:
//!
//! 1. `script::PreparedScripts` (sim-owned, opaque): the compiled scripts and the kind-name
//!    map, built once by `PreparedScripts::new(&GameDef, &RunnerRules)`. `AST` is immutable at
//!    run time (`call_fn` takes `&AST`), so it is shared behind `Rc` (the ASTs are not `Send`);
//!    no Rhai type enters `GameDef`.
//! 2. `ScriptRuntime::new_run(Arc<GameDef>, &RunnerRules, &PreparedScripts)`: a fresh `Iws` and
//!    `Shared` per run (the registered functions capture `Rc<Shared>`, so per-run state stays
//!    per run), reusing only the prepared ASTs. `Host::def` is `Arc<GameDef>`, which removes the
//!    deep clone.
//! 3. `Sim::prepare`/`Sim::new_prepared`/`Sim::new_or_halted_prepared` expose the retry path, and
//!    `Sim::new_with_setup` (the trace path) builds its run the same way. The client builds the
//!    artifact once in `view::new_run`, keeps it in a `NonSend` resource and hands it back on
//!    every `view::restart_run`.
//!
//! `tests/script_retry.rs` pins the isolation contract: a retry reproduces a fresh run, fresh
//! seeds differ, interleaved sims share no RNG/budget/log, and a start error re-halts the same
//! way; it also pins the prepared path to the same hash.

/// `println!` on native; `console.log` under node, where the wasm test has no stdout.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
macro_rules! report {
    ($($arg:tt)*) => { wasm_bindgen_test::console_log!($($arg)*) };
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
macro_rules! report {
    ($($arg:tt)*) => { println!($($arg)*) };
}

use std::alloc::System;
use std::sync::Arc;

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, GameDef, KindDef, RoomDef, ScriptDef, ScriptId, VarId};
use iwbtg_sim::hash::state_hash;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::host::Iws;
use iwbtg_sim::script::{PreparedScripts, ScriptRuntime};
use iwbtg_sim::sim::{Sim, SimOptions};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::world::SimStatus;
use stats_alloc::{Region, Stats, StatsAlloc};

#[global_allocator]
static GLOBAL: StatsAlloc<System> = StatsAlloc::system();

/// Scripts in the synthetic package. Enough that compilation dominates the retry.
const SCRIPT_COUNT: usize = 120;
/// Arithmetic lines in each `step`, so a script is a few KiB of real Rhai to parse and optimize.
const STEP_LINES: usize = 60;
/// Custom variable names, to give the `GameDef` clone a non-trivial map to copy.
const VAR_NAMES: usize = 200;
/// Placed scripted instances (their Create runs while the sim is built).
const PLACED: u32 = 8;
/// Ticks run on each measured sim, so the retry path includes a little gameplay.
const TICKS: u64 = 50;
/// Times each scenario is repeated; the table shows the median.
const ROUNDS: usize = 5;

fn source(i: usize) -> String {
    let mut s = String::with_capacity(STEP_LINES * 40 + 256);
    s.push_str(&format!("const STEP_{i} = {i}.0;\n"));
    s.push_str(&format!(
        "fn helper_{i}(n) {{\n    let a = n + STEP_{i};\n    let b = a * 2.0;\n    b - STEP_{i}\n}}\n"
    ));
    s.push_str("fn create() {\n    this.set_alarm(0, 5);\n}\n");
    s.push_str(&format!("fn step() {{\n    let v0 = helper_{i}(1.0);\n"));
    for line in 1..STEP_LINES {
        s.push_str(&format!("    let v{line} = v{} + {line}.0;\n", line - 1));
    }
    s.push_str(&format!(
        "    this.x = this.x + v{};\n    this.y = this.y + (random(2.0) - 1.0);\n}}\n",
        STEP_LINES - 1
    ));
    s.push_str("fn alarm(n) {\n    this.set_alarm(n, 5);\n}\n");
    s.push_str("fn user(n) {\n    this.x = this.x + to_float(n);\n}\n");
    s
}

/// The synthetic game: `SCRIPT_COUNT` scripted kinds, one script each, the first `PLACED` in
/// one room. Script `i` belongs to kind `i`.
fn package() -> GameDef {
    let mut b = GameDefBuilder::new("script-prep");
    let room = b.room(RoomDef::new("room", 800, 608));
    let sprite = b.sprite_rect((16, 16), (0, 0));
    let mut kinds = Vec::new();
    for i in 0..SCRIPT_COUNT {
        let mut k = KindDef::new(format!("s{i}"), Builtin::Block);
        k.script = Some(ScriptId(u32::try_from(i).expect("script id")));
        k.sprite = Some(sprite);
        kinds.push(b.kind(k));
    }
    for (n, kind) in kinds.iter().take(PLACED as usize).enumerate() {
        b.place(room, *kind, 16.0 * (n as f64 + 1.0), 16.0)
            .expect("place");
    }
    let mut def = b.build().expect("def");
    for i in 0..SCRIPT_COUNT {
        def.scripts.push(ScriptDef {
            name: format!("scripts/s{i}.rhai"),
            source: source(i),
        });
    }
    for i in 0..VAR_NAMES {
        def.var_names
            .insert(format!("v{i}"), VarId(u32::try_from(i).expect("var id")));
    }
    // Give every scripted kind the events its script defines, as the loader does, so the placed
    // instances run Step too and the measured runs do real script work.
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let runtime = ScriptRuntime::new(&def, &rules).expect("compile");
    for i in 0..SCRIPT_COUNT {
        let sid = ScriptId(u32::try_from(i).expect("script id"));
        def.kinds[i].events = runtime.event_set(sid);
    }
    def
}

fn options() -> SimOptions {
    SimOptions {
        start: StartPoint::NewGame { difficulty: 0 },
        seed: 7,
        dll_mode: DllMode::Stub,
        mode: PlayMode::Original,
        map: None,
    }
}

/// A sim built from `def`, stepped [`TICKS`] ticks: the retry path end to end.
fn run(def: Arc<GameDef>) -> Sim {
    let mut sim = Sim::new_or_halted(def, options()).expect("sim");
    step_measured(&mut sim);
    sim
}

/// The same run from a prepared artifact: the #1093 retry path.
fn run_prepared(def: &Arc<GameDef>, prepared: &PreparedScripts) -> Sim {
    let mut sim = Sim::new_or_halted_prepared(Arc::clone(def), options(), prepared).expect("sim");
    step_measured(&mut sim);
    sim
}

fn step_measured(sim: &mut Sim) {
    for _ in 0..TICKS {
        sim.step(InputFrame::EMPTY);
    }
    assert_eq!(
        sim.world().status,
        SimStatus::Running,
        "the measured run halted, so it did not exercise the scripts"
    );
}

struct Measured {
    /// Median wall time, `None` on a target with no clock.
    millis: Option<f64>,
    /// The last round's allocation delta. Allocation counts are deterministic here, so the
    /// rounds report the same number.
    stats: Stats,
}

/// Runs `f` [`ROUNDS`] times, dropping each value inside its round so retained allocations
/// are counted.
fn measure<T>(mut f: impl FnMut() -> T) -> Measured {
    let mut times = Vec::with_capacity(ROUNDS);
    let mut stats = Stats::default();
    for _ in 0..ROUNDS {
        let region = Region::new(&GLOBAL);
        let start = clock();
        let value = f();
        if let Some(start) = start {
            times.push(start.elapsed().as_secs_f64() * 1_000.0);
        }
        stats = region.change();
        drop(value);
    }
    times.sort_by(|a, b| a.total_cmp(b));
    Measured {
        millis: times.get(times.len() / 2).copied(),
        stats,
    }
}

/// The system clock, or `None` on the browser target (where `std::time` has no clock).
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[allow(
    clippy::disallowed_types,
    reason = "the measurement harness, not the sim, names the clock type"
)]
fn clock() -> Option<std::time::Instant> {
    None
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[allow(
    clippy::disallowed_types,
    reason = "the measurement harness, not the sim, reads the clock"
)]
fn clock() -> Option<std::time::Instant> {
    Some(std::time::Instant::now())
}

fn print_row(label: &str, m: &Measured) {
    let ms = m.millis.map_or_else(|| "n/a".to_owned(), |ms| format!("{ms:.1}"));
    report!(
        "{label:<24} {ms:>9} {:>12} {:>15} {:>10}",
        m.stats.allocations,
        m.stats.bytes_allocated,
        m.stats.reallocations
    );
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;

#[test]
#[ignore = "measurement harness: run with --ignored --nocapture"]
fn script_preparation_and_retry() {
    let def = Arc::new(package());
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let script_bytes: usize = def.scripts.iter().map(|s| s.source.len()).sum();

    // The same game without scripts: what a retry costs when there is nothing to prepare.
    let mut plain = (*def).clone();
    plain.scripts.clear();
    for kind in &mut plain.kinds {
        kind.script = None;
        kind.events = Default::default();
    }
    let plain = Arc::new(plain);

    // A retry must reproduce a fresh run. The hash is printed for the native/WASM comparison;
    // it is not pinned here because this harness is not a gate.
    let fresh = run(Arc::clone(&def));
    let retry = run(Arc::clone(&def));
    let hash = state_hash(fresh.world());
    assert_eq!(
        hash,
        state_hash(retry.world()),
        "a retry differs from a fresh run"
    );
    drop(fresh);
    drop(retry);

    // The prepared path must produce the same run as prepare-plus-run.
    let prepared = Sim::prepare(&def).expect("prepare artifact");
    let prepared_fresh = run_prepared(&def, &prepared);
    assert_eq!(
        hash,
        state_hash(prepared_fresh.world()),
        "the prepared path differs from a fresh run"
    );
    drop(prepared_fresh);

    // Warm the code paths once, then measure.
    drop(ScriptRuntime::new(&def, &rules).expect("warm compile"));
    drop(ScriptRuntime::new_run(Arc::clone(&def), &rules, &prepared).expect("warm run"));
    drop(run(Arc::clone(&def)));
    drop(run_prepared(&def, &prepared));

    let engine = measure(|| Iws::new(rules.num.clone()));
    let prepare = measure(|| PreparedScripts::new(&def, &rules).expect("prepare"));
    let prepared_run = measure(|| ScriptRuntime::new_run(Arc::clone(&def), &rules, &prepared).expect("run"));
    let deep_clone = measure(|| (*def).clone());
    // A retry is `Sim::new_or_halted`; the ticks after it are gameplay, not setup, so the table
    // keeps them apart. The prepared rows are the #1093 path.
    let retry = measure(|| Sim::new_or_halted(Arc::clone(&def), options()).expect("sim"));
    let prepared_retry =
        measure(|| Sim::new_or_halted_prepared(Arc::clone(&def), options(), &prepared).expect("sim"));
    let retry_ticks = measure(|| run(Arc::clone(&def)));
    let prepared_ticks = measure(|| run_prepared(&def, &prepared));
    let plain = measure(|| Sim::new_or_halted(Arc::clone(&plain), options()).expect("sim"));

    report!(
        "\nscript-heavy package: {} scripts, {} source bytes, {} kinds, {} placed, {} custom vars",
        def.scripts.len(),
        script_bytes,
        def.kinds.len(),
        PLACED,
        def.var_names.len()
    );
    report!(
        "each sim: {} ticks; state hash after the ticks: {}",
        TICKS,
        hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    report!(
        "\n{:<24} {:>9} {:>12} {:>15} {:>10}",
        "scenario",
        "ms (med)",
        "allocations",
        "alloc bytes",
        "reallocs"
    );
    print_row("engine only (Iws)", &engine);
    print_row("GameDef deep clone", &deep_clone);
    print_row("prepare (PreparedScripts)", &prepare);
    print_row("run (new_run, fresh engine)", &prepared_run);
    print_row("retry setup (Sim::new_or_halted)", &retry);
    print_row("retry setup, prepared", &prepared_retry);
    print_row("retry setup, no scripts", &plain);
    print_row("retry + 50 ticks", &retry_ticks);
    print_row("prepared retry + 50 ticks", &prepared_ticks);
    report!(
        "\nprepared / unprepared retry setup: {:.2}x allocations ({} vs {})",
        prepared_retry.stats.allocations as f64 / retry.stats.allocations.max(1) as f64,
        prepared_retry.stats.allocations,
        retry.stats.allocations
    );

    assert_eq!(def.scripts.len(), SCRIPT_COUNT);
    assert!(script_bytes > SCRIPT_COUNT * 1_000, "scripts are a few KiB each");
    assert!(prepare.stats.allocations > 0 && retry.stats.allocations > 0);
    assert!(
        prepared_retry.stats.allocations.saturating_mul(10) < retry.stats.allocations,
        "prepared retry {} allocations is not an order of magnitude below {}",
        prepared_retry.stats.allocations,
        retry.stats.allocations
    );
    assert!(
        prepared_retry.stats.allocations < 5_000,
        "prepared retry {} allocations (expected ~1.3-1.4k)",
        prepared_retry.stats.allocations
    );
}
