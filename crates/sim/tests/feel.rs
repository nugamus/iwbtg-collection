//! The public feel contracts (docs/architecture/feel-contracts.md): the format and the runner
//! run on every CI target without the private repos. The contracts are compiled in, so a target
//! without a filesystem (wasm32-unknown-unknown) runs the same bytes as `cargo xtask feel`.
//!
//! `tests/feel/mod.rs` holds the loader, validator and evaluator; `crates/tools/src/feel_cmd.rs`
//! includes the same module for the CLI, and `cargo xtask feel` discovers files in
//! `crates/sim/tests/feel/**`. The four contracts here are the example set from the issue.

#[path = "feel/mod.rs"]
mod contract;

use contract::Verdict;

/// Every public contract, by its id (the path below the contracts root) and its text.
const CONTRACTS: &[(&str, &str)] = &[
    (
        "lifecycle/retry_after_death",
        include_str!("feel/lifecycle/retry_after_death.toml"),
    ),
    (
        "lifecycle/reset_scope",
        include_str!("feel/lifecycle/reset_scope.toml"),
    ),
    ("physics/jump_apex", include_str!("feel/physics/jump_apex.toml")),
    (
        "collision/overlapping_hazards",
        include_str!("feel/collision/overlapping_hazards.toml"),
    ),
];

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
    for (id, text) in CONTRACTS {
        if let Verdict::Fail(mismatches) = run_contract(id, text) {
            let detail: String = mismatches.iter().map(ToString::to_string).collect();
            panic!("{id}: FAIL\n{detail}");
        }
    }
}

/// The public set is exactly the example set from the issue and every file is named once.
#[test]
fn the_public_contract_ids_are_unique() {
    let mut ids: Vec<&str> = CONTRACTS.iter().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), CONTRACTS.len(), "{ids:?}");
    for want in [
        "lifecycle/retry_after_death",
        "lifecycle/reset_scope",
        "physics/jump_apex",
        "collision/overlapping_hazards",
    ] {
        assert!(ids.contains(&want), "missing public contract {want}");
    }
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
