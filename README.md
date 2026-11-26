<div align="center">

# IWBTG Collection

**One client for I Wanna Be The Guy fangames — rebuilt game by game in an open, documented format.**

[![CI](https://github.com/nugamus/iwbtg-collection/actions/workflows/ci.yml/badge.svg)](https://github.com/nugamus/iwbtg-collection/actions/workflows/ci.yml)
[![Status](https://img.shields.io/badge/status-pre--release-fab219)](docs/roadmap.md)
[![Progress](https://img.shields.io/badge/progress-live-3987e5)](https://nugamus.github.io/iwbtg-collection/)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE-MIT)

</div>

> **Status: pre-release.** There are no release builds yet. The public alpha is planned for **phase 7 (Online)**. Today the simulation is deterministic, packages load, and the client plays a showcase on native and web; the wider client, ranked mode, online play and the 20 ports are in progress.

IWBTG Collection plays I Wanna Be The Guy fangames the way osu! plays beatmaps. Every supported game is rebuilt in our own readable package format from clean-room specs and recorded traces. The client never runs an original game or its engine.

## Project status

| Area | Status | Detail |
|---|---|---|
| Simulation core | **Working** | Fixed-tick, deterministic, hash-pinned on Windows, Linux and WASM. |
| Package format | **Working** | Format v0 reader, validator and writer, including `.iwbz` containers. |
| Client | **Showcase** | Plays the demo package natively and in the browser; game select and ranked screens are in progress. |
| Ranked mode | **In progress** | Phase 6: timer, replays, local boards, HUD, practice and results. |
| Online | **Planned** | Phase 7: accounts, verified submissions, leaderboards and profiles. |
| 20 ports | **In progress** | All 20 have port issues; measured coverage is on the [dashboard](https://nugamus.github.io/iwbtg-collection/). |
| Release builds | **None yet** | The public alpha is planned for phase 7 (Online). |

The [progress dashboard](https://nugamus.github.io/iwbtg-collection/) shows phase progress, issue activity, each port's checklist and recorded traces. Object-level coverage says *not measured yet* until the census and trace exporters exist.

## What you get

- **A deterministic simulation.** `crates/sim` runs a fixed tick with no Bevy, IO, clocks or threads. The same package and inputs give bit-identical state on Windows, Linux and WASM.
- **Readable packages.** Games are TOML data and sandboxed Rhai scripts, shipped as one `.iwbz` file. Nothing is compiled code, and a package can be read and reviewed.
- **One client, native and web.** The Bevy client renders rooms, audio and input from the sim's snapshots and runs on Windows, Linux and wasm32.
- **Fidelity you can check.** Behavior comes from specs and traces of the original, compared tick by tick; a port is proven by traces and a verified full clear.
- **A ranked-ready foundation.** Tick-based timing, replays and local boards are being built now; accounts and leaderboards follow.
- **An honest progress page.** [The dashboard](https://nugamus.github.io/iwbtg-collection/) shows per-game and overall coverage, or says *not measured yet* and links the issue that will measure it.

## Play the showcase

You need a Rust toolchain (pinned by `rust-toolchain.toml`) and the platform packages in [docs/dev-setup.md](docs/dev-setup.md).

```sh
git clone https://github.com/nugamus/iwbtg-collection
cd iwbtg-collection
cargo run -p iwbtg-client -- crates/client/showcase.pkg --mute
```

`--mute` is there so a test run does not play music. For the browser build, the test suite and the full toolchain list, see [docs/dev-setup.md](docs/dev-setup.md). The whole local gate is:

```sh
cargo xtask check
```

## Project layout

| Path | What lives there |
|---|---|
| `crates/sim` | The deterministic simulation: world, kid, collision, scripts, replays, state hash. |
| `crates/format` | Reads, validates, hashes and writes game packages. |
| `crates/removal` | The signed removal-list format shared by the server and the CLI. |
| `crates/client` | The Bevy app: shell, rendering, input, audio, HUD, settings. |
| `crates/server` | The leaderboard backend: submissions, replay verification, review queue. |
| `crates/tools` | The `iwbtg` command-line tool. |
| `crates/xtask` | Repository automation: `check`, `plan`, `repin`, `regen`, `pr-body`. |
| `docs/` | Format spec, architecture, decisions, per-game pages and the roadmap. |
| `progress/` | The generated porting-progress dashboard and its data. |
| `scripts/` | CI scope, build guards and development scripts. |

Ported games live in the private `iwbtg-collection-games` repo. Clean-room specs, traces and original game material stay in the private `iwbtg-collection-research` repo and never enter this one.

## How the work is organised

The roadmap is a set of phase gates, named `N · Domain` and tracked as milestones in this repo:

`0 · Foundation` · `1 · Behavior research` · `2 · Format & pipeline` · `3 · Sim fidelity` · `4 · First playable` · `5 · Client shell & UI` · `6 · Ranked-ready run` · `7 · Online` · `8 · Ghosts & races` · `9 · Content & ports` · `10 · Release`

Each phase has a scope, a lane, an entry condition and a checkable exit condition. The full dispatch table is in [docs/roadmap.md](docs/roadmap.md); the live counts and size points are on the [progress page](https://nugamus.github.io/iwbtg-collection/). Research and games issues carry a one-line `Phase:` pointer instead of a milestone.

## Rules that shape the code

- **Clean room.** Behavior comes only from the specs and traces in the research repo. Decompiled code, original binaries and extracted assets are never committed here.
- **Feel over implementation.** A port reproduces what a player can feel — position, death, timing, RNG on a played path, input response — not the original's internal structure.
- **Standard by default.** New games use the standard ruleset; a port selects a named ruleset and `[compat]` profile. One-game behavior goes to data, a script or a profile, never a shared-code quirk.
- **Packages are data.** Input uses client actions, never raw keys; scripts run sandboxed.

## Contributing

Issues and pull requests are welcome. [CONTRIBUTING.md](.github/CONTRIBUTING.md) covers the build and test commands, the PR flow, the licenses, the clean-room and findings rules and what a PR must contain. If you want to port a game, read the [porting guide](docs/porting-guide.md). Report security problems privately, never in a public issue; see [SECURITY.md](.github/SECURITY.md).

## Credits and license

I Wanna Be The Guy was made by Kayin. This is an unaffiliated fan project and is not endorsed by Kayin or by the creators of any fangame. Game content — characters, sprites, music and level design — belongs to its creators and is not covered by this repository's licenses.

Code is licensed under the [MIT License](LICENSE-MIT). Documentation in `docs/` is licensed under [CC BY 4.0](LICENSES/CC-BY-4.0.txt). Bundled fonts are under the SIL Open Font License 1.1; their licence files sit next to them. The Kid sprites in `crates/client/showcase.pkg` come from a fangame and are credited in its `CREDITS.md`.

Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in the work is licensed under the MIT License, without any additional terms or conditions.
