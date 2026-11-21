# Feel contracts

Part of [Architecture](../architecture.md). Status: specified by [decision 0005](../decisions/0005-standard-mechanics.md), accepted 2026-11-21. The runner and the file format are specified here; the implementation is the harness issue [#1161](https://github.com/nugamus/iwbtg-collection/issues/1161). Until it lands, trace diffs and golden hashes remain the only automated evidence.

A **feel contract** is data: a scenario plus assertions over what a player can observe. It is the evidence that a mechanic still feels the same after a redesign, and the gate for an F1 change ([fidelity and feel tiers](../plans/fidelity-tiers.md)).

It complements, and does not replace, three existing checks:

| Check | Proves | Does not prove |
|---|---|---|
| Determinism and golden hashes (`crates/sim/tests/`) | The same build and inputs give the same state everywhere. | That the state is the one the player expects. |
| Trace diffs (`iwbtg trace`, `cargo xtask fidelity`) | Every probed column matches the original's recorder on the traced path. | Which column a player can observe; anything off the traced path. |
| **Feel contract** | The player-observable outcome holds: position envelope, timing window, death, reset scope, control latency, RNG on a played path, feedback. | Bit-exact internal state; every probe column. |

A trace is evidence for a feel contract, not a substitute for one. Conversely, a contract never excuses a changed hash: a deliberate state change still re-pins `hash_vectors`, `FINAL_HASH` and the golden corpus, and a ranked outcome change follows the ruleset migration in [versioning](versioning.md).

## What a contract contains

- **Scenario.** A package or a public fixture, a start point, a seed, mods, an input script and room changes. A `save = { room = ..., x = ..., y = ... }` table starts the run from a checkpoint (`StartPoint::FromSave`); the run then starts in `save.room`, which must agree with `start.room` when both are given.
- **Assertions.** Values the player can observe, with an explicit tolerance.
- **Evidence.** The trace case, spec item, or playtest that justifies the assertion. An F1 contract names at least one trace or user playtest where one exists; an F2 contract names a screenshot or audio check.
- **Impact.** F1, F2 or F3. An F3 contract is unusual: an invisible detail belongs in a hash or a unit test, not a feel contract.
- **Ruleset and compatibility profile.** The contract asserts behavior inside one ruleset and profile. Both fields are optional in the harness v1: absent means the package's own ruleset/profile. Moving a port to `standard` writes a new contract; it does not delete the old one while the old board is live.

## File shape

Contracts live beside the trace cases, in `crates/tools/feel/<area>/<name>.toml` (package ids, room and kind names, no creator content). Public contracts that CI can run without the private repos live in `crates/sim/tests/feel/*.toml` and use the same fixture builder as the golden courses.

```toml
# crates/tools/feel/lifecycle/retry_after_death.toml
id = "lifecycle/retry_after_death"       # must equal the path below the root
impact = "F1"
package = "showcase"                     # a package id, or "fixture:<name>"
ruleset = "gm8@0"
profile = "gm8/a1@1"
start = { new_game = true, difficulty = 1 }   # or { room = "<name>", difficulty = ... }
play_mode = "original"                   # original | stripped
seed = 7
mods = []
evidence = ["research:rely-the-needle-4-2/tr-room-01-a", "#670"]

# `.inputs` frame f is the step that ends at World::tick == f + 1.
inputs = """
frame 20: press suicide
frame 21: release suicide
frame 40: press restart
frame 41: release restart
frame 60: hold jump
frame 70: release jump
frame 90: end
"""

[[assert]]
tick = 21                               # World::tick == 21, sampled after Sim::step
death = 1
room = "courtyard"

[[assert]]
tick = 41
room = "courtyard"
kid_count = 1
kid.x = { exact = 96.0 }
kid.y = { exact = 576.0 }

# The retry delay: the pressed jump at frame 41 must not move the kid.
[[window]]
ticks = [41, 42]
metric = "kid.y"
max_delta = 0.0
why = "the retry delay must not move the kid before control returns"

[[assert]]
tick = 43
control = { action = "jump", metric = "kid.y", latency_max = 1 }   # the first movable tick responds
why = "a jump pressed on the first movable tick moves the kid within one frame"

[[assert]]
tick = 90
death = 1
status = "running"
```

The input script uses the `.inputs` grammar ([replay](replay.md#text-inputs-for-tests-inputs)), so the same file can be turned into a replay by the runner. A bare `x`/`y` is a load error: every metric is a probe-registry name (`kid.x`, `kid.y`, `kid.hspeed`, `kid.djump`, `global.death`, `global.time`, `instance_number(kid)`, `save.x`, `save.room`, `random_get_seed()`), so a contract and a trace use one vocabulary.

### Assertion vocabulary

| Key | Meaning |
|---|---|
| `tick` / `window.ticks` | `World::tick == N`, sampled after `Sim::step`, or the closed tick window for an envelope. |
| `room`, `kid_count`, `death`, `time`, `clear`, `status`, `map` | Run state and counters. `room` is the name; `kid_count` is alive instances intersecting `ClassMask::KID | KID_FLIPPED`; `death` is `RunGlobals.deaths`; `time` is `RunGlobals.time`; `clear` is `RunGlobals.clear`; `status` is `running`, `ended` or `halted`. |
| `kid.x`, `kid.y`, `kid.hspeed`, `kid.vspeed`, `kid.djump`, `global.death`, `global.time`, `instance_number(kid)`, `save.x`, `save.room`, `random_get_seed()` | A metric from `probe::registry::column`. A bare `x`/`y` is a load error. Values are `{ exact = v }`, `{ min = v, max = v }`, or `{ within = w, of = c }`; `within` is pixels or speeds, never ULP. A bare scalar is shorthand for `exact` (`save.room = "play"`). |
| `window.metric`, `window.max_delta`, `window.envelope` | A trajectory envelope over a tick range. An envelope has exactly `b-a+1` `[min, max]` pairs; `max_delta` is against the value at the window's first tick. |
| `control.action`, `control.metric`, `control.latency_max` | `latency = response_frame - press_frame`, with 0 meaning the pressed step itself responded. `response_frame = response_tick - 1`, the `.inputs` frame of the step that ended at `response_tick`. The runner finds the latest press at or before `tick - 1` (or the declared `press_frame`), verifies it actually presses the action, or fails "scenario out of sync". |
| `reset_scope.before`, `reset_scope.after`, `reset_scope.persists`, `reset_scope.resets` | Names of counters/globals compared at the explicit `before` and `after` ticks (the `after` defaults to the assert's tick). `persists` names compare equal; `resets` names differ. |
| `sound.event`, `sound.sound`, `sound.tick` | One of `play`, `stop`, `stop_all`, `volume`, `pitch` (the `SoundOutput` events), a `SoundTarget::Builtin` id and its tick. `sound.tick` is the tick the emitting step began at: the step that starts at tick N emits tick-N events and ends at N+1, and `Sim::new`'s first-room output is tick 0. |
| `rng.stream`, `rng.seed`, `rng.value` | v1 supports `stream = "gameplay"` and the signed state (`Rng::seed`, as `random_get_seed()` prints it). Named streams arrive with Area I. |
| `matches_save` | Maps `kid.x`→`save.x`, `kid.y`→`save.y`, `room`→`save.room` for a save/load assertion. |
| `trace` | The trace case that is the primary evidence for this assertion. |
| `screenshot` | **Not a sim assertion.** It is client evidence or a separate UI test; `feel sweep` cannot evaluate it. |

A contract may assert the **absence** of a difference: `retry_delay` zero movement, `kid_count` one, no extra draw on the gameplay stream.

### Units and identity

- **Tick vs frame.** `tick = N` is `World::tick == N`, sampled after `Sim::step`. A `.inputs` frame `f` is the step that ends at `tick == f + 1`: `frame 20: press suicide` is observed at tick 21. Without this rule every trace-derived number is off by one. `tick = 0` is the state `Sim::new` produced, before any step.
- **Save scenario.** A `save` table starts the run from that checkpoint (`StartPoint::FromSave`), which is how a practice run and a retry fixture start. `start.room` and `save.room` must agree.
- **DLLs.** A contract runs with the modeled DLL adapters (`DllMode::Model`), like play, not the recorder's `--stub-dlls` mode. A contract asserts player behavior, not replayer behavior.
- **The Kid.** `kid_count` counts alive, active kids. A bare `kid` in `control` means the `World::first_of` kid (R21); more than one kid needs an explicit `kid#<n>`.
- **Exact floats.** A position `exact` compares the f64 bits. Use it only for a checkpoint or a placed coordinate whose value is a contract; a moving Kid uses an envelope or a window.
- **Names.** `room` is the package room name; `reset_scope` names a counter/global as the probe registry resolves it; `sound.sound` is the package sound name. A name that does not resolve is a contract load error, not a silent skip.
- **Status.** `running` is normal; `ended` is the run's end state; `halted` is a `SimError`, asserted only in an error-path test. `clear` is separate (`RunGlobals.clear`).

### Tolerances

- **Exact** is the default for counters, rooms, deaths, state hashes and the standard ruleset's saves; it compares raw bits.
- **A window or envelope** is allowed only where the player's perception is a window (a jump apex, a fade, a control delay) or where a trace/playtest records a legal range. `within`/`envelope` are pixels or speeds, never ULP. Every non-exact assertion has a one-line `why`.
- **No contract may widen a tolerance to hide a regression.** A widened tolerance is a change to the evidence and is reviewed like a pin change; it is recorded in the game's deviation log.
- **An F1 contract with no `evidence` loads but reports `OPEN`.** `--bless` rewrites statuses only, never assertion values. A tolerance or informational entry on a trace case's `required` column is a load error; an F1 trace needs a linked contract whose metrics are all `required`.

## Running contracts

The runner is one command over a directory of contracts, like the fidelity sweep:

```sh
# One contract, with the same mismatch line the trace diff prints.
cargo run -q -p iwbtg-tools -- feel run crates/tools/feel/lifecycle/retry_after_death.toml

# Every contract, with a per-contract OK/OPEN/FAIL line and a baseline.
cargo run -q -p iwbtg-tools -- feel sweep --contracts crates/tools/feel \
  --packages "$IWBTG_GAMES" --baseline crates/tools/feel-baseline.txt

# The xtask wrapper: builds the tool, runs the public set, and with IWBTG_GAMES
# runs the port contracts too.
cargo xtask feel
```

- A mismatch prints `mismatch: tick 41, kid.y: expected 576.0, got 579.0` plus five ticks of context, like the trace diff.
- The public set (`crates/sim/tests/feel/{lifecycle,physics,collision}/*.toml`) also runs as `cargo test -p iwbtg-sim --test feel`, so every CI target covers the format and the runner without the private repos.
- The private set runs like `cargo xtask fidelity`, but the packages come from the **games repo** (`IWBTG_GAMES`), where the hand-worked rooms and kinds live. `IWBTG_RESEARCH` is the trace and evidence repo only: it supplies the originals a contract cites in `evidence`, never a feel package. `--bless` updates the baseline. A missing package, a contract that fails to load and a status worse than the baseline are FAIL, never a skip.
- A new contract is `OPEN` until it passes. A broken contract is filed with the F tier and the game's deviation log, not deleted.
- CI runs the public set on every sim/format change and the private set is run by the lead after merge waves and before a game is ranked, exactly as the trace sweep is today ([testing](testing.md)).

### Runner layout

- `crates/sim/tests/feel/mod.rs`: `Contract`, `evaluate()`, `Verdict { Ok, Open, Fail }`, `Mismatch`; it reads `Sim`/`World` and changes no sim state. The sim test module holds it for now so `crates/sim/src` stays with its owner this wave; #1172 moves it to `crates/sim/src/feel.rs`, which is the target path.
- `crates/sim/tests/feel.rs` plus the `[[test]]` entry in `crates/sim/Cargo.toml` (the crate has `autotests = false`) and `toml` as a dev-dependency.
- `crates/sim/tests/feel/fixtures.rs`: the public fixture worlds, built with `iwbtg_sim::def::testing::GameDefBuilder` (the golden courses' builder).
- `crates/tools/src/feel_cmd.rs`: `feel run` / `feel sweep`, including `#[path]`-included access to the contract module; `crates/tools/src/baseline.rs` extracts the status/comparison code shared with `sweep_cmd.rs`.
- `crates/tools/feel/<area>/<name>.toml` and `crates/tools/feel-baseline.txt`; `id` must equal the path below the root.
- `cargo xtask feel [--bless]`, exit 0 for OK/OPEN, 1 for FAIL/regression, 2 for a tool error or an unreadable root/baseline.

The harness changes no behavior: no `SIM_VERSION`, `HASH_LAYOUT` or `REPLAY_FORMAT` bump, no board-key change, and contracts or the baseline are **not** pin files (`cargo xtask repin` never rewrites a contract). The later board-key item (ruleset and compatibility profile in `gameplay_hash`) is Area L, blocked by #933/#928/#930.

## How contracts, traces and boards fit together

1. **Feel contract** states the outcome. It is the reviewable promise.
2. **Trace diff** supplies the F1 evidence on the original's path. It may be bit-exact; the contract may allow a tolerance the player cannot perceive.
3. **Golden hash and determinism** prove the build is reproducible and that the contract ran the intended content.
4. **Replay** is the ranked unit: the sim replays the inputs, the server re-runs them and stores the verified ticks. A contract can emit a replay for a scenario that is not a full run; that replay is test data, not a board entry.
5. **Board key** is unchanged: `(package id, map, mods, gameplay_hash)`. The ruleset and its `[compat]` profile are part of `gameplay_hash`, so a behavior change is a new board; a contract does not enter the hash. Old replays verify with the build/ruleset they were made on ([versioning](versioning.md)).

## Writing a contract

1. Name the player-observable outcome, not the implementation. "R returns control at the checkpoint after the retry delay" beats "the boot tick runs".
2. Use the narrowest scenario that exercises it: one room, one input, one assertion.
3. Prefer exact values where the trace is exact. Use an envelope only for a perceived window, and say why.
4. Add the trace or playtest as `evidence`; an F1 contract without evidence is OPEN.
5. Run it under the package's current ruleset and compatibility profile, then under the target one. A difference is the redesign's cost; it is accepted, narrowed or dropped with the user.
6. Keep the contract after the port moves: the old profile's board still needs its promise.

## What this does not do

- It does not remove trace tests; they become the evidence for F1 rather than the only gate.
- It does not bless a state-hash change; a deliberate sim change still re-pins with `cargo xtask repin`.
- It does not let a package change runner behavior; a contract runs inside a ruleset, it does not define one.
- It does not replace the user's playtest; a playtest can add or narrow a contract, and an F1 redesign still asks the user where the outcome is irreversible.
