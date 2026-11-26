//! Every integration test of iwbtg-server in one executable (#517): one test binary links and
//! starts much faster than one per file. Each `tests/<file>.rs` is a module here, so its tests are
//! named `<file>::...`; run one file with `cargo test -p iwbtg-server --test it <file>::`.
//!
//! A new test file goes into this list (`autotests = false` in Cargo.toml, so cargo won't find it
//! alone).

#[path = "support.rs"]
mod support;

#[path = "db.rs"]
mod db;

#[path = "healthz.rs"]
mod healthz;

#[path = "catalog.rs"]
mod catalog;

#[path = "removal.rs"]
mod removal;

#[path = "healthchecks.rs"]
mod healthchecks;

#[path = "auth.rs"]
mod auth;

#[path = "admin.rs"]
mod admin;

#[path = "reset.rs"]
mod reset;

#[path = "submissions.rs"]
mod submissions;
