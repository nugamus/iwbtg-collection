//! Every integration test of iwbtg-format in one executable (#517): one test binary links and
//! starts much faster than one per file. Each former `tests/<file>.rs` is a module here, so its
//! tests are named `<file>::...`; run one file with `cargo test -p iwbtg-format --test it <file>::`.
//! Kept as their own targets (Cargo.toml): `fixtures`, `schemas`, which CI runs on wasm targets or
//! which regenerate pinned files.
//!
//! A new test file goes into this list (`autotests = false` in Cargo.toml, so cargo won't find it alone).

#[path = "fmt_files.rs"]
mod fmt_files;
#[path = "iwbz.rs"]
mod iwbz;
#[path = "room_art.rs"]
mod room_art;
#[path = "wp5a_physics.rs"]
mod wp5a_physics;
#[path = "wp5a_restart_steps.rs"]
mod wp5a_restart_steps;
#[path = "wp5b_physics.rs"]
mod wp5b_physics;
#[path = "wp5c_physics.rs"]
mod wp5c_physics;
#[path = "wp7a_load.rs"]
mod wp7a_load;
#[path = "wp7a_research.rs"]
mod wp7a_research;
#[path = "wp7b_loader.rs"]
mod wp7b_loader;

/// `autotests = false` means a new `tests/*.rs` file runs only once it is listed here or in Cargo.toml.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[allow(clippy::disallowed_methods, reason = "reads the test folder, not sim code")]
fn every_test_file_is_listed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let here = include_str!("it.rs");
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(dir.join("tests")).unwrap() {
        let entry = entry.unwrap();
        let mut name = entry.file_name().into_string().unwrap();
        if entry.path().join("main.rs").is_file() {
            name.push_str("/main.rs"); // a directory test target, like tests/script_conformance/
        }
        if name.ends_with(".rs") {
            let listed = here.contains(&format!("#[path = \"{name}\"]"))
                || manifest.contains(&format!("path = \"tests/{name}\""));
            if !listed {
                missing.push(name);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "add these to iwbtg-format's tests/it.rs (or Cargo.toml): {missing:?}"
    );
}
