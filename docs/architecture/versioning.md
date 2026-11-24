# Versioning: SIM_VERSION, engine_rev, gameplay_hash and format

Part of [Architecture](../architecture.md). Status: decided 2026-11-21 (#933). A board does not split on a `SIM_VERSION` bump: records are re-verified under the settings frozen at ranking, and a record that fails is quarantined rather than deleted. The replay-compatibility freeze is the first public nightly of phase 7.

The goal: a replay recorded today still verifies, with the same time, years from now, and an engine fix never silently changes a board.

## What identifies a run's behavior

Four identifiers, each with one job, plus the proposed ruleset and compatibility profile ([0005](../decisions/0005-standard-mechanics.md)). A replay's header records all of them ([replay](replay.md#replay-file-iwbr)).

| Identifier | Where | What changes it | Who reads it |
|---|---|---|---|
| `SIM_VERSION` | `crates/sim/src/lib.rs` | A sim build that could give a different outcome for some existing package on its own ruleset revision: a Rhai or toolchain upgrade, a state hash encoding change (`HASH_LAYOUT`), a replay decoding change, or any bug in rev N's behavior fixed in place. | The verifier: a replay is verified by a build with its `SIM_VERSION`. |
| `engine_rev` | the package's `game.toml` | The format-0 name for the built-in/ruleset revision. The package moves to a newer revision by the porter, or automatically when provably safe ([content](../content.md#engines)). At format 1, `ruleset@N` is the explicit form; `engine_rev` remains the alias. | The sim: selects rule switches and code branches. Covered by `gameplay_hash`. |
| `ruleset` | the package's `game.toml` (proposed, 0005) | `gm8@<rev>` (compatibility) or `standard@<rev>` (new games), including the built-in revision. Absent in format 0 = `gm8@<engine_rev>`. | The sim: selects the runner implementation. Covered by `gameplay_hash`. |
| `[compat] profile` | the package's `game.toml` (proposed, 0005) | The package names an engine-defined compatibility bundle inside its ruleset and its declared deviations. It does not supply runner-rule values. | The sim and the board (through `gameplay_hash`). |
| `gameplay_hash` | computed by `iwbtg pkg hash` | Any gameplay edit to the package, or its ruleset, compatibility profile or deviations ([format v0](../format/versioning.md)). | Boards. Cosmetic edits keep it. |
| `format` | the package's `game.toml` (`FORMAT_VERSION` in `crates/format/src/manifest.rs`, 1 since #795) | The package file layout or schema. | The loader only. It never changes an outcome: every format version loads into the same `GameDef`. |

**Ruleset and profile are content, not builds.** A compatibility profile names an engine-defined bundle; it cannot supply a new runner-rule value. A new value is a ruleset change and a client release. An absent `ruleset`/`[compat]` in a format-0 package means `gm8` at its `engine_rev`, with the profile inferred from the physics preset, so no existing `gameplay_hash` moves. A package that opts into `standard` or changes its profile gets a new `gameplay_hash` and therefore a new board; the old board and its replays stay on the old ruleset/build. `SIM_VERSION` remains build-level and is not part of the board key.

## What a board keys on

**Decided (2026-11-21, #933): (package id, map, mods, `gameplay_hash`), not `SIM_VERSION`.** Since `gameplay_hash` covers the ruleset (including its built-in revision) and the compatibility profile, a board pins the package's content, its semantic contract and its deviation set. `SIM_VERSION` is a property of how a run was verified, not of the board: after the freeze point every build must reproduce every older build's outcomes for the revisions it already knew, and the bump check below enforces that for the golden corpus. Where a new build cannot (the rare `SIM_VERSION` bump), each record is re-verified on the new build; a failure is quarantined for review and stays verifiable on the build it was made with ([Re-verification](#re-verification-after-a-rebuild)).

**Ranked settings freeze at ranking.** Once a game is ranked, its ruleset, `[compat]` profile, declared deviations and physics tuning do not change; only the engine build under it can. A rebuild therefore re-verifies the same `gameplay_hash` on a new build. A package change that moves those settings is a new `gameplay_hash` and a new board, with the old board and its replays kept.

Keying on `SIM_VERSION` as well would split every board at every Rhai or toolchain upgrade, which players would see as their records vanishing for a change that touched no game. The cost of the decision is that the server must keep old verifier builds and run a re-verification pass after a bump (below).

## When `SIM_VERSION` bumps

- **Before the freeze point** it normally stays `1`; outcome-changing PRs re-pin the golden hashes with `cargo xtask repin`. A state-hash encoding change still bumps it even before the freeze: `HASH_LAYOUT` 3 shipped as `SIM_VERSION` 2 (#1157/#1292, bumped by #1303), because a verifier that only compared hashes could not tell "wrong build" from "wrong run". `iwbtg replay verify` reports a `sim_version` mismatch and stops before any hash comparison for a replay from another build. Replays recorded before the freeze (local replays and PBs, #476) may stop verifying with a later build; their times stay in the local stats.
- **The freeze point** is when `SIM_VERSION_FREEZE` in `crates/sim/src/lib.rs` changes from `None` to `Some(SIM_VERSION)`. **Decided (2026-11-21, #933): the first public nightly of phase 7**, the first build whose runs reach a public board. Until then, counting versions only creates history nobody needs.
- **After the freeze point** every change that alters an outcome of an existing package on its current `engine_rev` bumps `SIM_VERSION`. A fix that should change outcomes goes under a new `engine_rev` instead (next section), so that old revisions, the golden corpus and `SIM_VERSION` all stay as they were. A bump is therefore rare and means "this build cannot verify older replays": Rhai, Rust or `libm` upgrades, a hash encoding change, a replay decoding change. A bump never moves a board to a new key; it triggers the re-verification pass below.

### Re-verification after a rebuild

On a `SIM_VERSION` bump after the freeze, the server re-simulates and re-verifies every stored record with the new build, under the game's ranked settings frozen at ranking time. The package, ruleset, profile and deviations do not change; only the engine build does.

- **A record that still verifies keeps its place.** There is no resubmission and no re-upload.
- **A record that fails is quarantined, not deleted.** It stays stored, remains verifiable on the build it was set on (the kept verifier build below), and is surfaced for review. The default for a failure is review, never automatic removal.
- **The pass is queued per board and per bump**, resumes after a restart, and records the build each record was verified with. A board is not re-verified again until the next bump ([#929](https://github.com/nugamus/iwbtg-collection/issues/929)).

### The bump check

`crates/sim/tests/sim_version_bump.rs` (`cargo test -p iwbtg-sim --test it sim_version_bump::`), also run by `cargo xtask repin`:

- Before the freeze point the pin check is inert: `sim_version_pins.txt` holds no pins and no pin difference is enforced. One guard is still active: `a_hash_layout_change_comes_with_a_sim_version_bump` fails when `HASH_LAYOUT` and `SIM_VERSION` move apart, so a hash-encoding change without the required bump is caught locally even before the freeze.
- After it, `sim_version_pins.txt` records every golden pin (the `FINAL_HASH` of `determinism.rs`, the state hash vectors, each golden corpus case's state hash) with the `SIM_VERSION` it was pinned under. A pin that differs while `SIM_VERSION` is unchanged fails the test with the names of the changed pins: bump `SIM_VERSION`, then run `cargo xtask repin`, which records the pins under the new version. `repin` refuses to record over a changed pin without a bump.
- A new golden case or vector needs a `repin` (it is recorded) but no bump. `SIM_VERSION` never goes below the freeze point or the recorded version.
- At the freeze: set `SIM_VERSION_FREEZE = Some(SIM_VERSION)` and run `cargo xtask repin` in the same PR.

The golden corpus runs on `engine_rev` 0. A package on a newer revision is protected by the same rule only once a corpus case uses that revision: every new `engine_rev` adds one (next section).

## A fix: rule switch, code branch, or new verifier

| The fix is | It ships as | Example |
|---|---|---|
| A GM rule we chose wrong, expressible as a setting | A new `RunnerRules` field value, the new value the default from `engine_rev` N | the compare epsilon, a rounding mode, a conversion rule (`crates/sim/src/rules/`, `num/`) |
| Built-in logic that differs, too specific for a rule | A code branch `if engine_rev < N { old } else { new }` in the built-in. More than three revision branches in one built-in: move the old paths into a per-revision function the built-in dispatches to | a collision edge case in one built-in |
| Not branchable: the interpreter, the compiler, a dependency, the hash encoding | A `SIM_VERSION` bump; older replays go to the old verifier build | a Rhai upgrade, a Rust toolchain upgrade that changes float codegen |

Every new `engine_rev` adds a golden corpus case that runs on it, so both sides of each branch are pinned. A package moves to the new revision through `iwbtg engine check` ([cli](cli.md)) and the upgrade rule in [content](../content.md#engines): automatically when every stored replay and trace reproduces (`SAFE`), else on the porter's opt-in.

### A ruleset or profile move

A port stays on `gm8` and its profile until each F1 move is proved by a feel contract and the existing traces ([0005](../decisions/0005-standard-mechanics.md), [feel contracts](feel-contracts.md)). Moving to `standard` or changing the profile is a package change: a new `gameplay_hash`, a new board, and the old board kept with its replays verified on the old build. It is not an `engine_rev` move; `engine_rev` stays inside a ruleset. A profile edit that changes feel is the same kind of change; a profile edit that does not change behavior must normalize to the same hash, or it is treated as a new board. The automatic engine-upgrade rule runs within a ruleset only; a ruleset move is explicit and carries the user's decision where a ranked outcome could change.

### Old verifier builds

**Decided (2026-11-21, #933): the server keeps one verifier build per `SIM_VERSION` that any stored replay still on a board was made with, forever** (they are a few MB each, and a bump is rare). The no-split and quarantine rules depend on it: a quarantined record is re-checked on the build it was set with. Each is:

- built from the git tag `sim-v<N>` with the toolchain pinned in `rust-toolchain.toml` and the committed `Cargo.lock`, for x86_64 Linux;
- stored content-addressed by the SHA-256 of the binary, in a table `SIM_VERSION -> sha256`;
- rebuilt once from its tag when it is added, and kept only if the rebuild is byte-identical, so a lost binary can be rebuilt.

The verifier worker picks the build by the replay header's `sim_version`. Building, storing and dispatching are server work: #929, for #56.

## Format 1 freeze

- **Criteria, met by #795:** every item of [content](../content.md#format-changes-needed), Format changes needed, is done or moved out of format 1 with a reason ([v1](../format/v1.md#what-format-1-does-not-contain)); the 20 converted packages upgrade and pass `iwbtg pkg check --trace-mode`; `FORMAT_VERSION` is 1 with schemas under `docs/format/schema/v1/`. **Default: format 1 freezes no later than the `SIM_VERSION` freeze point**, since public packages and public replays arrive together; until then an additive key may still join format 1. The guard that enforces the freeze, and the user's confirmation of the point, is #1395.
- **Migration rule:** after the freeze, any schema change bumps `format`. `crates/format` loads every older format into the same `GameDef` (principle 7), so an old package always loads; `iwbtg pkg upgrade` rewrites one on disk and refuses to write unless the result loads into an equal `GameDef` ([v1](../format/v1.md#upgrading-a-format-0-package)). Today `gameplay_hash` hashes the parsed TOML of each file, not the `GameDef`, so an upgrade that renames or moves keys changes it even though nothing plays differently. Until #928 makes the hash stable across migrations, the server treats an upgraded package like a SAFE `engine_rev` move: it re-runs the stored replays and, when all reproduce, carries the board over to the new hash.

## Example: a physics bug fixed a year after release

A Ranked game is on `engine_rev` 3. A year later, a spike collision is found to differ from the original by one pixel.

1. The fix lands as `engine_rev` 4: a `RunnerRules` value or an `engine_rev < 4` branch, with a golden case on rev 4. Rev 3's behavior is untouched, so the golden pins on revs 0 to 3 do not move and `SIM_VERSION` stays (the bump check proves it).
2. The game's board, keyed on its `gameplay_hash` (which includes rev 3), is unchanged. Every old replay verifies on the new build, because the new build runs rev 3 exactly as before.
3. The server runs `iwbtg engine check --to 4` on the game. If every stored replay and trace reproduces (no run ever touched that pixel), the game moves to rev 4 and its board carries over. Otherwise it stays on rev 3, and the porter gets the differences; on opt-in, runs that still clear on rev 4 stay on the board and the others move to a legacy board ([content](../content.md#engines)).
4. Only if the fix could not be branched (say it needed a new Rhai) would `SIM_VERSION` bump: the board still stands, and the server verifies older replays with the kept build of their `SIM_VERSION`.

## Follow-ups

- Server: keep and dispatch one verifier build per `SIM_VERSION`, queue re-verification after a bump and quarantine failures: #929 (for #56).
- Server: board carry-over on a SAFE `engine_rev` or format upgrade, and the legacy board: #930 (for #62).
- `gameplay_hash` stable across format migrations: #928.
- `iwbtg pkg upgrade` and format 1: done (#795); the freeze guard: #1395.
