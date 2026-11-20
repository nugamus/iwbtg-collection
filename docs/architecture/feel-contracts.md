# Feel contracts

Part of [Architecture](../architecture.md). Status: proposed by [decision 0005](../decisions/0005-standard-mechanics.md), 2026-11-20. The runner and the file format are specified here; the implementation is the harness issue linked from umbrella #1146. Until it lands, trace diffs and golden hashes remain the only automated evidence.

A **feel contract** is data: a scenario plus assertions over what a player can observe. It is the evidence that a mechanic still feels the same after a redesign, and the gate for an F1 change ([fidelity and feel tiers](../plans/fidelity-tiers.md)).

It complements, and does not replace, three existing checks:

| Check | Proves | Does not prove |
|---|---|---|
| Determinism and golden hashes (`crates/sim/tests/`) | The same build and inputs give the same state everywhere. | That the state is the one the player expects. |
| Trace diffs (`iwbtg trace`, `cargo xtask fidelity`) | Every probed column matches the original's recorder on the traced path. | Which column a player can observe; anything off the traced path. |
| **Feel contract** | The player-observable outcome holds: position envelope, timing window, death, reset scope, control latency, RNG on a played path, feedback. | Bit-exact internal state; every probe column. |

A trace is evidence for a feel contract, not a substitute for one. Conversely, a contract never excuses a changed hash: a deliberate state change still re-pins `hash_vectors`, `FINAL_HASH` and the golden corpus, and a ranked outcome change follows the ruleset migration in [versioning](versioning.md).

## What a contract contains

- **Scenario.** A package or a public fixture, a start point, a seed, mods, an input script and room changes.
- **Assertions.** Values the player can observe, with an explicit tolerance.
- **Evidence.** The trace case, spec item, or playtest that justifies the assertion. An F1 contract names at least one trace or user playtest where one exists; an F2 contract names a screenshot or audio check.
- **Impact.** F1, F2 or F3. An F3 contract is unusual: an invisible detail belongs in a hash or a unit test, not a feel contract.
- **Ruleset and profile.** The contract asserts behavior inside one ruleset/profile. Moving a port to `standard` writes a new contract; it does not delete the old one while the old board is live.

## File shape

Contracts live beside the trace cases, in `crates/tools/feel/<area>/<name>.toml` (package ids, room and kind names, no creator content). Public contracts that CI can run without the private repos live in `crates/sim/tests/feel/*.toml` and use the same fixture builder as the golden courses.

```toml
# crates/tools/feel/lifecycle/retry_after_death.toml
id = "lifecycle/retry_after_death"
impact = "F1"
package = "showcase"                  # a package id, or "fixture:<name>"
ruleset = "gm8"
profile = "a1"
start = { room = "courtyard", difficulty = 1, mode = "new_game" }
seed = 7
mods = []
evidence = ["research:rely-the-needle-4-2/tr-room-01-a", "#670"]

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
tick = 21
death = 1
room = "courtyard"

[[assert]]
tick = 41
room = "courtyard"
kid_count = 1
x = { exact = 96.0 }
y = { exact = 576.0 }

# The retry delay: the pressed jump at frame 41 must not move the kid.
[[window]]
ticks = [41, 42]
metric = "kid.y"
max_delta = 0.0

[[assert]]
tick = 43
control = { action = "jump", latency_max = 1 }   # the first movable tick responds

[[assert]]
tick = 90
death = 1
status = "running"
```

