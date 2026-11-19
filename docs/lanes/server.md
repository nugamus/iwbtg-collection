# Lane: server

**Owns:** `crates/server/**` (the `iwbtg-server` binary, its router and tests), the `crates/server` entry in the root `Cargo.toml` `members`, `docs/server.md`.
**Never edits:** `crates/sim`, `crates/format`, `crates/client`, `crates/tools`, `.github/workflows` except when an issue says so.

## Build and test

- `cargo test -p iwbtg-server`: every test lives in one executable (`tests/it.rs`, #517); one file: `cargo test -p iwbtg-server --test it <file>::`.
- `cargo xtask check` once before the PR.
- A new worktree's first build is cold: the crate compiles axum, tokio and tower from scratch, so it takes minutes, not seconds.
- Tests drive the real router in-process through `tower::ServiceExt::oneshot` (`iwbtg_server::app()`), so they bind no port. Only `cargo run -p iwbtg-server` listens (default `127.0.0.1:8080`, `--bind`/`IWBTG_BIND`).

## Pitfalls seen before

- `crates/xtask/src/main.rs` maps folders to crates in `CRATES`: a new crate needs both the root `Cargo.toml` `members` entry and a `CRATES` line, or `cargo xtask check` skips it (#152).
- `CRATES` lists `iwbtg-sim` and `iwbtg-format` as server dependents only once the verify worker links them; until then, editing sim or format neither rebuilds nor tests the server.
- No SQLite, accounts, submissions or verify worker yet: stack decisions live in `docs/server.md`, and SQLite waits for #153.
- `cargo deny` (licenses; sim bans) runs in CI, not locally, so a new dependency's license shows up as a red CI job, not a local error.
