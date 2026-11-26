//! The retry boundary of the script host (#1021). A quick retry is a whole new `Sim`
//! (`crates/client/src/view.rs`, `restart_run`): it must reproduce a fresh run exactly and it
//! must not inherit anything from the run it replaces. These tests pin both sides before the
//! preparation work is split out of `ScriptRuntime::new`, so that refactor cannot quietly share
//! per-run state (RNG, budget, log, parked world, nesting depth) between runs.
//!
//! The measurement that motivated the work is `tests/script_prep.rs` (ignored harness).

use std::sync::Arc;

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, GameDef, KindDef, RoomDef, ScriptDef, ScriptId};
use iwbtg_sim::hash::state_hash;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sim::{Sim, SimOptions};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::world::SimStatus;

/// The spawner's Create creates a child, whose own Create runs nested.
const SPAWNER: &str = r#"
fn create() {
    instance_create("child", 5, 6);
}
"#;

/// The child draws nothing; it exists so the nested Create can be counted.
const CHILD: &str = "fn create() { }\n";

/// The walker draws from the run's RNG and writes to the capped script log every step.
const WALKER: &str = r#"
fn step() {
    this.x = this.x + random(5.0);
    print("tick");
}
"#;

fn options(seed: u32) -> SimOptions {
    SimOptions {
        start: StartPoint::NewGame { difficulty: 0 },
        seed,
        dll_mode: DllMode::Stub,
        mode: PlayMode::Original,
        map: None,
    }
}

/// A game with a spawner (nested Create), a child, and a walker with RNG and `print`.
fn scripted() -> Arc<GameDef> {
    let mut b = GameDefBuilder::new("script-retry");
    let room = b.room(RoomDef::new("room", 320, 240));
    let sprite = b.sprite_rect((16, 16), (0, 0));
    let mut kinds = Vec::new();
    for (name, script) in [("spawner", 0u32), ("child", 1), ("walker", 2)] {
        let mut k = KindDef::new(name, Builtin::Block);
        k.script = Some(ScriptId(script));
        k.sprite = Some(sprite);
        kinds.push(b.kind(k));
    }
    b.place(room, kinds[0], 8.0, 8.0).expect("spawner");
    b.place(room, kinds[2], 32.0, 8.0).expect("walker");
    let mut def = b.build().expect("def");
    for (name, source) in [
        ("scripts/spawner.rhai", SPAWNER),
        ("scripts/child.rhai", CHILD),
        ("scripts/walker.rhai", WALKER),
    ] {
        def.scripts.push(ScriptDef {
            name: name.into(),
            source: source.into(),
        });
    }
    // The loader merges each script's events into its kind; a hand-built def does it here.
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let runtime = ScriptRuntime::new(&def, &rules).expect("compile");
    for (i, sid) in [ScriptId(0), ScriptId(1), ScriptId(2)].into_iter().enumerate() {
        def.kinds[i].events = runtime.event_set(sid);
    }
    Arc::new(def)
}

fn start(def: &Arc<GameDef>, seed: u32) -> Sim {
    Sim::new_or_halted(Arc::clone(def), options(seed)).expect("sim")
}

fn step(sim: &mut Sim, ticks: u64) {
    for _ in 0..ticks {
        sim.step(InputFrame::EMPTY);
    }
}

fn children(sim: &Sim) -> usize {
    sim.world()
        .iter_alive()
        .filter(|i| sim.def().kind(i.kind).is_some_and(|k| k.name == "child"))
        .count()
}

#[test]
fn a_retry_reproduces_a_fresh_run() {
    let def = scripted();
    let mut first = start(&def, 1);
    step(&mut first, 40);
    let mut again = start(&def, 1);
    step(&mut again, 40);
    assert_eq!(state_hash(first.world()), state_hash(again.world()));
    assert_eq!(
        children(&first),
        children(&again),
        "the nested Create ran once per run"
    );
}

#[test]
fn a_fresh_seed_changes_the_run() {
    let def = scripted();
    let mut one = start(&def, 1);
    let mut two = start(&def, 2);
    step(&mut one, 40);
    step(&mut two, 40);
    assert_ne!(state_hash(one.world()), state_hash(two.world()));
}

#[test]
fn alternating_sims_share_no_rng_budget_or_log() {
    let def = scripted();
    let mut one = start(&def, 1);
    let mut two = start(&def, 2);
    for _ in 0..40 {
        one.step(InputFrame::EMPTY);
        two.step(InputFrame::EMPTY);
    }
    let mut alone_one = start(&def, 1);
    let mut alone_two = start(&def, 2);
    step(&mut alone_one, 40);
    step(&mut alone_two, 40);
    assert_eq!(state_hash(one.world()), state_hash(alone_one.world()));
    assert_eq!(state_hash(two.world()), state_hash(alone_two.world()));
}

