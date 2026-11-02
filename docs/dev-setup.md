# Dev setup

How a dev machine is laid out, which tools it needs, and how to check it works. Recorded on the home Windows PC on 2026-11-02 (issue #192).

## Folder layout

All three repos sit side by side in one workspace folder. CLAUDE.md files refer to each other with these relative paths (`../research`, `../games`), so keep the folder names exactly as below.

```
IWBTG Collection/            workspace root, not a repo
  CLAUDE.md                  points to the three repos
  TEAM_LEAD_PROMPT.md        prompt that starts an agent-team lead (in no repo)
  worktrees/                 one git worktree per working agent (in no repo)
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

## Agent teams

Several agents can work at once through Claude Code agent teams (experimental). The workspace settings turn the feature on with `"env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" }`. One session is the lead, and it starts teammates. Each teammate is a full Claude Code session with its own context. Teammates talk to the lead and each other with messages and share a task list. On Windows they run in-process: the lead's agent panel lists them, arrow keys and Enter open one, and `x` stops it. Split panes need tmux or iTerm2, which Windows Terminal and the VS Code terminal don't support.

Teammates share the lead's working folder, so the CLAUDE.md files require:

- **One worktree per agent:** `IWBTG Collection/worktrees/<repo>-<issue>`, created with `git worktree add ../worktrees/<repo>-<n> -b issue-<n>-<name> origin/main` from inside the repo. The main checkouts stay on `main`. `worktrees/` is in no repo.
- **Claims:** the `in-progress` label, in all three repos. The session-start hook lists claimed issues separately and leaves them out of the next-issues list.
- **Merging:** teammates open PRs and send the link to the lead. The lead merges one at a time and removes the worktree.
- **Research data work** (crawls, downloads, extraction, traces, the converter) needs `archive/`, `extracted/`, `db/` and `tools/`. Those exist only in the main research checkout, so that work runs there, one agent at a time.

Each client worktree builds its own `target/` folder: the first build there takes a few minutes and a few GB. Teams use several times the tokens of one session. The docs advise 3 to 5 teammates, Sonnet for teammates, and checking in on the team often. `/resume` does not bring back in-process teammates.

The lead prompt is `TEAM_LEAD_PROMPT.md` in the workspace root.

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

Changes that touch only docs, Markdown, `workspace/` or `.claude/` skip it. Builds run without debug info or incremental state, to save time.

- Windows and Linux: `cargo clippy --workspace --tests -- -D warnings`, `cargo test --workspace` (all `--locked`, warnings are errors), and `cargo fmt --check` on Linux.
- wasm32: `cargo test -p iwbtg-sim` and `cargo test -p iwbtg-format --lib --test fixtures` on wasm32-wasip1 under wasmtime, and a wasm32-unknown-unknown build. The package-loading `wp7a_*` tests run on native targets only, because wasm32-wasip1 aborts on the panics of unmerged WP0 stubs. On wasm32-unknown-unknown, `wp1_determinism` runs under node through `wasm-bindgen-test` (`.cargo/config.toml` sets the runner): `cargo test -p iwbtg-sim --test wp1_determinism --target wasm32-unknown-unknown`. The runner is the wasm-bindgen CLI, and its version must equal `wasm-bindgen` in Cargo.lock: `cargo install wasm-bindgen-cli --version <lock version> --locked`. Other sim tests still need the same `#[wasm_bindgen_test]` switch to run there.
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
