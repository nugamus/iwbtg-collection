# Lane: sim

**Owns:** `crates/sim`, `crates/tools` (except `pkg_cmd.rs`), `docs/architecture/sim.md` and `docs/architecture/sim/` (including the register pages), `replay.md`, `traces.md`, `docs/format/converter-requests.md`, and `docs/plans/sim-core.md` as history. The infra lane owns `.github/` and `.claude/`; touch them here only when the issue assigns it and the infra lane agrees.
**Never edits:** `crates/client` except when an issue says so.

**Live text:** `docs/architecture/sim.md` and `docs/architecture/sim/` (the area pages and `register-*.md`). Converter requests are in `docs/format/converter-requests.md`. `docs/plans/sim-core.md` is history only.

## Build and test

- One test file is one module of the `it` binary: `cargo test -p iwbtg-sim --test it <file>::`; unit tests: `--lib <module>::`. Files are named after the module they test (for example `kid_b_core.rs`, `builtins_water.rs`); shared helpers are in `support.rs`, per-group helpers in `*_support.rs` modules, and collision helpers in `collide_support/`.
- Determinism: `--test wp1_determinism` (the target keeps that name for CI; its file is `tests/determinism.rs`), `--test golden_corpus`.
- A deliberate change to hashed state re-pins three things (and only then) with `cargo xtask repin`: `tests/hash_vectors.rs`, the `FINAL_HASH` in `tests/determinism.rs` and the golden corpus. `HASH_LAYOUT` changes only with the encoding. A merge conflict in a pin file (`crates/xtask/pin-files.txt`): take either side and repin, never hand-merge.
- Script conformance: `UPDATE_CONFORMANCE=1 cargo test -p iwbtg-sim --test it script_conformance::`.

## Pitfalls seen before

- Preserve feel, not implementation. An F3 detail (a package-authored boot room, temp file, internal id, probe-only value) is free to redesign; an F1 change needs a feel contract and, where irreversible, the user. Standard ruleset plus named `[compat]` profiles; one-game behavior goes to data, a script or a profile, never a shared-code switch ([0005](../decisions/0005-standard-mechanics.md)).
- Launch the client only with `--mute` (`cargo run -p iwbtg-client -- <pkg> --mute`, or `IWBTG_MUTE=1`): an unmuted test run played music on the user's speakers for 20 minutes (#712).
- Clean room: behavior comes only from `../iwbtg-collection-research/specs/` and its traces. The original's implementation is evidence, not the design. If the spec does not say: F1 → file `needs-user` with a default; F2/F3 → use the standard model and file the deviation. Never clone a quirk just because a probe column exposes it.
- Raw `.get()` or `Real` comparisons in `src/kid` and `src/builtins` fail the gm-compare lint unless they use `num::gm_*` or carry `gm-compare: ok (<why>)`.
- `builtins/state.rs`'s `BuiltinState` enum is append-only (its variant order is in the hash): on a merge conflict keep both new variants, in merge order.
- No floats from `std` math, clocks, IO, threads or `HashMap` in the sim (`crates/sim/clippy.toml`).
