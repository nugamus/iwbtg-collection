# Lane: data

**Owns:** `crates/format` (package loading, validation, schemas), `docs/format/`, `crates/client/src/settings*` and `stats*` (settings file, local stats). The `iwbtg pkg check|fmt|hash|upgrade` commands and the package hashes live in `crates/format/src/pkg_cmd.rs` (#1120); `crates/tools` only registers them.
**Never edits:** the UI kit and screens (ui lane), `crates/sim` internals (sim lane).

## Build and test

- `cargo test -p iwbtg-format --test it <file>::` for one former test file, `--test fixtures` for the validator fixtures, `--lib` for unit tests.
- Schemas are generated: `cargo xtask regen`. Never hand-merge a schema file; regenerate it after merging `main`.
- A package check: `cargo run -p iwbtg-tools -- pkg check <dir>`; canonical formatting: `pkg fmt <dir> --check`.

## Pitfalls seen before

- A package declares a `ruleset` and names an engine-defined `[compat]` profile; it cannot supply runner-rule values or code. A new value is a ruleset change and a client release. A behavior change is a new `gameplay_hash` and board, never a silent edit ([0005](../decisions/0005-standard-mechanics.md)).
- Preserve feel, not implementation: an F3 detail (loader order, internal id, schema-only change) is free to redesign; an F1 change (a default that moves a run's outcome) needs a feel contract and, where irreversible, the user. Every deliberate deviation gets a deviation-log entry and a contract id ([fidelity tiers](../plans/fidelity-tiers.md)).
- Launch the client only with `--mute` (`cargo run -p iwbtg-client -- <pkg> --mute`, or `IWBTG_MUTE=1`): an unmuted test run played music on the user's speakers for 20 minutes (#712).
- `manifest.rs` and `docs/format/game.md` are shared hot spots: merge `origin/main` just before opening the PR.
- Every format change updates `docs/format/` and the schema in the same PR.
- Loading code must stay pure (`crates/format/clippy.toml`): no clocks, no unordered maps in gameplay data.
