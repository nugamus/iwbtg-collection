# Lane: sim

**Owns:** `crates/sim`, `crates/tools` (except `pkg_cmd.rs`), `docs/architecture/sim.md` and `docs/architecture/sim/`, `replay.md`, `traces.md`, `docs/plans/sim-core.md` and `docs/plans/sim-core/`. The infra lane owns `.github/` and `.claude/`; touch them here only when the issue assigns it and the infra lane agrees.
**Never edits:** `crates/client` except when an issue says so.

## Build and test

- One test file is one module of the `it` binary: `cargo test -p iwbtg-sim --test it <file>::`; unit tests: `--lib <module>::`. Files are named after the module they test (`room.rs`, `kid_b_core.rs`, `builtins_drop_flip_edge.rs`); the shared fixtures are `support.rs` and `collide_support/`.
- Determinism: `--test wp1_determinism` (the target keeps that name for CI; its file is `tests/determinism.rs`), `--test golden_corpus`.
- A deliberate change to hashed state re-pins three things (and only then) with `cargo xtask repin`: `tests/hash_vectors.rs`, the `FINAL_HASH` in `tests/determinism.rs` and the golden corpus. `HASH_LAYOUT` changes only with the encoding. A merge conflict in a pin file (`crates/xtask/pin-files.txt`): take either side and repin, never hand-merge.
- Script conformance: `UPDATE_CONFORMANCE=1 cargo test -p iwbtg-sim --test it script_conformance::`.

## Pitfalls seen before

- Launch the client only with `--mute` (`cargo run -p iwbtg-client -- <pkg> --mute`, or `IWBTG_MUTE=1`): an unmuted test run played music on the user's speakers for 20 minutes (#712).
- Clean room: behavior comes only from `../iwbtg-collection-research/specs/` and its traces. If the spec doesn't say, mark it OPEN and ask for a spec item.
- Raw `.get()` or `Real` comparisons in `src/kid` and `src/builtins` fail the gm-compare lint unless they use `num::gm_*` or carry `gm-compare: ok (<why>)`.
- `builtins/state.rs`'s `BuiltinState` enum is append-only (its variant order is in the hash): on a merge conflict keep both new variants, in merge order.
- No floats from `std` math, clocks, IO, threads or `HashMap` in the sim (`crates/sim/clippy.toml`).
