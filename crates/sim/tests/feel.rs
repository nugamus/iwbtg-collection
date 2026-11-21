//! The public feel contracts (docs/architecture/feel-contracts.md): the format and the runner
//! run on every CI target without the private repos. The contracts are compiled in, so a target
//! without a filesystem (wasm32-unknown-unknown) runs the same bytes as `cargo xtask feel`.
//!
//! `tests/feel/mod.rs` holds the loader, validator and evaluator; `crates/tools/src/feel_cmd.rs`
//! includes the same module for the CLI, and `cargo xtask feel` discovers files in
//! `crates/sim/tests/feel/**`. The contract list is generated into `feel/contracts.rs` from the
//! folder by `cargo xtask feel`; `the_contract_list_matches_the_folder` fails on a filesystem
//! target when it is stale, so a new contract cannot be silently skipped (#1241).

#[path = "feel/mod.rs"]
mod contract;

use contract::Verdict;

// Every public contract, by its id (the path below the contracts root) and its text. Generated;
// do not edit `feel/contracts.rs` by hand.
include!("feel/contracts.rs");

/// Parses, validates and runs one contract against its public fixture.
fn run_contract(id: &str, text: &str) -> Verdict {
    let raw = contract::parse(text).unwrap_or_else(|e| panic!("{id}: {e}"));
    let package = contract::package(&raw);
    let def = match contract::fixtures::from_package(package) {
        Some(Ok(def)) => def,
        Some(Err(e)) => panic!("{id}: {e}"),
        None => panic!("{id}: a public contract must name `fixture:<name>` (got package {package:?})"),
    };
    let loaded = contract::validate(raw, &def, id).unwrap_or_else(|e| panic!("{id}: {e}"));
    contract::run(&loaded, def)
}

/// Every public contract passes or is OPEN; a failure names the mismatch and five ticks of
/// context, like `trace diff`.
#[test]
fn the_public_contracts_pass() {
    assert!(!CONTRACTS.is_empty(), "the public contract set is empty");
    for (id, text) in CONTRACTS {
        if let Verdict::Fail(mismatches) = run_contract(id, text) {
            let detail: String = mismatches.iter().map(ToString::to_string).collect();
            panic!("{id}: FAIL\n{detail}");
        }
    }
}

/// The compiled list names each contract once (the staleness test below checks the folder).
#[test]
fn the_public_contract_ids_are_unique() {
    let mut ids: Vec<&str> = CONTRACTS.iter().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "{ids:?}");
}

/// The generated list follows `crates/sim/tests/feel/`: a new contract file that `cargo xtask
/// feel` has not seen fails here instead of being silently skipped (#1241). A target without a
/// filesystem cannot enumerate, so this check is native/wasip1 only.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the test, not the sim, enumerates the folder to catch a stale generated list"
)]
fn the_contract_list_matches_the_folder() {
    use std::path::Path;

    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("the feel folder reads") {
            let path = entry.expect("a feel entry").path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().is_some_and(|e| e == "toml") {
                out.push(
                    path.strip_prefix(root)
                        .expect("below the feel root")
                        .with_extension("")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/feel");
    let mut found = Vec::new();
    walk(&root, &root, &mut found);
    found.sort();
    let mut listed: Vec<String> = CONTRACTS.iter().map(|(id, _)| (*id).to_owned()).collect();
    listed.sort();
    assert_eq!(
        listed, found,
        "the compiled contract list is stale; run `cargo xtask feel` (or `cargo xtask regen`)"
    );
}

/// `feel/self/pass`: a contract that must pass.
#[test]
fn self_pass() {
    let text = r#"
id = "self/pass"
impact = "F2"
package = "fixture:jump"
start = { room = "room_0" }
inputs = "frame 0: end"
[[assert]]
tick = 0
kid_count = 1
status = "running"
room = "room_0"
"#;
    assert_eq!(run_contract("self/pass", text), Verdict::Ok);
}

/// `feel/self/mismatch`: a contract that must fail with the exact mismatch line.
#[test]
fn self_mismatch() {
    let text = r#"
id = "self/mismatch"
impact = "F2"
package = "fixture:jump"
start = { room = "room_0" }
inputs = "frame 0: end"
[[assert]]
tick = 0
kid_count = 2
"#;
    let Verdict::Fail(mismatches) = run_contract("self/mismatch", text) else {
        panic!("the contract must fail");
    };
    assert_eq!(mismatches.len(), 1);
    let line = mismatches[0].to_string();
    assert!(
        line.starts_with("mismatch: tick 0, kid_count: expected 2, got 1"),
        "{line}"
    );
}

/// An F1 contract with no evidence loads and reports OPEN; with evidence it is OK.
#[test]
fn self_open_without_evidence() {
    let text = r#"
id = "self/open"
impact = "F1"
package = "fixture:jump"
start = { room = "room_0" }
inputs = "frame 0: end"
[[assert]]
tick = 0
kid_count = 1
"#;
    assert_eq!(run_contract("self/open", text), Verdict::Open);
    let with_evidence = text.replace(
        "inputs = \"frame 0: end\"",
        "evidence = [\"golden:kid_a1_basics\"]\ninputs = \"frame 0: end\"",
    );
    assert_eq!(run_contract("self/open", &with_evidence), Verdict::Ok);
}

/// A missing fixture is an error the tool maps to FAIL, never a skip.
#[test]
fn self_missing_package_is_an_error() {
    let err = contract::fixtures::fixture("does-not-exist").unwrap_err();
    assert!(err.contains("no public fixture"), "{err}");
}

/// A tick past the input script is a load error, not a silent pass.
#[test]
fn a_tick_past_the_inputs_is_a_load_error() {
    let text = r#"
id = "self/late"
impact = "F2"
package = "fixture:jump"
start = { room = "room_0" }
inputs = "frame 0: end"
[[assert]]
tick = 5
kid_count = 1
"#;
    let raw = contract::parse(text).unwrap();
    let def = contract::fixtures::fixture("jump").unwrap();
    let err = contract::validate(raw, &def, "self/late").unwrap_err();
    assert!(err.contains("past the inputs' last tick"), "{err}");
}
