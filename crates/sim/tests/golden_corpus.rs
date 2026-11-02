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
