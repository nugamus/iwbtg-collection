# Contributing to IWBTG Collection

Thanks for helping. This repository holds the client (Rust and Bevy), the game-package format, the command-line tools and the project docs. Ported games live in a separate private repository; clean-room specs, traces and original game material stay in another private one and never enter this repository.

Read the [README](../README.md) first for what the project is and where the code lives, and [dev-setup](../docs/dev-setup.md) for the full machine setup. [CLAUDE.md](../CLAUDE.md) holds the project rules; it is written for agents, and the rules apply to every change, human or agent.

This page covers how to build and test, what rules a change must keep, and what a pull request must contain. Porting a game has its own guide: [porting guide](../docs/porting-guide.md).

## Before you start

- Find or open an issue for the change. An issue with the `in-progress` label is already claimed; comment on it before starting, and do not work on the same issue as someone else.
- Keep it to one issue, one branch named `issue-<n>-<short-name>`, and one pull request. Split unrelated work into its own issue.
- The maintainer makes the irreversible decisions. Changes that can move a ranked outcome, change the package format, or decide people, assets or design wait for an answer on the issue; a cheap reversible choice can proceed.

## Set up

The Rust toolchain is pinned by `rust-toolchain.toml`, so rustup installs the right version. The platform packages (Linux system libraries, `wasm32` targets, browser tools) are listed in [dev-setup](../docs/dev-setup.md).

```sh
cargo xtask doctor --scope native
cargo build -p iwbtg-client
cargo run -p iwbtg-client -- crates/client/showcase.pkg --mute
```

`--mute` is there so a test run never plays sound on someone else's speakers. For the browser build, the test suite and the toolchain list, see [dev-setup](../docs/dev-setup.md).

## Build and test

Run the tests of what you touched while you work, then the local gate once before opening the pull request:

```sh
cargo xtask check
```

`cargo xtask check` selects the crates your change touches from the same manifest plan CI uses (`scripts/ci_scope.py`), runs rustfmt, Clippy (warnings are errors) and their tests, then compile-checks the dependent crates. It writes a complete log per step under `target/xtask/logs/` and prints the failing log's tail, so read that instead of rerunning everything. `cargo xtask plan` prints the selection without building. `cargo xtask check --all` runs the whole native workspace and is slow; it is for a change that touches shared inputs.

Documentation-only changes skip the Rust builds; keep prose changes separate from code changes so the fast path stays fast. `cargo xtask pr-body` drafts a pull-request body from the check output.

Keep the normal build fingerprint: no ad hoc `RUSTFLAGS`, profiles, feature flags, linker or target-directory overrides. Anything that changes the fingerprint rebuilds every dependency, and CI does not set those variables either.

## Clean room

The client rebuilds fangames from clean-room specs and recorded traces, never from the original games:

- Behavior comes only from the specs and traces in the private research repository (`research/specs/`). Never read another runner's or tool's source, and never read a decompiled game, while designing or writing code.
- Do not submit decompiled code, original game binaries or extracted game files. Examples include `.exe`, `.win`, `.gmk`, `.gms`, GameMaker data files and archives of a fangame.
- Do not submit assets taken from a game unless the credited exception has been approved (the Kid sprites in `crates/client/showcase.pkg` are the only one).
- If a spec does not answer your question, open an issue asking for a spec item or a trace instead of guessing or reading another runner. Do not invent original behavior you cannot evidence; use the standard default and say so.
- Packages are readable text and sandboxed scripts, never compiled code. Game content belongs to its creators; see the [README](../README.md#credits-and-license).

## Report what you find

Every unfinished bug, gap, missing test, doc drift, risk or follow-up becomes a GitHub issue, not a comment left behind: search existing issues first, then open one with `## Goal`, `## Done when` as checkboxes, and the `triage` label so it can be placed and sized. If you find a behavior or fidelity difference, say what a player sees or feels and give it a feel-impact tier ([fidelity tiers](../docs/plans/fidelity-tiers.md)).

Update the docs a change affects in the same pull request. Architecture notes go on the affected area's page under `docs/architecture/`; game-specific decisions go in `docs/games/<id>.md`; converter gaps go in [converter requests](../docs/format/converter-requests.md).

## Pull requests

1. Branch from `main` as `issue-<n>-<short-name>` and start the body with `Closes #<n>`.
2. Fill every section of the [pull request template](pull_request_template.md):
   - **What changed** and **Docs updated**: which docs the PR touches, or why none are needed.
   - **Follow-up issues**: the issues filed for findings, or "none".
   - **Review checklist**: ticked against the diff. State the feel-impact tier; an F1 change names its feel contract and evidence, an F2 change names a screenshot or audio check, an F3 change says why no player can observe it. A ruleset, `[compat]` profile or deviation change says so, with the board migration and the user's decision.
   - **User check**: the commands or steps to run and what the user should see, expected versus broken. Visible and playable changes must be something the user can check on `main`, so make it concrete.
3. Run `cargo xtask check` (or say in the body why it was not run) and record its real output. Do not present a stale check as current.
4. Keep the change to the issue's scope. Do not reformat or edit unrelated files.

A pull request merges as a squash commit once its required checks pass. Review your own diff against the checklist and record the real User check output; the maintainer merges when the required `ci-ok` and `body` gates are green. Never force-push over review comments, and never delete a branch someone else works on.

## Licenses

- Code is licensed under the [MIT License](../LICENSE-MIT).
- Documentation in `docs/` is licensed under [CC BY 4.0](../LICENSES/CC-BY-4.0.txt).
- Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in the work is licensed under the MIT License, without any additional terms or conditions. This matches the [README](../README.md#credits-and-license).
- Do not change the license terms. Game content is not covered by them.
- A new file needs an entry in [REUSE.toml](../REUSE.toml); check it with `uvx --from "reuse[charset-normalizer]" reuse lint` ([dev-setup](../docs/dev-setup.md#licences)).

## Security

Do not open a public issue for a security problem. Follow [SECURITY.md](SECURITY.md).
