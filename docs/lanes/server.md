# Lane: server

**Owns:** `crates/server/**` (the `iwbtg-server` binary, its router and tests), the `crates/server` entry in the root `Cargo.toml` `members`, `docs/server.md`.
**Never edits:** `crates/sim`, `crates/format`, `crates/client`, `crates/tools`, `.github/workflows` except when an issue says so.

## Build and test

- `cargo test -p iwbtg-server`: every test lives in one executable (`tests/it.rs`, #517); one file: `cargo test -p iwbtg-server --test it <file>::`.
- `cargo xtask check` once before the PR.
- A new worktree's first build is cold: the crate compiles axum, tokio and tower from scratch, so it takes minutes, not seconds.
- Tests drive the real router in-process through `tower::ServiceExt::oneshot` (`iwbtg_server::app()`), so they bind no port. Only `cargo run -p iwbtg-server` listens (default `127.0.0.1:8080`, `--bind`/`IWBTG_BIND`).

## Pitfalls seen before

- Selection comes from the Cargo manifests through `scripts/ci_scope.py`, shared by CI and `cargo xtask check`: a new crate needs its root `Cargo.toml` `members` entry and dependencies, with no separate `CRATES` list to update (#1028). A manifest change selects full coverage until the graph is consistent.
- The server links `iwbtg-sim` for replay decoding (#162) and verification (#163), so editing sim rebuilds and tests it. The verification core itself is `crates/verify` (`iwbtg-verify`, #1565), shared with `iwbtg replay verify`: the server's child must call it, never copy it. A manifest edge also changes the plan's consumers, and the hard-coded expectations in `crates/xtask/src/main.rs` and `scripts/test_ci_scope.py` must be updated in the same PR (the dependency edge's PR owns that, as #162 did).
- SQLite (#153), accounts (#156/#157/#160), the Argon2 bound (#1505), rate limiting (#161) and replay submissions (#162) are in. The verify worker is #163 and the copy/seed flags are #164.
- `cargo deny` (licenses; sim bans) runs in CI, not locally, so a new dependency's license shows up as a red CI job, not a local error.
- Any dependency change must regenerate `THIRD_PARTY_LICENSES` with the pinned `cargo-about` in the same PR: `cargo about generate --locked --output-file THIRD_PARTY_LICENSES about.hbs`. The `licenses` job's freshness check fails otherwise (#976, #160).
- Preserve feel, not implementation: a change that can alter a verified outcome or a board key is F1 and needs a replay/board migration and the user where the outcome is irreversible; an internal refactor is F3 ([0005](../decisions/0005-standard-mechanics.md), [versioning](../architecture/versioning.md)).