#[test]
fn a_new_run_starts_with_an_empty_log_and_no_leftover_budget() {
    let def = scripted();
    // The start-room Create itself spends budget (the spawner creates a child). A fresh sim
    // shows exactly its own start work and nothing from an earlier run.
    let fresh = start(&def, 1);
    let fresh_used = fresh.scripts().engine().expect("engine").budget().used();
    let fresh_peak = fresh.scripts().engine().expect("engine").budget().peak();
    assert!(fresh_used > 0, "the start-room Create ran");
    drop(fresh);

    let mut first = start(&def, 1);
    step(&mut first, 5);
    let first_log = first.scripts().engine().expect("engine").log();
    assert_eq!(first_log.len(), 5, "the walker printed on each step");

    // The retry replaces the sim, so its engine is a new one: no log lines and no operations
    // from the run it replaces.
    let mut again = start(&def, 1);
    let engine = again.scripts().engine().expect("engine");
    assert!(engine.log().is_empty(), "the log leaked into the retry");
    assert_eq!(engine.budget().used(), fresh_used, "budget from the old run");
    assert_eq!(engine.budget().peak(), fresh_peak, "peak from the old run");
    step(&mut again, 1);
    assert_eq!(again.scripts().engine().expect("engine").log().len(), 1);
    // The replaced run still owns its log.
    assert_eq!(first.scripts().engine().expect("engine").log(), first_log);
}

#[test]
fn a_start_script_error_halts_again_on_a_retry() {
    let mut def = (*scripted()).clone();
    def.scripts[2] = ScriptDef {
        name: "scripts/walker.rhai".into(),
        source: "fn create() { throw \"boom\"; }".into(),
    };
    let def = Arc::new(def);
    // `Sim::new` rejects a halted start; `new_or_halted` keeps the halted sim so the client can
    // open the script error screen (#946), and a retry does the same.
    assert!(Sim::new(Arc::clone(&def), options(1)).is_err());
    let halted = |sim: &Sim| match &sim.world().status {
        SimStatus::Halted(e) => e.to_string(),
        other => panic!("not halted: {other:?}"),
    };
    let first = start(&def, 1);
    let again = start(&def, 1);
    assert!(halted(&first).contains("boom"), "{}", halted(&first));
    assert_eq!(halted(&first), halted(&again), "the retry halts differently");
}

#[test]
fn a_prepared_retry_reproduces_a_fresh_run_and_keeps_per_run_state() {
    let def = scripted();
    let prepared = Sim::prepare(&def).expect("prepare");

    // A prepared run and the prepare-plus-run path hash identically.
    let mut plain = start(&def, 1);
    step(&mut plain, 40);
    let mut first =
        Sim::new_or_halted_prepared(Arc::clone(&def), options(1), &prepared).expect("prepared sim");
    step(&mut first, 40);
    assert_eq!(
        state_hash(first.world()),
        state_hash(plain.world()),
        "a prepared run differs from a fresh run"
    );

    // The artifact is shared, but each run owns its engine, budget and log.
    let fresh = Sim::new_or_halted_prepared(Arc::clone(&def), options(1), &prepared).expect("prepared sim");
    let fresh_used = fresh.scripts().engine().expect("engine").budget().used();
    drop(fresh);
    let mut again =
        Sim::new_or_halted_prepared(Arc::clone(&def), options(1), &prepared).expect("prepared sim");
    let engine = again.scripts().engine().expect("engine");
    assert!(engine.log().is_empty(), "the log leaked into the prepared retry");
    assert_eq!(engine.budget().used(), fresh_used, "budget from the old run");
    step(&mut again, 40);
    assert_eq!(state_hash(first.world()), state_hash(again.world()));
    assert_eq!(
        children(&first),
        children(&again),
        "the nested Create ran once per run"
    );
}

#[test]
fn a_prepared_artifact_cannot_run_another_package() {
    let def = scripted();
    let prepared = Sim::prepare(&def).expect("prepare");
    let mut b = GameDefBuilder::new("other");
    let room = b.room(RoomDef::new("room", 320, 240));
    b.start_room(room);
    let other = Arc::new(b.build().expect("def"));
    let e = Sim::new_or_halted_prepared(other, options(1), &prepared)
        .expect_err("another package's artifact must be refused");
    assert!(e.to_string().contains("cannot run package"), "{e}");
}

#[test]
fn game_def_stays_plain_data() {
    // The package definition is shared by `Arc` across runs and has no script host inside it
    // (docs/architecture/sim/scripts.md): it must stay `Send + Sync` so the client can keep it
    // in a shared resource and the server can load one per connection.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GameDef>();
    assert_send_sync::<iwbtg_sim::def::ScriptDef>();
}
