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
| OpenGMK tracer | `research/tools/opengmk/bin/gm8emulator.exe`, sha256 `d83371ba68900fa3…` | built from OpenGMK 9e4282e6 + `iwc-trace.patch`. The source checkout is also on disk in `tools/opengmk/src/`, only to rebuild the tracer: no agent opens, reads or greps it except to apply `iwc-trace.patch` and build, and never while writing specs or client code (research#28) |
| REA | MCP server `rea`, available in sessions | `rea-linux` is blocked in settings |

## Agent teams

Several agents can work at once through Claude Code agent teams (experimental). The workspace settings turn the feature on with `"env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" }`. One session is the lead, and it starts teammates. Each teammate is a full Claude Code session with its own context. Teammates talk to the lead and each other with messages. The docs describe a shared task list, but none of the sessions has a tool to create or read one (checked 2026-11-02: the lead had only Agent, SendMessage and TaskStop, a teammate has no task tool either, and the session's `~/.claude/tasks/` folder stayed empty), so the `in-progress` labels are the task list (see Claims below). On Windows they run in-process: the lead's agent panel lists them, arrow keys and Enter open one, and `x` stops it. Split panes need tmux or iTerm2, which Windows Terminal and the VS Code terminal don't support.

Teammates share the lead's working folder, so the CLAUDE.md files require:

- **One worktree per lane:** `IWBTG Collection/worktrees/<repo>-<lane>` (`client-ui`, `client-data`, `client-sim`), created once with `git worktree add ../worktrees/<repo>-<lane> -b issue-<n>-<name> origin/main` from inside the repo. Each next issue is a new branch in the same worktree (`git switch -c issue-<n>-<name> origin/main`), so its `target/` stays warm. The main checkouts stay on `main`. `worktrees/` is in no repo.
- **Claims:** the `in-progress` label, in all three repos. The session-start hook lists claimed issues separately and leaves them out of the next-issues list.
- **Merging:** teammates open PRs and send the link to the lead. The lead merges one at a time. Lane worktrees are kept between issues.
- **Research data work** (crawls, downloads, extraction, traces, the converter) needs `archive/`, `extracted/`, `db/` and `tools/`. Those exist only in the main research checkout, so that work runs there, one agent at a time.

Each client worktree builds its own `target/` folder: the first build there takes about 5 minutes alone and over 10 when three teammates start at once. That is why worktrees are per lane and never removed between issues (see Build speed). Teams use several times the tokens of one session. The docs advise 3 to 5 teammates, Sonnet for teammates, and checking in on the team often. `/resume` does not bring back in-process teammates.

**Which hooks fire for teammates** (checked 2026-11-02 in the first team run, three in-process Sonnet teammates):

- `PreToolUse` guard (`guard_archive.py`): **fires.** A teammate's `cd archive` was blocked with the guard's message. (The check ran in a worktree that has no `archive/`, so nothing could have happened either way.)
- `SessionStart` (`issues.py start`): **does not fire.** None of the three teammates got the project status at startup.
- `Stop` (`issues.py stop`): **not seen.** No teammate was held to a Findings line. Treat it as not firing.

So a teammate runs the status hook by hand at the start (from the workspace root: `python -I client/.claude/hooks/issues.py start`; the output is JSON, read `additionalContext`) and ends every turn that changed files with `Findings: <issue links>` or `Findings: none`, whether or not a hook asks. The lead's own session still gets both hooks.

The lead prompt is `TEAM_LEAD_PROMPT.md` in the workspace root.

**Background commands don't wake a teammate** (checked 2026-11-05 in the transcripts of the 2026-11-04 run): when a teammate ends its turn "waiting for" a background cargo run, the completion notice is queued and only delivered when the lead next messages it. Teammates sat idle 30 to 60 minutes per wait: the data and sim teammates were idle 2.3 of their 3.3 and 3.1 hours. Teammates run cargo in the foreground; the workspace settings raise the shell tool limit to 45 minutes (`BASH_MAX_TIMEOUT_MS`) and the default to 15 (`BASH_DEFAULT_TIMEOUT_MS`).

## Build speed

Measured on the home PC (Ryzen 7 7800X3D, 64 GB, NVMe) on 2026-11-05:

| Step | Before | With this setup |
|---|---|---|
| Cold build of `iwbtg-client`, one build alone | 4 min 50 s | 3 min 45 s |
| Cold build, three worktrees at once (2026-11-04 run) | 10 min or more, often hit the old 10-minute tool limit | |
| Then every test binary of the workspace | | 3 min 20 s more |
| Touch `crates/client/src/main.rs`, rebuild the client | 13 s | 4 s |
| Touch `crates/sim/src/lib.rs`, rebuild every test binary | 33 s (82 binaries) | 7 s (12 binaries, #517) |
| Run `cargo test --workspace` once built | 20 s (7 s of tests, the rest starting 82 executables) | 10 s |
| `target/` of one worktree | 8 to 21 GB | 6 GB |
| CI, whole matrix | 2 to 3 min | |

What follows from it:

- `.cargo/config.toml` links with `rust-lld` on Windows, and `Cargo.toml` keeps only line tables as debug info for our crates and none for dependencies: faster links and a smaller `target/`.
- `.claude/hooks/build_guard.py` (PreToolUse, all sessions and teammates) blocks cargo commands that change the fingerprint or run in the background; `# build-guard: ok (<why>)` in the command overrides it. Self-test: `python -I client/.claude/hooks/build_guard.py --test`.
- Anything that changes the build fingerprint (`RUSTFLAGS`, `--config`, `CARGO_PROFILE_*`, a different linker) rebuilds all dependencies, about 5 minutes, and again on the way back. Don't copy CI's `RUSTFLAGS`/`CARGO_INCREMENTAL` into local commands.
- Keep worktrees per lane and warm (above). A cold worktree costs 5 to 10 minutes and 10 to 20 GB.
- Locally, run the tests of what you changed; run the full workspace once before the PR and let CI do the rest.
- `python -I client/.claude/stats.py [--days N]` from the workspace root reads the Claude Code transcripts and prints, per session and teammate, wall time, turns, average context, time inside cargo, idle time (waiting for a message), slow cargo calls, merges of `main`, conflicts and PRs.

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
- `cargo deny check licenses`, and the sim's dependency ban: `cargo deny --manifest-path crates/sim/Cargo.toml --config crates/sim/deny.toml check bans` (docs/architecture/sim.md, Sim purity).

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
- `research/tools/opengmk/src/` is kept only to rebuild the tracer; research/CLAUDE.md now says so (research#28).
- No originals, extraction output or database are tracked in any repo. `git status --ignored` shows them ignored in research.
