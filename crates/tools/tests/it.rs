//! Every integration test of iwbtg-tools in one executable (#517): one test binary links and
//! starts much faster than one per file. Each former `tests/<file>.rs` is a module here, so its
//! tests are named `<file>::...`; run one file with `cargo test -p iwbtg-tools --test it <file>::`.
//!
//! A new test file goes into this list (`autotests = false` in Cargo.toml, so cargo won't find it alone).

#[path = "assets.rs"]
mod assets;
