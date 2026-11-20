# Versioning: SIM_VERSION, engine_rev, gameplay_hash and format

Part of [Architecture](../architecture.md). Status: proposed by Claude, 2026-11-17 (#783). Two points are the user's to decide and wait on #933 (`needs-user`): whether a board keys on `SIM_VERSION`, and when the freeze point is. This page states the defaults used until then.

The goal: a replay recorded today still verifies, with the same time, years from now, and an engine fix never silently changes a board.

## What identifies a run's behavior

Four identifiers, each with one job, plus the proposed ruleset and profile ([0005](../decisions/0005-standard-mechanics.md)). A replay's header records all of them ([replay](replay.md#replay-file-iwbr)).

| Identifier | Where | What changes it | Who reads it |
|---|---|---|---|
| `SIM_VERSION` | `crates/sim/src/lib.rs` | A sim build that could give a different outcome for some existing package on its own `engine_rev`: a Rhai or toolchain upgrade, a state hash encoding change (`HASH_LAYOUT`), a replay decoding change, or any bug in rev N's behavior fixed in place. | The verifier: a replay is verified by a build with its `SIM_VERSION`. |
| `engine_rev` | the package's `game.toml` | The package moves to a newer revision of the built-ins (by the porter, or automatically when provably safe, [content](../content.md#engines)). | The sim: selects rule switches and code branches. Covered by `gameplay_hash`. |
| `ruleset` | the package's `game.toml` (proposed, 0005) | The package selects a named runner semantic: `gm8` (compatibility) or `standard` (new games). | The sim: selects the runner implementation. Covered by `gameplay_hash`. |
| `profile` | the package's `game.toml` (proposed, 0005) | The package selects a data bundle inside its ruleset: retry, save precision, ordering, RNG streams, sound adapters, declared deviations. | The sim and the board (through `gameplay_hash`). |
| `gameplay_hash` | computed by `iwbtg pkg hash` | Any gameplay edit to the package, or its `engine_rev`, `ruleset` or `profile` ([format v0](../format/versioning.md)). | Boards. Cosmetic edits keep it. |
| `format` | the package's `game.toml` (`FORMAT_VERSION` in `crates/format/src/manifest.rs`) | The package file layout or schema. | The loader only. It never changes an outcome: every format version loads into the same `GameDef`. |

**Ruleset and profile are content, not builds.** A profile can only set values the ruleset exposes; a new value is a ruleset change and a client release. An absent `ruleset`/`profile` in a format-0 package means `gm8` and the profile inferred from the physics preset, so no existing `gameplay_hash` moves. A package that opts into `standard` or changes its profile gets a new `gameplay_hash` and therefore a new board; the old board and its replays stay on the old ruleset/build. `SIM_VERSION` remains build-level and is not part of the board key.

## What a board keys on

**Default: (package id, map, mods, `gameplay_hash`), not `SIM_VERSION`** (needs-user, #933). Since `gameplay_hash` covers `engine_rev`, `ruleset` and `profile`, a board pins the package's content, its ruleset/profile and its built-in revision. `SIM_VERSION` is a property of how a run was verified, not of the board: after the freeze point every build must reproduce every older build's outcomes for the revisions it already knew, and the bump check below enforces that for the golden corpus. Where a new build cannot (the rare `SIM_VERSION` bump), old replays are verified by the old build they were made with, and their place on the board stands.

Keying on `SIM_VERSION` as well would split every board at every Rhai or toolchain upgrade, which players would see as their records vanishing for a change that touched no game. The cost of the default is that the server must keep old verifier builds (below).

## When `SIM_VERSION` bumps

- **Before the freeze point** it stays `1`. Outcome-changing PRs re-pin the golden hashes with `cargo xtask repin` and nothing else. Replays recorded before the freeze (local replays and PBs, #476) may stop verifying with a later build; their times stay in the local stats, and `iwbtg replay verify` reports `sim_version` mismatch for them.
- **The freeze point** is when `SIM_VERSION_FREEZE` in `crates/sim/src/lib.rs` changes from `None` to `Some(SIM_VERSION)`. **Default: the first build whose runs reach a public board, the first public nightly of M7** (needs-user, #933). Until then, counting versions only creates history nobody needs.
- **After the freeze point** every change that alters an outcome of an existing package on its current `engine_rev` bumps `SIM_VERSION`. A fix that should change outcomes goes under a new `engine_rev` instead (next section), so that old revisions, the golden corpus and `SIM_VERSION` all stay as they were. A bump is therefore rare and means "this build cannot verify older replays": Rhai, Rust or `libm` upgrades, a hash encoding change, a replay decoding change.

### The bump check

`crates/sim/tests/sim_version_bump.rs` (`cargo test -p iwbtg-sim --test it sim_version_bump::`), also run by `cargo xtask repin`:

- Before the freeze point it is inert: `sim_version_pins.txt` holds no pins and nothing is enforced.
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

**Default: the server keeps one verifier build per `SIM_VERSION` that any stored replay still on a board was made with, forever** (they are a few MB each, and a bump is rare). Each is:

- built from the git tag `sim-v<N>` with the toolchain pinned in `rust-toolchain.toml` and the committed `Cargo.lock`, for x86_64 Linux;
- stored content-addressed by the SHA-256 of the binary, in a table `SIM_VERSION -> sha256`;
- rebuilt once from its tag when it is added, and kept only if the rebuild is byte-identical, so a lost binary can be rebuilt.

The verifier worker picks the build by the replay header's `sim_version`. Building, storing and dispatching are server work: #929, for #56.

## Format 1 freeze

- **Criteria:** every item of [content](../content.md#format-changes-needed), Format changes needed, is done or moved out of format 1 with a reason; the ported packages pass `iwbtg pkg check`; `FORMAT_VERSION` becomes 1 with schemas under `docs/format/schema/v1/`. Tracked in #795. **Default: format 1 freezes no later than the `SIM_VERSION` freeze point**, since public packages and public replays arrive together.
- **Migration rule:** from format 1 on, any schema change bumps `format`. `crates/format` keeps a loader or in-memory migrator for every older format (principle 7), so an old package always loads; `iwbtg pkg upgrade` rewrites one on disk (#795, not built yet). Today `gameplay_hash` hashes the parsed TOML of each file, not the `GameDef`, so an upgrade that renames or moves keys changes it even though nothing plays differently. Until #928 makes the hash stable across migrations, the server treats an upgraded package like a SAFE `engine_rev` move: it re-runs the stored replays and, when all reproduce, carries the board over to the new hash.

## Example: a physics bug fixed a year after release

A Ranked game is on `engine_rev` 3. A year later, a spike collision is found to differ from the original by one pixel.

1. The fix lands as `engine_rev` 4: a `RunnerRules` value or an `engine_rev < 4` branch, with a golden case on rev 4. Rev 3's behavior is untouched, so the golden pins on revs 0 to 3 do not move and `SIM_VERSION` stays (the bump check proves it).
2. The game's board, keyed on its `gameplay_hash` (which includes rev 3), is unchanged. Every old replay verifies on the new build, because the new build runs rev 3 exactly as before.
3. The server runs `iwbtg engine check --to 4` on the game. If every stored replay and trace reproduces (no run ever touched that pixel), the game moves to rev 4 and its board carries over. Otherwise it stays on rev 3, and the porter gets the differences; on opt-in, runs that still clear on rev 4 stay on the board and the others move to a legacy board ([content](../content.md#engines)).
4. Only if the fix could not be branched (say it needed a new Rhai) would `SIM_VERSION` bump: the board still stands, and the server verifies older replays with the kept build of their `SIM_VERSION`.

## Follow-ups

- Server: keep and dispatch one verifier build per `SIM_VERSION`: #929 (for #56).
- Server: board carry-over on a SAFE `engine_rev` or format upgrade, and the legacy board: #930 (for #62).
- `gameplay_hash` stable across format migrations: #928.
- `iwbtg pkg upgrade` and format 1: #795.
