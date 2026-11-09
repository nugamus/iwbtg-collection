# Lane: data

**Owns:** `crates/format` (package loading, validation, schemas), `docs/format/`, `crates/client/src/settings*` and `stats*` (settings file, local stats), `crates/tools/src/pkg_cmd.rs`.
**Never edits:** the UI kit and screens (ui lane), `crates/sim` internals (sim lane).

## Build and test

- `cargo test -p iwbtg-format --test it <file>::` for one former test file, `--test fixtures` for the validator fixtures, `--lib` for unit tests.
- Schemas are generated: `cargo xtask regen`. Never hand-merge a schema file; regenerate it after merging `main`.
- A package check: `cargo run -p iwbtg-tools -- pkg check <dir>`; canonical formatting: `pkg fmt <dir> --check`.

## Pitfalls seen before

- `manifest.rs` and `docs/format/v0.md` are shared hot spots: merge `origin/main` just before opening the PR.
- Every format change updates `docs/format/` and the schema in the same PR.
- Loading code must stay pure (`crates/format/clippy.toml`): no clocks, no unordered maps in gameplay data.
