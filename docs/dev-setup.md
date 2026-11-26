# Dev setup

How a dev machine is laid out, which tools it needs, and how to check it works. Recorded on the home Windows PC on 2026-11-02 (issue #192).

## Folder layout

All three repos sit side by side in one workspace folder. Runnable commands and CLAUDE.md use these checkout names. Some prose and architecture citations use the role names `client/`, `research/` and `games/`; they refer to the checkouts below. If your checkouts use the short names, substitute them consistently in commands and relative paths.

```
IWBTG Collection/            workspace root, not a repo
  CLAUDE.md                  points to the three repos
  TEAM_LEAD_PROMPT.md        prompt that starts an agent-team lead (in no repo)
  worktrees/                 one git worktree per working agent (in no repo)
  .claude/settings.json      plugins, MCP denylist, deny rules, archive guard, issue hooks (in no repo; copy of iwbtg-collection/workspace/settings.json)
  iwbtg-collection/          nugamus/iwbtg-collection: Bevy client, format spec, server, docs
  iwbtg-collection-games/    nugamus/iwbtg-collection-games (private): ported packages, games/<game_id>/
  iwbtg-collection-research/ nugamus/iwbtg-collection-research (private)
    archive/  extracted/     original downloads and extraction output: gitignored, never committed or opened
    db/catalog.sqlite        catalog database: gitignored
    tools/                   gitignored research tools: gm8decompiler, gmksplitter, utmt-cli, opengmk
    build/packages/          converter output (format v0 research builds): gitignored
```

Start Claude Code sessions from the workspace root. Its `.claude/settings.json` loads the archive guard (`iwbtg-collection-research/.claude/hooks/guard_archive.py`) and the issue hooks (`iwbtg-collection/.claude/hooks/issues.py`, the same file is in all three repos). Hooks load when a session starts, so restart Claude Code after changing them.

The root settings file is in no repo. Its tracked copy is `iwbtg-collection/workspace/settings.json`. On a new machine, copy it to `.claude/settings.json` in the workspace root. To change the settings, change the tracked copy through a PR, then copy it over. The session-start hook warns when the two differ.

## Why `iwbtg-collection-games/` has no packages yet

That's expected. `iwc convert` (research) writes its packages to `iwbtg-collection-research/build/packages/`, which is gitignored. They are research builds from extracted originals. A package only moves into `iwbtg-collection-games/<game_id>/` when its port issue runs (games repo, phases 4 (First playable) and 9 (Content & ports)). On 2026-11-02, `iwbtg-collection-research/build/packages/` held 20 packages.

## Toolchain

| Tool | Version on the home PC | Notes |
|---|---|---|
| rustup | 1.29.2 | toolchain pinned by `iwbtg-collection/rust-toolchain.toml` |
| rustc / cargo | 1.99.1 / 1.99.1 | targets installed: x86_64-pc-windows-msvc, wasm32-unknown-unknown. `wasm32-wasip1` is in rust-toolchain.toml; rustup adds it on first use |
| Python | 3.12.9 | research venv at `iwbtg-collection-research/.venv` |
| uv | 0.12.18 | |
| gh | 2.102.0 | logged in as nugamus |
| git | 2.52.0.windows.1 | |
| Claude Code | 2.1.316 | |
| OpenGMK tracer | `../iwbtg-collection-research/tools/opengmk/bin/gm8emulator.exe`, sha256 `d83371ba68900fa3…` | built from OpenGMK 9e4282e6 + `iwc-trace.patch`. The source checkout is also on disk in `tools/opengmk/src/`, only to rebuild the tracer: no agent opens, reads or greps it except to apply `iwc-trace.patch` and build, and never while writing specs or client code (research#28) |
| REA | MCP server `rea`, available in sessions | `rea-linux` is blocked in settings |
| cargo-about | 0.9.3 | generates `THIRD_PARTY_LICENSES`: `cargo install cargo-about --locked --features cli` |

## Licences

`REUSE.toml` gives most files their licence and copyright by path. A file outside its tables carries an in-file `SPDX-FileCopyrightText`/`SPDX-License-Identifier` header (in the format's comment syntax), and a binary gets a `<file>.license` sidecar with the same two tags. Check it with `uvx --from "reuse[charset-normalizer]" reuse lint` (REUSE 6.3.0; the `charset-normalizer` extra is needed on Windows), expected `Congratulations! Your project is compliant with version 3.3 of the REUSE Specification :-)`. A file without licence information, or a licence with no text in `LICENSES/`, breaks it. CI's `licenses` job runs the same `reuse lint` and regenerates `THIRD_PARTY_LICENSES`, failing on a diff (#976).

`THIRD_PARTY_LICENSES` lists the Rust crates the Windows and Linux builds include, generated from `Cargo.lock` by cargo-about (`about.toml` mirrors the licences `deny.toml` allows; workspace crates are left out). Regenerate it after a `Cargo.lock` change and for each release (#71):

```
cargo about generate --locked --output-file THIRD_PARTY_LICENSES about.hbs
```

## Agent teams

Several agents can work at once through Claude Code agent teams (experimental). The workspace settings turn the feature on with `"env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" }`. One session is the lead, and it starts teammates. Each teammate is a full Claude Code session with its own context. Teammates talk to the lead and each other with messages. The docs describe a shared task list, but none of the sessions has a tool to create or read one (checked 2026-11-02: the lead had only Agent, SendMessage and TaskStop, a teammate has no task tool either, and the session's `~/.claude/tasks/` folder stayed empty), so the `in-progress` labels are the task list (see Claims below). On Windows they run in-process: the lead's agent panel lists them, arrow keys and Enter open one, and `x` stops it. Split panes need tmux or iTerm2, which Windows Terminal and the VS Code terminal don't support.

Teammates share the lead's working folder, so the CLAUDE.md files require:

- **One worktree per lane:** `IWBTG Collection/worktrees/<repo>-<lane>` (`client-ui`, `client-data`, `client-sim`), created once with `git worktree add ../worktrees/<repo>-<lane> -b issue-<n>-<name> origin/main` from inside the repo. Each next issue is a new branch in the same worktree (`git switch -c issue-<n>-<name> origin/main`), so its `target/` stays warm. The main checkouts stay on `main`. `worktrees/` is in no repo.
- **Claims:** the `in-progress` label, in all three repos. The session-start hook lists claimed issues separately and leaves them out of the next-issues list.
- **Merging:** the PR owner reviews the diff, runs the User check and requests `gh pr merge <N> --squash --auto`; routine changes need no separate lead handoff. Leads handle cross-lane contract questions and integration after merge waves. Keep lane worktrees and never use `--delete-branch`. See [agent workflow](agent-workflow.md) and CI below.
- **Doc pages per module** (#562): on 2026-11-10 five of seven open client PRs conflicted with `main` only in docs, `docs/architecture/client.md` most (21 commits touched it from 2026-11-01 to 2026-11-11). Every client PR edited that one page, so each merge made the waiting PRs conflict again. The client's notes are now one page per module in `docs/architecture/client/` (`client.md` is the index), so PRs on different modules never touch the same file. A PR adds its note to its module's page; a new module adds a new page and one line, in alphabetical order, in the index. The sim follows the same rule (#619): a note goes on its area's page in `docs/architecture/sim/`, a register row on its area's page `docs/architecture/sim/register-<area>.md`, a converter request on `docs/format/converter-requests.md`; `sim.md` and its register section are the live indexes. `merge=union` in `.gitattributes` would not help: GitHub ignores repository merge drivers in its mergeability check and its merge buttons (GitHub Support, quoted in community discussion 9288; the feature request there is still open in 2026-10), so it would only help local merges.
- **Hash-pin conflicts** (#949): two PRs that each change hashed sim state always conflict in the hash pins (`determinism.rs` and `hash_vectors.rs` took 15 commits each from 2026-11-01 to 2026-11-18). The pins are generated, so a conflict in one is never merged by hand: take either side and mark it resolved (`git checkout --theirs -- <file>`, then `git add <file>`), then re-add any new vector or test the other side brought as a normal edit, resolve the other files, then run `cargo xtask repin` and commit what it changed. The pin files are listed once, in `crates/xtask/pin-files.txt`, which `repin` and `conflict_guard.py` both read; the guard enforces this (Build speed).
- **Scratch files:** all teammates share one scratchpad folder, and generic names (`pr.md`, `msg.txt`) collide: on 2026-11-16 one teammate's `pr.md` overwrote another's and a research PR briefly showed a client PR's description (#869). Each teammate keeps scratch files in `<scratchpad or %TEMP%>/<agent-name>/`, never at the top level and never inside a lane worktree.
- **Research data work** (crawls, downloads, extraction, traces, the converter) needs `archive/`, `extracted/`, `db/` and `tools/`. Those exist only in the main research checkout, so that work runs there, one agent at a time.

Each client worktree builds its own `target/` folder: the first build there takes about 5 minutes alone and over 10 when three teammates start at once. That is why worktrees are per lane and never removed between issues (see Build speed). Teams use several times the tokens of one session. The docs advise 3 to 5 teammates, Sonnet for teammates, and checking in on the team often. `/resume` does not bring back in-process teammates.

**Which hooks fire for teammates** (checked 2026-11-02 in the first team run, three in-process Sonnet teammates):

- `PreToolUse` guard (`guard_archive.py`): **fires.** A teammate's `cd archive` was blocked with the guard's message. (The check ran in a worktree that has no `archive/`, so nothing could have happened either way.)
- `SessionStart` (`issues.py start`): **does not fire.** None of the three teammates got the project status at startup.
- `Stop` (`issues.py stop`): **not seen.** No teammate was held to a Findings line. Treat it as not firing.

So a teammate runs the status hook by hand at the start (from the workspace root: `python -I iwbtg-collection/.claude/hooks/issues.py start`; the output is JSON, read `additionalContext`) and ends every turn that changed files with `Findings: <issue links>` or `Findings: none`, whether or not a hook asks. The lead's own session still gets both hooks.

**Archive guard timeout (#755).** The archive guard is registered with `onFailure: "block"`, so a timeout blocks the tool call instead of failing open. Measured on 2026-11-19 on a laptop (Intel Core Ultra 5 225H, 31 GB RAM, Python 3.14.7, Defender real-time protection on; not the home PC above): a bare `python -I -c pass` takes a median 25 ms, `guard_archive.py --test` 48 ms and a real hook payload 51 ms (max 60 ms) over 20 runs; four concurrent hook invocations median 46 ms, max 54 ms; twelve concurrent invocations max 210 ms; a never-seen copy of the hook, first run, 100 to 495 ms. The guard's own code is about 24 ms of that (importing `json` and `re`); interpreter startup and Defender scanning dominate. The 2026-11-13 team-run timeouts were therefore transient stalls, not steady-state cost: that day Defender updated its signatures at 08:39 and 10:58 (Defender Operational log), which can invalidate cached scan results, while several cargo builds wrote gigabytes and Defender scanned every produced executable (Build speed), so a cold `python.exe` start could take seconds. The timeout is now 30 s in the tracked copy and in `iwbtg-collection-research/.claude/settings.json`: still bounded, still blocking on failure, and over 60x the worst measured start. The workspace-root `.claude/settings.json` is in no repo, so copy `iwbtg-collection/workspace/settings.json` over it and restart Claude Code for the new value to load; until then `issues.py start` prints the drift warning.

**Screenshots come only from the client** (#822): on 2026-11-15 the ui teammate on #495, wanting to see the loading card live, took two OS screen captures of the user's main screen; they showed the desktop, not the game window. A teammate takes screenshots only with the client's own `--screenshot` flag (`WGPU_BACKEND=dx12` when the display is locked), never with an OS or desktop capture or a screen-capture tool. An agent cannot press keys in a live window either, so that behavior is covered by tests and by the user's check on `main`.

The lead prompt is `TEAM_LEAD_PROMPT.md` in the workspace root. It assigns lanes and spawns teammates; the tracked `CLAUDE.md` and `docs/agent-workflow.md` contract is authoritative for routine merges. If the external copy still says the lead merges routine PRs, synchronize it through #1025.

**Shutdown before the next teammate** (#677): the lead spawns a lane's next teammate only after the previous one's `shutdown_approved` arrived, and never sends a teammate new work in the same message batch as its shutdown request. A shutdown request is only read between steps, and a teammate with a pending message acts on that first: on 2026-11-12 the old sim teammate was still merging `main` in `worktrees/client-sim` when its successor ran `git switch -c` there, and the old one's `git push` ran on the wrong branch.

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

Concurrency measured on the development laptop on 2026-11-19 (Intel Core Ultra 5 225H, 14 cores, 31 GB RAM, NVMe):

| `cargo check -p …` | one alone | two at once | three at once | warm (deps built) |
|---|---|---|---|---|
| `iwbtg-client`, cold | 119.6 s | 341.8 s | 665.0 s | about 6 s at one, two or three |
| `iwbtg-sim`, cold | 26.5 s | 40.3 s | 57.1 s | about 7 s at one, two or three |

Cold client targets used about 945 MB each; sim targets about 187 MB each. Three concurrent cold client builds take 11.1 minutes, against 6.0 minutes if the same three run one after another, so heavy client builds are serialized by the machine-wide slot below. Warm checks show no contention and stay concurrent; CI runners are isolated per job, so this applies to a development host only.

**Heavy build slot (#1029).** A cold or heavy client build goes through one slot shared by every worktree of this checkout:

```
python -I scripts/heavy_build.py -- cargo build -p iwbtg-client --locked
python -I scripts/heavy_build.py --status
```

The holder records pid, host, command and worktree and refreshes the lock every 5 s in the system temp folder. Waiters print progress while editing, searches, narrow tests and warm checks continue. A holder with no heartbeat for 90 s is reclaimed, and a waiter gives up after 15 minutes (exit 3) instead of queueing forever. The lock never enters a repository.
| CI, whole matrix on the shared runners | 24 min (measured 2026-11-18, #961) | 5 to 9 min warm, 9 to 15 min cold (CI section) |

**Windows Defender** scans every executable the build writes (82 test executables per run before #517). Excluding the build output folders, and only those, speeds builds and test runs up. It needs an administrator PowerShell, so the user runs it once:

```
Add-MpPreference -ExclusionPath "C:\Users\Nugamus\Desktop\Code Projects\IWBTG Collection\client\target","C:\Users\Nugamus\Desktop\Code Projects\IWBTG Collection\worktrees\*\target"
```

Never exclude the whole workspace: `../iwbtg-collection-research/archive/` and `../iwbtg-collection-research/extracted/` hold untrusted downloads that Defender must keep scanning. A Windows Dev Drive (ReFS, Defender's performance mode) would help more, but means moving the workspace, which also moves every Claude Code transcript and memory path; not done.

Tried and not adopted (#532, 2026-11-05): Bevy's `dynamic_linking` feature cut a client rebuild after touching `view.rs` from 3.5 s to 2.4 s, but it is a second feature set, so every worktree would build Bevy twice (minutes and gigabytes) and tests and clippy would need the feature too. Splitting `iwbtg-client` into a library and a thin binary saves at most the binary's own compile (about 2 s of the 3.5 s) while the crate is about 4,000 lines. Look again when the client crate passes about 15,000 lines or a rebuild passes 15 s.

What follows from it:

- `.cargo/config.toml` links with `rust-lld` on Windows, and `Cargo.toml` keeps only line tables as debug info for our crates and none for dependencies: faster links and a smaller `target/`.
- `.claude/hooks/build_guard.py` (PreToolUse, all sessions and teammates) blocks cargo commands that change the fingerprint or run in the background; `# build-guard: ok (<why>)` in the command overrides it. Self-test: `python -I iwbtg-collection/.claude/hooks/build_guard.py --test`.
- `.claude/hooks/conflict_guard.py` (PreToolUse, all sessions and teammates, #698) denies a `git commit` whose staged changes, or a `git push` whose unpushed commits, add a line starting with git's conflict markers (`<<<<<<< ` or `>>>>>>> `; a lone `=======` is a Markdown heading underline and passes). A resolve script and its commit in one command are checked before the script runs, so the push check is the one that catches what the script missed: fix the file, commit the fix, push again. While a conflict is unresolved it also guards the hash-pin files of `crates/xtask/pin-files.txt` (#949): `git add` of a pin file that matches neither side (hand-merged, or still with markers) is denied, and `git commit` or `git merge --continue` with unmerged pin files is denied with the fix, `cargo xtask repin` after taking either side; on a mixed conflict it also lists the other files to resolve by hand. `# conflict-guard: ok (<why>)` overrides it. Self-test: `python -I iwbtg-collection/.claude/hooks/conflict_guard.py --test`.
- Anything that changes the build fingerprint (`RUSTFLAGS`, `--config`, `CARGO_PROFILE_*`, a different linker) rebuilds all dependencies, about 5 minutes, and again on the way back. Don't copy CI's `RUSTFLAGS`/`CARGO_INCREMENTAL` into local commands.
- Keep worktrees per lane and warm (above). A cold worktree costs 5 to 10 minutes and 10 to 20 GB.
- Locally, run the tests of what you changed; run `cargo xtask check` once before the PR (only the changed crates and their dependents; `--all` for everything) and let CI do the rest.
- `python -I iwbtg-collection/.claude/stats.py [--days N]` from the workspace root reads the Claude Code transcripts and prints, per session and teammate, in-window wall time, logical model calls (message fragments counted once), average context per call, cargo time as both an elapsed union and a sum across overlapping commands (the percentage uses the union), idle time (waiting for a message), slow cargo calls, unclosed tool calls, merges of `main`, conflicts, PRs and timestamp-unknown events. `--issues` lists the issues that took longest; `--summary --days 7` prints tokens and hours per merged PR per lane; `--test` runs synthetic accounting fixtures, not real transcripts. `--days` clips events and tool intervals to the window instead of counting whole sessions. The lead appends its totals once a week to `docs/velocity.md` (numbers only).

## The client in a browser

The wasm32-unknown-unknown build plays a package over HTTP ([client architecture, Web](architecture/client/web.md), #29). To run it in a browser, from `iwbtg-collection/`:

```
cargo run -p iwbtg-tools --bin iwbtg -- pkg pack crates/client/showcase.pkg -o crates/client/web/showcase.iwbz
cargo build -p iwbtg-client --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir crates/client/web/pkg target/wasm32-unknown-unknown/release/iwbtg-client.wasm
python -m http.server 8765 --bind 127.0.0.1 --directory crates/client/web
```

Then open http://127.0.0.1:8765/: the page fetches `showcase.iwbz` from the same folder and starts the game. `?package=<url>` plays another `.iwbz` (pack it into `crates/client/web/`, or give any URL whose server sends CORS headers), and `?ui-gallery` opens the UI gallery ([client architecture, UI](architecture/client/ui.md)). `iwbtg pkg pack` prints the package's gameplay hash, and the page logs the same hash after loading, so the terminal and the browser console can be compared. `wasm-bindgen` must be the version in Cargo.lock (`cargo xtask doctor --scope browser` checks it; see CI below); `crates/client/web/pkg/` and `crates/client/web/*.iwbz` are not committed. It needs WebGL2. The page's script before `init()` unlocks the audio on the first key press or click ([client architecture, Audio](architecture/client/audio.md)); keep it when changing `index.html`. Browser automation that fakes a device pixel ratio different from the screen's (Playwright's default on a 125% display) puts the pointer in the wrong place; give the context the screen's `deviceScaleFactor`.

CI builds the same bundle (`web` job), runs `wasm-opt -Oz` (pinned binaryen via `.github/actions/wasm-opt`) on `iwbtg-client_bg.wasm`, checks the compressed result against `scripts/check_web_size.py`'s budget (17 MiB), uploads it as `web-bundle` (with `showcase.iwbz`) for 7 days, and runs `scripts/web_smoke.py`: it packs `crates/client/tests/fixtures/clear.pkg`, loads `?package=clear.iwbz` in headless Chrome (SwiftShader WebGL2) and asserts the console gameplay hash equals `iwbtg pkg pack`'s. Run it locally with `python -I scripts/web_smoke.py` (pass `--chrome` to pick a browser); it needs the `pkg/` bundle above and works without `wasm-opt`. The bundle uses the workspace release profile and the normal build fingerprint; only the post-build `wasm-opt` step and the measured budget encode the size decision (#1131).

## Check a machine

From the workspace root:

```
cd iwbtg-collection
cargo build --workspace
python -I .claude/hooks/issues.py start
python -I ../iwbtg-collection-research/.claude/hooks/guard_archive.py --test
python -I .claude/hooks/conflict_guard.py --test
cargo xtask doctor --scope all
```

Expected: the build ends with `Finished`. `issues.py start` prints JSON with main's CI status (a first line `MAIN IS RED` when it failed), milestone progress and the next issues. The guards print `guard_archive self-test passed` and `conflict_guard self-test passed`.

`cargo xtask doctor --scope <native|browser|wasi|all>` reports the prerequisites for that work only, all at once: the pinned Rust/cargo/rustfmt/clippy versions, the target directory and fixtures, the client's Linux system libraries on Linux, and for browser/WASI the installed targets, `wasm-bindgen` CLI version, and `node`/`wasmtime` runner. Each missing item prints an exact repair command and the command exits 1, so a native-only task does not fail because browser tools are absent. `cargo xtask doctor --scope native` checks only native work; `--scope all` is the full machine check. `--warm` then runs `cargo fetch --locked` at the default build fingerprint to populate the registry/cache. Run the browser scope again after Cargo.lock changes.

## CI

PRs use `scripts/ci_scope.py` to select changed Cargo packages and their transitive consumers from normal, build, dev and target dependencies. Native Clippy/tests run for those packages on Windows and Linux. Client-only changes keep browser Clippy but skip unchanged simulation corpus/report jobs; server-only changes build/test the server without Bevy or WASM. Sim/format changes retain both WASM targets and cross-platform determinism. Tool changes retain determinism reports. The same script exposes a machine-readable plan with test/compile sets, gates and per-input reasons; `cargo xtask plan [--all]` prints it without building, and `cargo xtask check` consumes the same graph. Shared, unknown, manifest, lockfile, toolchain, workflow and script inputs select full coverage.

Documentation fixtures listed in `crates/xtask/test-docs.txt` select sim/format and their consumers and run the native tests that read them; the same list is used by local xtask selection. A fixture is not a code input, so on its own it does not select the WASM targets or the determinism reports: a prose, example or `docs/format/schema/` edit runs the native format/sim tests only. A fixture keeps a WASM target only when a test there reads it: the generated `docs/format/script/iws.d.rhai` is compiled into the golden corpus, which both WASM jobs run, and `docs/format/scripts.md`, `docs/architecture/sim/scripts.md` and `docs/decisions/0001-script-language.md` are read by the sim's script doc-example test, which the `wasm32-wasip1` job runs. A docs example that breaks a test therefore still fails the PR. `cargo xtask plan` prints the per-input reason and the resulting target flags; `code` stays true for a fixture so `cargo xtask check` still runs the native readers locally. CI's native job tests only `iwbtg-sim` and `iwbtg-format` for a fixture-only plan; the consumers (`iwbtg-client`, `iwbtg-tools`) are still tested when code changes and compile-checked by the local check (#1405). Ordinary architecture prose no longer rebuilds the simulation; ordinary documentation skips Rust builds. Unknown paths, policy/workflow/scripts, manifests/lockfiles/toolchain changes, missing base revisions and merge groups select the full matrix. Every push to main runs the full matrix as the integration backstop.

**Preserved gates:** `ci-ok` and `body` remain required, including successful selection/hook tests even on documentation PRs. Native formatting, package canonical-format checking when affected, sim dependency bans and license checks remain. Dependency/license jobs run on every selected code change. No determinism target or corpus case was removed. Every job retains bounded timeouts (5–60 minutes; `web` is 60 since the `wasm-opt` pass, #1131). `ci-ok` treats failed/cancelled work as failure and accepts intentional skips.

The selected jobs run:

- Windows and Linux: `cargo clippy <selected packages> --tests --locked -- -D warnings`, `cargo test <selected packages> --locked` (warnings are errors), `cargo fmt --all -- --check` first on Linux, and the canonical `pkg fmt --check` when determinism is selected.
- wasm32, two jobs that run in parallel (#961). `wasm32-wasip1`: `cargo test -p iwbtg-sim` and `cargo test -p iwbtg-format --lib --test fixtures` under wasmtime. `wasm32-unknown-unknown`: a build of the sim and the format crate, `cargo clippy -p iwbtg-client --target wasm32-unknown-unknown --locked -- -D warnings` for the client (its browser build opens the UI gallery; this also lints the web-only code such as the `localStorage` backend), then `cargo test -p iwbtg-sim --test determinism --test golden_corpus --test feel --target wasm32-unknown-unknown`, which runs under node through `wasm-bindgen-test` (`.cargo/config.toml` sets the runner). The public feel contracts are compiled in, so they link and run on every target (#1169); the `wasm32-wasip1` job already runs them through `cargo test -p iwbtg-sim`, and `cargo xtask plan` shows a separate `feel` step for the same selection. The package-loading tests (`wp7a_*`, `wp7b_loader`, `load_*`) run on native targets only, because wasm32-wasip1 aborts on panic. The runner is the wasm-bindgen CLI, and its version must equal `wasm-bindgen` in Cargo.lock; CI downloads the release tarball for that version (`wasm-bindgen-<version>-x86_64-unknown-linux-musl.tar.gz`, checksum from its `.sha256sum` asset) instead of running `cargo install wasm-bindgen-cli --locked`, which compiled it for 1m38s on a cold cache. Other sim tests still need the same `#[wasm_bindgen_test]` switch to run there.
- `web` (#1078, #1131): builds the browser page (`cargo build -p iwbtg-client --release --target wasm32-unknown-unknown` plus the pinned `wasm-bindgen`), runs the pinned `wasm-opt -Oz` on `iwbtg-client_bg.wasm`, fails when the compressed result is over the budget in `scripts/check_web_size.py` (17 MiB today), packs `showcase.iwbz`, uploads `index.html`, `pkg/` and `showcase.iwbz` as the playable `web-bundle` artifact for 7 days, and runs `scripts/web_smoke.py` — a fixture `.iwbz` packed by the native tools, loaded in headless Chrome (SwiftShader WebGL2), whose logged gameplay hash must equal `iwbtg pkg pack`'s. It runs only when the client, including `crates/client/web/`, changed; sim/format-only changes skip it, since the wasm job already lints the client. Download the artifact, serve its folder (`python -m http.server --directory <folder>`) and open the printed URL to play the showcase.
- `cargo deny check licenses`, and the sim's dependency ban: `cargo deny --manifest-path crates/sim/Cargo.toml --config crates/sim/deny.toml check bans` (docs/architecture/sim/layout.md, Sim purity). Both run through the local action `.github/actions/cargo-deny` (pinned cargo-deny 0.21.0 with SHA-256 check, `curl --fail --retry`), not a Docker action (#893).

**Release builds (#71).** `.github/workflows/release.yml` builds `iwbtg-client --release` on `windows-latest` and `ubuntu-latest` on a `v*` tag, stages the binary with `crates/client/showcase.pkg`, `THIRD_PARTY_LICENSES`, `NOTICE`, `LICENSE-MIT` and `LICENSES/`, zips it, and uploads the zip as an artifact. On a tag it also attaches both zips to that tag's GitHub release. `workflow_dispatch` runs the same builds and artifacts without creating a release, which is how the workflow is validated without publishing. The bundle ships the one package in this repository; the five starter games (#51) and the full all-ported-games bundle are not included yet. No installer is built.

**Nightly builds (#195).** `.github/workflows/nightly.yml` runs daily (04:23 UTC) and on `workflow_dispatch`: it builds the same Windows/Linux client bundle as `release.yml` from `main` and uploads the zips as artifacts. Publishing is opt-in: the `publish` job creates or refreshes the `nightly-<UTC date>` **prerelease** (with GitHub's generated notes from merged PRs) only when the repository variable `NIGHTLY_PUBLISH` is exactly `true`. While it is unset a run produces artifacts and no release. Deploying the web build is still open on #195.

**Formatting first (#1031).** The Linux leg runs `cargo fmt --all -- --check` immediately after `rustup show`, before the Rust cache, system packages, Clippy and tests. Formatting needs none of those, so a format-only failure reports in seconds instead of after the expensive build. The step stays in the existing job, so it adds no runner, no second toolchain download and no queue delay. Locally a deliberate formatting error was reported in 1.4 s with exit 1, before any Clippy or test command ran. Hosted time-to-first-actionable-failure and resource savings are measured on #1025.

**Job timeouts (#967, #1041).** Every job sets `timeout-minutes`: 25 for the two wasm jobs, 60 for `web` (the cold release build plus `wasm-opt -Oz`, #1131), 45 for the Windows/Linux matrix, 15 for the determinism reports, 5 for `changes`, `determinism-compare`, `sim-deps`, `licenses` and `ci-ok`. Without them a wedged step ran for GitHub's 6 hour default while `ci-ok` waited and auto-merge stayed off for every PR, which is the worst place to be: a job that hangs is usually a hung test under wasmtime or node (the golden corpus, #966) or an `actions/*` step waiting on the network. The native cap was raised from 30 after a full-coverage Windows run measured 28m36s on a cold-ish cache (PR #1036, run 44531896479), only 84 s of margin; main also runs the Rust-cache save post-step there. The warm full main run took 5m32s on Windows, so the larger cap costs nothing on the common path. The two wasm caps stay 25 (slowest cold measurement 11m33s, #961); the `web` cap is 60 because a cold release wasm build and `wasm-opt -Oz` run back to back. A timed-out job ends `cancelled`, which `ci-ok`'s loop already counts as a failure, so the gate goes red and reports the job instead of sitting silent; the fix is to read that job's log, and if the work really needs longer, raise its `timeout-minutes` in the same PR that needs it.

**Golden test reuse:** within the `golden_corpus` test binary, hash and coverage assertions inspect one cached report. The run-to-run test executes a second fresh report and compares the two. This reduces four complete corpus executions to two without caching both sides of the determinism assertion. The CLI report remains uncached. Profiles, arithmetic checks, pins and cases are unchanged.

**Caches:** existing rust-cache restore/save policy is retained (PRs restore; main writes). After a successful main CI run, `.github/workflows/cache-maintenance.yml` prunes superseded Rust cache generations toward 8 GiB, protecting the newest cache for each ref/environment/key family. It never deletes unrelated caches or current families. It can be dispatched manually; `python3 scripts/prune_ci_cache.py --repo nugamus/iwbtg-collection` previews deletions without applying them. If protected entries alone exceed the target it reports that rather than evicting them. This addresses #965's near-full cache budget; it cannot guarantee a provider cache hit. The maintenance job is outside the PR's required-check path.

**Agent checks:** selector/cache tests and the three hook self-tests run in the lightweight changes job, including on harness-only PRs. `python -I scripts/test_spec_claims.py` checks that every claim, register and converter-request id cited in client `docs/**` and `crates/**/*.rs` resolves — claim ids against the committed `docs/architecture/sim/claim-ids.txt` (ids only, no spec text), register ids against the `register-*.md` rows, and `CR<n>` against `docs/format/converter-requests.md`; after a research spec split or id change, regenerate the claim list with `IWBTG_RESEARCH=../iwbtg-collection-research python3 -I scripts/test_spec_claims.py --write`. Issue creation and the findings Stop rule remain mandatory. The session dashboard reuses successful snapshots for 120 seconds; `python -I .claude/hooks/issues.py start --refresh` bypasses it. Issue claims and merge decisions still require current issue/PR reads.

**Auto-merge (#528).** A ruleset on `main` requires the `ci-ok` and `body` checks; the repository admin can bypass it. The PR owner records its User check, reviews the diff and requests `gh pr merge <n> --squash --auto`; ordinary work no longer waits for a separate lead handoff. GitHub merges when both checks are green, so nobody polls CI. Branches don't have to be up to date with `main` (that would stall auto-merge, which doesn't update branches), so the push run on `main` is the check after each merge; a PR that changes pinned hashes is updated with `gh pr update-branch <n>` before its auto-merge. Cross-area/contract changes still need domain review. Never use `--delete-branch` on a lane worktree. GitHub's merge queue would update and test each PR against `main` itself, but it is not offered for repositories owned by a personal account (the ruleset API refused it, 2026-11-05). The workflows keep their `merge_group` triggers in case the repo moves to an organization.

**Cancellation (#1027).** PR and merge-group runs keep `cancel-in-progress: true`, so a superseded push is stopped; their newest head reports. A push to `main` does not cancel the active run. It queues behind it, and GitHub keeps at most one pending run per concurrency group, so a burst of merges still lets one trusted main generation finish and save its caches while the newest head waits. The newest head still gets its own run; a completed superseded run never stands in for it. Main is therefore bounded at one running plus one pending run, and `ci-ok` still fails on a genuine failure or timeout. The cache-maintenance workflow runs after each successful main run and skips cancelled ones.

**Stuck runs (#666).** Every PR run started on 2026-11-11 after about 20:20 UTC stayed `queued` with no jobs for 16 hours while GitHub status showed Actions operational, and auto-merge waited, because nobody polls CI. `gh run cancel`, `gh run rerun` and force-cancel all refuse such a run (409 "not queued yet" / "already running"), and `gh pr update-branch` does nothing when the branch is up to date. What works: `gh pr close <n> && gh pr reopen <n>`, which starts fresh runs; reopening turns auto-merge off, so turn it on again with `gh pr merge <n> --squash --auto`. The status hook marks such a run `CI STUCK`; inspect it first, then restart. Do not poll CI in a loop while waiting.

**Where the time goes (#961, measured 2026-11-18/19).** A code PR takes about 5 to 9 minutes with a warm cache and 9 to 15 minutes when the caches are cold (a new `Cargo.lock` or toolchain, a runner image update that changes rust-cache's environment hash, or GitHub evicting the cache, #965), not the "about 3 minutes" earlier notes claimed. The two runs measured on that branch with cold caches were 9m32s and 15m20s; the second was gated by the native Linux job rebuilding from scratch (15m00s), not by wasm. Almost all of the wasm time is one suite: `tests/golden_corpus.rs` (6 tests) runs the whole corpus and takes 185 to 478 s per target, because it is compiled unoptimized (the `test` profile) and wasm execution is slow: about 4 to 8 minutes under wasmtime and 3 to 5 minutes under node. Compiling the sim and the client for the two targets is 10 to 20 s once the cache is warm and 1 to 5 minutes cold. That suite runs on both wasm targets on purpose (docs/plans/sim-core.md 10.6), so the two wasm jobs run in parallel and the wall time is the slower of the two, not their sum. Shortening it further means building the wasm test profile optimized or running the corpus less often per PR; both change what CI compiles or checks, so they are a decision of their own, not a CI tweak (#966).

**Measurements (2026-11-19).** A recent full-matrix run (44504103250) took 8m46s end to end; browser WASM was 509s, including 246s for determinism/golden tests. The server scaffold PR (#978 implementing #152, run 44501556236) spent 1,926s in Windows native checks (1,370s tests); it also changed shared workspace/lock inputs, so it still selects full coverage, while later server-code-only edits can use the smaller scope. The last 60 merged PRs had median 12.43 min open-to-merge, p90 50.78 min, median 154.5 changed lines and 5 files. These are baseline observations, not promises of post-change speed; measure the new critical path from fresh Actions results.

The same checks locally, from `iwbtg-collection/`:

```
cargo clippy --workspace --tests -- -D warnings
cargo test --workspace
cargo fmt --all -- --check
```

Locally, run only what you changed: `cargo xtask check` selects changed crates, runs rustfmt, Clippy and their tests, then compile-checks dependents, and runs the WP-era ratchet (`cargo test -p xtask no_new_wp_era_identifiers`) when the plan includes format or sim, so a new WP id fails locally instead of only on main's full run (#1525); it also rejects a free-text `TODO(OPEN` marker under `crates/*/src` that names no issue or URL (#1119). `cargo xtask check --all` runs the whole native workspace. Selection comes from `scripts/ci_scope.py`, the same manifest graph CI uses: changed crates are tested locally and their transitive consumers are compile-checked, while CI tests both. `cargo xtask plan [--all]` prints the plan, its gates and the reason for each input without building; unknown or shared inputs select full coverage. Each step streams actionable lines and writes a complete log with its cwd, argv and exit status under `target/xtask/logs/`; a failure names the log and prints the last 120 lines unfiltered, so a nested compiler, pkg-config or test cause is visible without rerunning. Atomic receipts under `target/xtask/receipts/` record the plan identity (the content of the step's crate and in-repo dependency sources, the changed paths, shared lock/config/toolchain inputs and fingerprint environment), argv, cwd, status and duration; a later check reuses a step only when all of those match and the receipt checksum is intact. An unrelated prose change reruns only rustfmt, while failures, interruptions and damaged receipts rerun. `cargo xtask pr-body` marks a stale or missing plan identity instead of presenting an old result as current. The logs and receipts are untracked and obvious credentials are redacted. Cross-target tests, dependency bans and licenses remain in CI; private fidelity fixtures stay local to their authorized environment.

CI cannot see the research repo, so trace fidelity is checked locally after each merge wave (docs/architecture/testing.md, CI and the fidelity sweep):

```
IWBTG_RESEARCH=../iwbtg-collection-research cargo xtask fidelity
```

Feel contracts state the player-observable outcome (docs/architecture/testing.md, Feel contracts). The public set is part of the sim test suite, and the same sweep runs the CLI runner:

```
cargo test -p iwbtg-sim --test feel
cargo xtask feel
IWBTG_GAMES=../iwbtg-collection-games cargo xtask feel
```

The first is the CI gate on every target. The second runs the public fixture contracts; the third adds the port contracts under `crates/tools/feel/` and compares them with `crates/tools/feel-baseline.txt`. A worse status, a new failing contract or a removed contract exits 1; `--bless` is the deliberate acknowledgement and rewrites statuses only.

It prints a game x OK/OPEN/FAIL matrix and exits 1 when a case got worse than `crates/tools/fidelity-baseline.txt`; `--bless` rewrites the baseline after a deliberate change. The session-start hook (`.claude/hooks/issues.py start`) prints `FIDELITY STALE: ...` when commits touching `crates/sim` or `crates/format` landed on `origin/main` after the last sweep (recorded in `target/xtask/last-fidelity.txt`).

Without wasmtime, a wasm32-wasip1 test binary also runs under Node's WASI. Build it with `cargo test -p iwbtg-sim --target wasm32-wasip1 --no-run`, then run the printed `.wasm` with a small script using `node:wasi` (preview1).

### Progress dashboard refresh

`.github/workflows/progress.yml` publishes the generated page to `gh-pages`. It refreshes on:

- a push to `main` that touches `progress/**`, `docs/games/**` or the workflow itself;
- **every successful `CI` run on `main`** (`workflow_run`). This is the reliable path for an ordinary merge that touches no progress path; failed or cancelled CI runs are skipped;
- the daily `17 4 * * *` schedule, which is a backstop only. The cron added 2026-11-24 never fired and the Actions API has no `schedule` runs for this repository (#1547), so do not rely on it;
- `workflow_dispatch`.

To force a refresh from anywhere with repository access:

```
gh workflow run progress --ref main
gh run list --workflow progress.yml --limit 1
```

The run reads the public GitHub API with the repository token, restores the durable `history.json` from `gh-pages`, generates with `--gh --history --require-gh`, and commits `index.html`, `data.json`, `history.json`, `fonts/` and `.nojekyll` only when the generated bytes changed. It never writes `main`. The push to `gh-pages` triggers GitHub's `pages-build-deployment` workflow automatically; the progress workflow sends no separate build request (the POST raced the automatic build and could fail on an empty API response, #1570). The private games repo is not readable from CI, so the port column keeps its committed data there; rerun locally with games-repo access to refresh it. The published site is <https://nugamus.github.io/iwbtg-collection/>. A failed read or push fails the run visibly rather than publishing a stale page; a failed Pages build appears as a failed `pages-build-deployment` run.

## Home PC findings (2026-11-02)

- All three repos were behind GitHub; each now fast-forwards cleanly to `main`.
- research had uncommitted trace tool work. It is saved as `backup/home-pc-2026-11-02-local-trace-work`; research#27 lands it.
- `iwbtg-collection/.claude/worktrees/` held six workflow worktrees with unpushed WP1, WP2, WP3, WP4a, WP7a and WP8a work. Each is saved as `backup/home-pc-2026-11-02-<wp>`, and its issue (#1, #2, #3, #4, #8, #10) points to it. #203 removes the worktrees.
- The root `.claude/settings.json` lacked the issue hooks, and they are now added. #202 added the tracked copy and the drift warning.
- `iwbtg-collection-research/tools/opengmk/src/` is kept only to rebuild the tracer; iwbtg-collection-research/CLAUDE.md now says so (research#28).
- No originals, extraction output or database are tracked in any repo. `git status --ignored` shows them ignored in research.