The input script uses the `.inputs` grammar ([replay](replay.md#text-inputs-for-tests-inputs)), so the same file can be turned into a replay by the runner.

### Assertion vocabulary

| Key | Meaning |
|---|---|
| `tick` / `window.ticks` | The tick after which the assertion is read, or the closed tick window for an envelope. |
| `room`, `kid_count`, `death`, `time`, `clear`, `status`, `map` | Run state and counters. `time` is play-timer ticks; `status` is `running`, `cleared` or `halted`. |
| `x`, `y`, `hspeed`, `vspeed`, `image_index` | `{ exact = v }`, `{ min = v, max = v }`, or `{ delta_max = v }` against the value at the window's first tick. |
| `window.metric`, `window.max_delta`, `window.envelope` | A trajectory envelope over a tick range. An envelope is a per-tick `[min, max]` list. |
| `control.action`, `control.latency_max` | Ticks from the press to the first tick the action changes the named metric. This is the control-latency contract. |
| `reset_scope.persists`, `reset_scope.resets` | Names of counters/globals that survive or reset across a retry/new game. |
| `sound.event`, `sound.id`, `sound.tick` | A `play`, `stop`, `fade` or `volume` event and its tick. Used for F1 sound state (`sound_isplaying`) and F2 latency/fade. |
| `rng.stream`, `rng.seed`, `rng.value` | The state or a named draw of a stream at a tick. Used where a draw changes a played path. |
| `trace` | The trace case that is the primary evidence for this assertion. |
| `screenshot` | An F2 image check, with the `--screenshot` path and a tolerance; only for visible feedback. |

A contract may assert the **absence** of a difference: `retry_delay` zero movement, `kid_count` one, no extra draw on the gameplay stream.

### Tolerances

- **Exact** is the default for counters, rooms, deaths, state hashes and the standard ruleset's saves.
- **A window or envelope** is allowed only where the player's perception is a window (a jump apex, a fade, a control delay) or where a trace/playtest records a legal range. Every non-exact tolerance has a one-line `why` in the contract.
- **No contract may widen a tolerance to hide a regression.** A widened tolerance is a change to the evidence and is reviewed like a pin change; it is recorded in the game's deviation log.

## Running contracts

The runner is one command over a directory of contracts, like the fidelity sweep:

```sh
# One contract, with the same mismatch line the trace diff prints.
cargo run -q -p iwbtg-tools -- feel run crates/tools/feel/lifecycle/retry_after_death.toml

# Every contract, with a per-contract OK/OPEN/FAIL line and a baseline.
cargo run -q -p iwbtg-tools -- feel sweep --contracts crates/tools/feel \
  --baseline crates/tools/feel-baseline.txt

# The xtask wrapper: builds the tool, runs the public set, and with IWBTG_RESEARCH
# runs the port contracts too.
cargo xtask feel
```

- A mismatch prints `mismatch: tick 41, kid.y: expected 576.0, got 579.0` plus five ticks of context, like the trace diff.
- The public set (`crates/sim/tests/feel/*.toml`) also runs as `cargo test -p iwbtg-sim --test feel`, so every CI target covers the format and the runner without the private repos.
- The private set runs like `cargo xtask fidelity`: `IWBTG_RESEARCH` points at the research checkout, the packages come from `$IWBTG_RESEARCH/build/packages`, and `--bless` updates the baseline. A missing package, a contract that fails to load and a status worse than the baseline are FAIL, never a skip.
- A new contract is `OPEN` until it passes. A broken contract is filed with the F tier and the game's deviation log, not deleted.
- CI runs the public set on every sim/format change and the private set is run by the lead after merge waves and before a game is ranked, exactly as the trace sweep is today ([testing](testing.md)).

## How contracts, traces and boards fit together

1. **Feel contract** states the outcome. It is the reviewable promise.
2. **Trace diff** supplies the F1 evidence on the original's path. It may be bit-exact; the contract may allow a tolerance the player cannot perceive.
3. **Golden hash and determinism** prove the build is reproducible and that the contract ran the intended content.
4. **Replay** is the ranked unit: the sim replays the inputs, the server re-runs them and stores the verified ticks. A contract can emit a replay for a scenario that is not a full run; that replay is test data, not a board entry.
5. **Board key** is unchanged: `(package id, map, mods, gameplay_hash)`. The ruleset and profile are part of `gameplay_hash`, so a behavior change is a new board; a contract does not enter the hash. Old replays verify with the build/ruleset they were made on ([versioning](versioning.md)).

## Writing a contract

1. Name the player-observable outcome, not the implementation. "R returns control at the checkpoint after the retry delay" beats "the boot tick runs".
2. Use the narrowest scenario that exercises it: one room, one input, one assertion.
3. Prefer exact values where the trace is exact. Use an envelope only for a perceived window, and say why.
4. Add the trace or playtest as `evidence`; an F1 contract without evidence is OPEN.
5. Run it under the package's current ruleset/profile, then under the target one. A difference is the redesign's cost; it is accepted, narrowed or dropped with the user.
6. Keep the contract after the port moves: the old profile's board still needs its promise.

## What this does not do

- It does not remove trace tests; they become the evidence for F1 rather than the only gate.
- It does not bless a state-hash change; a deliberate sim change still re-pins with `cargo xtask repin`.
- It does not let a package change runner behavior; a contract runs inside a ruleset, it does not define one.
- It does not replace the user's playtest; a playtest can add or narrow a contract, and an F1 redesign still asks the user where the outcome is irreversible.
