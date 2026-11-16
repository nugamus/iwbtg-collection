//! The golden corpus against its pinned lines (docs/plans/sim-core.md 10.6). Runs on every CI
//! target, wasm32-unknown-unknown included. `UPDATE_GOLDEN=1` rewrites `golden/expected.txt`.

#[path = "golden/mod.rs"]
mod golden;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;

/// The pinned lines, compiled in so that every target reads the same bytes without a file system.
const EXPECTED: &str = include_str!("golden/expected.txt");

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the test, not the sim, reads UPDATE_GOLDEN and rewrites the pinned file"
)]
fn golden_corpus_matches_the_pinned_hashes() {
    let report = golden::report();
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/expected.txt");
        std::fs::write(path, &report).expect("write expected.txt");
        return;
    }
    assert_eq!(
        report,
        EXPECTED.replace("\r\n", "\n"),
        "golden corpus changed: if this is deliberate, run `UPDATE_GOLDEN=1 cargo test -p iwbtg-sim --test golden_corpus` and commit golden/expected.txt"
    );
}

#[test]
fn golden_corpus_is_run_to_run_identical() {
    assert_eq!(golden::report(), golden::report());
}

#[test]
fn non_finite_case_halts_with_non_finite() {
    let case = golden::cases()
        .into_iter()
        .find(|c| c.name == "non_finite_halts")
        .expect("case exists");
    let out = golden::run(&case);
    assert!(out.status.starts_with("halted"), "{}", out.line());
    assert!(out.ticks < case.ticks, "the run stopped early: {}", out.line());
}

/// The scripted case really runs create, step, alarm and collide: the walkers bounce off the
/// walls (collide emits `walker_reset` every third hit), the alarm spawns sparks, and the sim
/// never halts.
#[test]
fn the_scripted_case_runs_every_event() {
    use iwbtg_sim::output::OutputEvent;
    use iwbtg_sim::script::ScriptOutput;
    use iwbtg_sim::world::SimStatus;
    let case = golden::cases()
        .into_iter()
        .find(|c| c.name == "scripted_object")
        .expect("the case exists");
    let (fx, ran) = golden::run_fixture(&case);
    assert_eq!(fx.w.status, SimStatus::Running);
    assert_eq!(ran, case.ticks);
    let resets = fx
        .w
        .output
        .iter()
        .filter(|e| matches!(e, OutputEvent::Script(ScriptOutput::Emit { name }) if name == "walker_reset"))
        .count();
    assert!(resets >= 2, "collide ran {resets} resets");
    // Four instances were placed; sparks came from the alarm.
    let newest = fx.w.iter_alive().map(|i| i.id.0).max().expect("alive");
    let oldest = fx.w.iter_alive().map(|i| i.id.0).min().expect("alive");
    assert!(
        newest - oldest >= 6,
        "ids {oldest}..{newest}: the alarm spawned sparks"
    );
}

/// Built-ins no golden case runs, each with the reason. Shrinks; never grows without one.
const UNCOVERED_BUILTINS: &[(&str, &str)] = &[(
    "inert",
    "only `LoadMode::Trace` loads it: an unmapped kind with no events",
)];

/// Every `Builtin` and every `KidFamily` runs in some golden case (#786), so a new kind or
/// family cannot skip the cross-platform byte compare: the report's coverage lines name a case
/// for each, except the allow-listed built-ins. Every kid case must also still be running at
/// its end (a halted course would cover nothing after the halt).
#[test]
fn every_builtin_and_kid_family_runs_in_a_golden_case() {
    use iwbtg_sim::def::Builtin;
    let report = golden::report();
    let mut missing = Vec::new();
    for b in Builtin::ALL {
        let line = report
            .lines()
            .find(|l| l.split(' ').take(2).eq(["builtin", b.name()]))
            .expect("a line per built-in");
        let uncovered = line.ends_with(" -");
        let allowed = UNCOVERED_BUILTINS.iter().any(|(n, _)| *n == b.name());
        if uncovered != allowed {
            missing.push(line.to_owned());
        }
    }
    for f in golden::FAMILIES {
        let line = report
            .lines()
            .find(|l| l.starts_with(&format!("family {f:?} ")))
            .expect("a line per family");
        if line.ends_with(" -") {
            missing.push(line.to_owned());
        }
    }
    assert!(
        missing.is_empty(),
        "add a golden case (tests/golden/courses.rs) or fix the allow-list: {missing:?}"
    );
    for case in golden::cases()
        .iter()
        .filter(|c| c.name.starts_with("kid_") || c.name == "script_api")
    {
        let out = golden::run(case);
        assert_eq!(out.status, "running", "{}", out.line());
        assert_eq!(out.ticks, case.ticks, "{}", out.line());
    }
}

/// Every function and instance property of the script API (the generated
/// `docs/format/script/iws.d.rhai`) is called in a golden script, so the byte compare covers
/// each host function.
#[test]
fn golden_scripts_call_every_script_api_function() {
    const STUBS: &str = include_str!("../../../docs/format/script/iws.d.rhai");
    const SCRIPTS: &[&str] = &[
        include_str!("golden/script_api.rhai"),
        include_str!("golden/script_mover.rhai"),
        include_str!("golden/scripted_walker.rhai"),
        include_str!("golden/scripted_spark.rhai"),
    ];
    let text = SCRIPTS.concat();
    let mut missing = std::collections::BTreeSet::new();
    for line in STUBS.lines() {
        let Some(sig) = line.strip_prefix("fn ") else {
            continue;
        };
        let name = sig.split('(').next().unwrap_or_default().trim();
        let found = if let Some(p) = name.strip_prefix("get ") {
            text.contains(&format!(".{p}"))
        } else if let Some(p) = name.strip_prefix("set ") {
            text.contains(&format!(".{p} =")) || text.contains(&format!(".{p} -="))
        } else if name.starts_with("index$") {
            // `this.hp` reads and writes go through the indexers.
            text.contains("this.hp =") && text.contains("this.hp +")
        } else {
            text.contains(&format!("{name}("))
        };
        if !found {
            missing.insert(name.to_owned());
        }
    }
    assert!(missing.is_empty(), "no golden script calls {missing:?}");
}
