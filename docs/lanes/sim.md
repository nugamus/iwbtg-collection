# Lane: sim

**Owns:** `crates/sim`, `crates/tools`, `docs/architecture/sim.md` and `docs/architecture/sim/` (including the register pages), `replay.md`, `traces.md`, `docs/format/converter-requests.md`, and `docs/plans/sim-core.md` as history. The infra lane owns `.github/` and `.claude/`; touch them here only when the issue assigns it and the infra lane agrees.
**Never edits:** `crates/client` except when an issue says so.

**Live text:** `docs/architecture/sim.md` and `docs/architecture/sim/` (the area pages and `register-*.md`). Converter requests are in `docs/format/converter-requests.md`. `docs/plans/sim-core.md` is history only.

## Build and test

- One test file is one module of the `it` binary: `cargo test -p iwbtg-sim --test it <file>::`; unit tests: `--lib <module>::`. Files are named after the module they test (for example `kid_b_core.rs`, `builtins_water.rs`); shared helpers are in `support.rs`, per-group helpers in `*_support.rs` modules, and collision helpers in `collide_support/`.
- Names name the subject, not the history: test files, modules and targets are named after what they test, and a module's docs describe the behavior it covers, not who owns it. A WP-era id (a `WP` followed by a number) stays only where it explains history a reader needs (the historical `docs/plans/sim-core.md`, or a comment that names the work a later change replaced); new code, tests and docs carry none. `cargo test -p xtask no_new_wp_era_identifiers` ratchets the remaining ones down.
- Determinism: `--test determinism`, `--test golden_corpus`.
- A deliberate change to hashed state re-pins three things (and only then) with `cargo xtask repin`: `tests/hash_vectors.rs`, the `FINAL_HASH` in `tests/determinism.rs` and the golden corpus. `HASH_LAYOUT` changes only with the encoding. A merge conflict in a pin file (`crates/xtask/pin-files.txt`): take either side and repin, never hand-merge.
- Script conformance: `UPDATE_CONFORMANCE=1 cargo test -p iwbtg-sim --test it script_conformance::`.

## Test files by module

`tests/it.rs` is the one executable; each other file is one module in it. The separate targets are `determinism`, `golden_corpus`, `feel` and `script_prep`. Files, grouped by the module they cover:

- sim core, tick and world: `world.rs`, `world_lifetime.rs`, `pipeline.rs`, `lifecycle.rs`, `input.rs`, `first_step_output.rs`, `entity_identity.rs`, `pairs.rs`.
- hash and determinism: `determinism.rs`, `hash_vectors.rs`, `golden_corpus.rs`, `golden/`, `sim_version_bump.rs`, `sim_version_pins.txt`.
- rooms: `room_persistent.rs`, `room_requests.rs`, `room_rows.rs`, `room_sequence.rs`, `room_start.rs`, `room_transaction.rs`, `placed_ids.rs`, `persistence.rs`.
- kid: `kid_a1_actions.rs`, `kid_a1_boundary.rs`, `kid_a1_collisions.rs`, `kid_a1_death.rs`, `kid_a1_gravity.rs`, `kid_a2.rs`, `kid_a3.rs`, `kid_a4.rs`, `kid_a5.rs`, `kid_api.rs`, `kid_b_collisions.rs`, `kid_b_core.rs`, `kid_b_death.rs`, `kid_b_flip.rs`, `kid_b_preset.rs`, `kid_b_slope.rs`, `kid_c.rs`, `kid_c_key.rs`, `kid_flip_swap.rs`, `kid_ice.rs`, `kid_ladder.rs`, `kid_moving_block.rs`, `kid_script_slots.rs`, `kid_slope.rs`, `kid_temp.rs`, `kid_water_setters.rs`, `slip.rs`.
- built-ins: `builtins_<kind>.rs`, one per built-in, with `builtins_*_support.rs` for shared helpers.
- collision: `bbox.rs`, `grid.rs`, `query.rs`, `shapes.rs`, `collide_support/`.
- motion, numbers and rng: `motion.rs`, `path.rs`, `num.rs`, `rng.rs`, `bounce.rs`, `particles_rng.rs`.
- sound: `sound.rs`, `death_sound.rs`, `dll_sound_length.rs`, `kid_sounds.rs`.
- scripts: `script_conformance/`, `script_derived_kinds.rs`, `script_doc_examples.rs`, `script_engine.rs`, `script_host.rs`, `script_int_args.rs`, `script_prep.rs`, `script_retry.rs`, `script_save.rs`, `script_validate.rs`.
- save, restart, timing and view: `save.rs`, `save_reverse.rs`, `restart.rs`, `room_restart.rs`, `kid_restart.rs`, `multikid_restart.rs`, `timing.rs`, `ranked_time.rs`, `view.rs`.
- probe, traces and feel: `motion_traces.rs` (private research fixtures), `feel.rs`, `feel/`.
- shared fixtures and lints: `support.rs`, `state_support.rs`, `room_support.rs`, `kid_a2_a3_support.rs`, `kid_a4_a5_support.rs`, `kid_motion_support.rs`, `fixtures/`, `stress/`, `lint_gm_compare.rs`.

## Pitfalls seen before

- Preserve feel, not implementation. An F3 detail (a package-authored boot room, temp file, internal id, probe-only value) is free to redesign; an F1 change needs a feel contract and, where irreversible, the user. Standard ruleset plus named `[compat]` profiles; one-game behavior goes to data, a script or a profile, never a shared-code switch ([0005](../decisions/0005-standard-mechanics.md)).
- Launch the client only with `--mute` (`cargo run -p iwbtg-client -- <pkg> --mute`, or `IWBTG_MUTE=1`): an unmuted test run played music on the user's speakers for 20 minutes (#712).
- Clean room: behavior comes only from `../iwbtg-collection-research/specs/` and its traces. The original's implementation is evidence, not the design. If the spec does not say: F1 → file `needs-user` with a default; F2/F3 → use the standard model and file the deviation. Never clone a quirk just because a probe column exposes it.
- Raw `.get()` or `Real` comparisons in `src/kid` and `src/builtins` fail the gm-compare lint unless they use `num::gm_*` or carry `gm-compare: ok (<why>)`.
- `builtins/state.rs`'s `BuiltinState` enum is append-only (its variant order is in the hash): on a merge conflict keep both new variants, in merge order.
- No floats from `std` math, clocks, IO, threads or `HashMap` in the sim (`crates/sim/clippy.toml`).
