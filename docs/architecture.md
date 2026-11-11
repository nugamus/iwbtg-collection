# Architecture

Status: draft for roadmap Phase 3. Accepted by Claude on the user's behalf, 2026-10-24 (user may revisit).

This page describes how the client repo is built: the crates, the simulation, how numbers and collisions work, the first script API, the replay format, and how we prove that a port matches its original.

Related: [0001 script language](decisions/0001-script-language.md), [0002 data syntax](decisions/0002-data-syntax.md), [format v0](format/v0.md), [format principles](format/README.md), [server](server.md), [roadmap](roadmap.md).

Facts about GameMaker 8 behavior come from the engine spec, `research/specs/engines/gm8/CLONE_SPEC.md` (not written yet). Where this page names a GM8 rule, the spec is the authority. Anything marked **OPEN** waits for it.

## Goals

- One simulation, `crates/sim`, runs the game on the client, the leaderboard server and the CLI.
- The simulation is deterministic: the same package and inputs give bit-identical state on x86_64 Windows (MSVC), WASM and x86_64 Linux (the server).
- A port reproduces the original's positions tick for tick. Traces of the original prove it.
- Agents write and maintain all code. Clarity, tooling and guardrails come before cleverness.

## Crates

One Cargo workspace in `client/`, `license = "MIT OR Apache-2.0"`, toolchain pinned to an exact stable in `rust-toolchain.toml`.

| Crate | What it does | Depends on |
|---|---|---|
| `crates/sim` | The deterministic simulation: world, built-ins, kid, collision, tick pipeline, script host, replays, state hash. No Bevy, no IO, no threads, no clock. `#![forbid(unsafe_code)]`. | `rhai` (exact pin), `libm` (exact pin, no default features), `blake3`, `smallvec`, `postcard`, `serde`. Optional later: `rustc_apfloat` (see [numbers](architecture/sim/numbers.md#number-handling)). |
| `crates/format` | Reads, validates, hashes and writes game packages. Turns TOML files into `sim::def::GameDef`. Parses from an in-memory map of path to bytes (`PackageFiles`), so the client, the server and WASM share one code path with no filesystem. | `sim`, `toml`, `toml_edit`, `serde`, `serde_spanned`, `schemars`, `png`, `blake3`, `sha2`. |
| `crates/tools` | The `iwbtg` command-line tool (see below). Later also the music preview picker and the level editor. | `sim`, `format`. |
| `crates/client` | The Bevy app: game select, rendering, input, audio, settings. Owns one `sim::Sim`, feeds it one input frame per tick and draws its snapshots. | `sim`, `format`, `bevy`. |
| `crates/server` | Phase 8. Leaderboard backend. Verifies replays by running `sim` in a child process ([server](server.md)). | `sim`, `format`, `axum`, `sqlx`. |

The sim never parses text and never touches files. `format` hands it a validated `GameDef`. The client and server hand it input frames.
## Pages

Each lane edits its own page, so parallel PRs don't collide here (#518).

- [The client](architecture/client.md): the Bevy app, room art, audio, screen layout.
- [The `iwbtg` CLI](architecture/cli.md): every command and its exit codes.
- [The simulation](architecture/sim.md): sim module layout, sim purity, world model, tick pipeline, number handling, collision, scripts and the script API, random numbers, state hash, sound model.
- [Input and replay format](architecture/replay.md): actions and input frames, the `.iwbr` replay file, `.inputs` test files, verification, versions.
- [Comparing traces with the original](architecture/traces.md): recorder behavior, CSV columns, mapping and diff.
- [Testing strategy](architecture/testing.md).

## Open items

Every GM rule the sim needs that the spec marks UNKNOWN, Inferred or Conflict is tracked with its switch, default and settling trace in the [plan's register](plans/sim-core.md#15-unknown-and-inferred-register). The items below are the broad ones.

- **OPEN:** exact GM8 event order, collision handling for solid instances, destroy timing, room-change timing and 8.0 versus 8.1 differences. Source: `research/specs/engines/gm8/CLONE_SPEC.md`.
- **OPEN:** comparison epsilon, truthiness boundary, rounding and conversion rules, motion coupling details. Source: the same spec.
- **OPEN:** whether the real GM8 runner's trig and precision match OpenGMK's plain f64. Source: Phase 2 cross-check.
- **OPEN:** the built-in kind list and each kind's parameters. Source: `research/specs/mechanics.md` (not written yet).
- **OPEN:** whether any room really runs at a speed other than 50 (editor values 30, 60 and 5 exist, but all 20 games set 50 in code).
- **OPEN:** spike mask type and rotated precise masks.
