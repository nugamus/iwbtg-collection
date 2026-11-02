# Dev setup

How a dev machine is laid out, which tools it needs, and how to check it works. Recorded on the home Windows PC on 2026-11-02 (issue #192).

## Folder layout

All three repos sit side by side in one workspace folder. CLAUDE.md files refer to each other with these relative paths (`../research`, `../games`), so keep the folder names exactly as below.

```
IWBTG Collection/            workspace root, not a repo
  CLAUDE.md                  points to the three repos
  .claude/settings.json      plugins, MCP denylist, deny rules, archive guard, issue hooks (in no repo; copy of client/workspace/settings.json)
  client/                    nugamus/iwbtg-collection: Bevy client, format spec, server, docs
  games/                     nugamus/iwbtg-collection-games (private): ported packages, games/<game_id>/
  research/                  nugamus/iwbtg-collection-research (private)
    archive/  extracted/     original downloads and extraction output: gitignored, never committed or opened
    db/catalog.sqlite        catalog database: gitignored
    tools/                   gitignored research tools: gm8decompiler, gmksplitter, utmt-cli, opengmk
    build/packages/          converter output (format v0 research builds): gitignored
```

Start Claude Code sessions from the workspace root. Its `.claude/settings.json` loads the archive guard (`research/.claude/hooks/guard_archive.py`) and the issue hooks (`client/.claude/hooks/issues.py`, the same file is in all three repos). Hooks load when a session starts, so restart Claude Code after changing them.

The root settings file is in no repo. Its tracked copy is `client/workspace/settings.json`. On a new machine, copy it to `.claude/settings.json` in the workspace root. To change the settings, change the tracked copy through a PR, then copy it over. The session-start hook warns when the two differ.

## Why `games/` has no packages yet

That's expected. `iwc convert` (research) writes its packages to `research/build/packages/`, which is gitignored. They are research builds from extracted originals. A package only moves into `games/<game_id>/` when its port issue runs (games repo, milestones M3 and M5). On 2026-11-02, `research/build/packages/` held 20 packages.

## Toolchain

| Tool | Version on the home PC | Notes |
|---|---|---|
| rustup | 1.29.2 | toolchain pinned by `client/rust-toolchain.toml` |
| rustc / cargo | 1.99.1 / 1.99.1 | targets installed: x86_64-pc-windows-msvc, wasm32-unknown-unknown. `wasm32-wasip1` is in rust-toolchain.toml; rustup adds it on first use |
| Python | 3.12.9 | research venv at `research/.venv` |
| uv | 0.12.18 | |
| gh | 2.102.0 | logged in as nugamus |
| git | 2.52.0.windows.1 | |
| Claude Code | 2.1.316 | |
| OpenGMK tracer | `research/tools/opengmk/bin/gm8emulator.exe`, sha256 `d83371ba68900fa3…` | built from OpenGMK 9e4282e6 + `iwc-trace.patch`. The source checkout is also on disk in `tools/opengmk/src/`, see research#28 |
| REA | MCP server `rea`, available in sessions | `rea-linux` is blocked in settings |

## Check a machine

From the workspace root:

```
cd client
cargo build --workspace
python -I .claude/hooks/issues.py start
python -I ../research/.claude/hooks/guard_archive.py --test
```

Expected: the build ends with `Finished`. `issues.py start` prints JSON with milestone progress and the next issues. The guard prints `guard_archive self-test passed`.

## CI

`.github/workflows/ci.yml` runs on every PR and every push to main:

- Windows and Linux: `cargo build`, `cargo clippy --workspace --tests -- -D warnings`, `cargo test --workspace` (all `--locked`, warnings are errors), and `cargo fmt --check` on Linux.
- wasm32: `cargo test -p iwbtg-sim -p iwbtg-format --target wasm32-wasip1` under wasmtime, and a wasm32-unknown-unknown build. wasm32-unknown-unknown tests under node: #206.
- `cargo deny check licenses`.

The same checks locally, from `client/`:

```
cargo clippy --workspace --tests -- -D warnings
cargo test --workspace
cargo fmt --all -- --check
```

Without wasmtime, a wasm32-wasip1 test binary also runs under Node's WASI. Build it with `cargo test -p iwbtg-sim --target wasm32-wasip1 --no-run`, then run the printed `.wasm` with a small script using `node:wasi` (preview1).

## Home PC findings (2026-11-02)

- All three repos were behind GitHub; each now fast-forwards cleanly to `main`.
- research had uncommitted trace tool work. It is saved as `backup/home-pc-2026-11-02-local-trace-work`; research#27 lands it.
- `client/.claude/worktrees/` held six workflow worktrees with unpushed WP1, WP2, WP3, WP4a, WP7a and WP8a work. Each is saved as `backup/home-pc-2026-11-02-<wp>`, and its issue (#1, #2, #3, #4, #8, #10) points to it. #203 removes the worktrees.
- The root `.claude/settings.json` lacked the issue hooks, and they are now added. #202 added the tracked copy and the drift warning.
- `research/tools/opengmk/src/` contradicts research/CLAUDE.md: research#28.
- No originals, extraction output or database are tracked in any repo. `git status --ignored` shows them ignored in research.
