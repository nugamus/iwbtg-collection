# Contributing to IWBTG Collection

Thanks for helping. This repository holds the client code (Rust and Bevy), the game-package format and the project docs. Ported games live in a separate repository; clean-room specs, traces and original game material are kept private.

Read `../CLAUDE.md` for the project rules and `../docs/dev-setup.md` for the toolchain and machine setup.

## Licenses

- Code is licensed under the [MIT License](../LICENSE-MIT).
- Documentation in `docs/` is licensed under [CC BY 4.0](../LICENSES/CC-BY-4.0.txt).
- Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in the work is licensed under the MIT License, without any additional terms or conditions. This matches the [README](../README.md#credits-and-license).
- Do not change the license terms. Game content is not covered by them.

## Clean room

The client rebuilds fangames from clean-room specs and recorded traces, never from the original games:

- Do not submit decompiled code, original game binaries or extracted game files. Examples include `.exe`, `.win`, `.gmk`, `.gms`, GameMaker data files and archives of a fangame.
- Do not submit assets taken from a game unless the user has explicitly approved that credited exception (the Kid sprites in `crates/client/showcase.pkg` are the only one).
- Behavior comes only from the specs and traces in the private research repo. If a spec does not answer your question, open an issue asking for a spec item or trace instead of guessing or reading another runner's source.
- Game content belongs to its creators. See "Games and assets" in the [README](../README.md#credits-and-license).

## Pull requests

- One issue, one branch `issue-<n>-<short-name>`, one PR whose body starts with `Closes #<n>`.
- Update the docs a change affects in the same PR.
- Fill in every section of the [pull request template](pull_request_template.md), including a **User check**: the commands or steps to run and what the user should see (expected vs. broken). The user tests visible changes, so make it something they can see or play.
- Run `cargo xtask check` before opening the PR, or say in the PR why it was not run.
- Keep the workspace build fingerprint at its defaults; do not add `RUSTFLAGS`, feature flags or profile overrides (see `../CLAUDE.md`, Builds).

## Security

Do not open a public issue for a security problem. Follow [SECURITY.md](SECURITY.md).
