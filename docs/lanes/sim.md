# Lane: sim

**Owns:** `crates/sim`, `crates/tools` (except `pkg_cmd.rs`), `.github/`, `docs/architecture/sim.md` and `docs/architecture/sim/`, `replay.md`, `traces.md`, `docs/plans/sim-core.md` and `docs/plans/sim-core/`.
**Never edits:** `crates/client` except when an issue says so.

## Build and test

- One former test file: `cargo test -p iwbtg-sim --test it <file>::`; unit tests: `--lib <module>::`.
- Determinism: `--test wp1_determinism`, `--test golden_corpus`.
- A deliberate change to hashed state re-pins three things (and only then) with `cargo xtask repin`: `tests/hash_vectors.rs`, the `FINAL_HASH` in `tests/wp1_determinism.rs` and the golden corpus. `HASH_LAYOUT` changes only with the encoding.
- Script conformance: `UPDATE_CONFORMANCE=1 cargo test -p iwbtg-sim --test it script_conformance::`.

## Pitfalls seen before

- Clean room: behavior comes only from `research/specs/` and its traces. If the spec doesn't say, mark it OPEN and ask for a spec item.
- Raw `.get()` or `Real` comparisons in `src/kid` and `src/builtins` fail the gm-compare lint unless they use `num::gm_*` or carry `gm-compare: ok (<why>)`.
- `builtins/state.rs`'s `BuiltinState` enum is append-only (its variant order is in the hash): on a merge conflict keep both new variants, in merge order.
- No floats from `std` math, clocks, IO, threads or `HashMap` in the sim (`crates/sim/clippy.toml`).
