# IWBTG Collection: client

@README.md

## Rules

- Never commit original game binaries, decompiled code or extracted files. Those stay in the research repo (`../research`). Ported games live in the games repo (`../games`), rebuilt in our format from clean-room specs.
- Clean room: GameMaker behavior comes only from `research/specs/` (CLONE_SPECs) and the traces there. Never read another runner's or tool's source (OpenGMK, Jtool, renex, decompiled games) while writing client code. If the spec does not answer a question, mark it OPEN and request a spec item or trace.
- The simulation is deterministic: fixed tick at the game's original rate, no frame time in `sim`, number handling matching the original as its CLONE_SPEC documents. Timing, splits and leaderboards count ticks, never rendered frames.
- `sim` must not depend on Bevy.
- Game packages are readable text and never contain compiled code. Custom behavior goes only through our sandboxed script language. See `docs/format/README.md`.
- Each game keeps its own physics profile. Never "fix" a quirk or bug players rely on.
- Input is a fixed set of client actions (left, right, jump, shoot, restart, ...). Games map actions; they never read raw keys.
- Player-facing decisions (ranked stripped mode, mods, leaderboards, UI) are in `docs/product.md`. Porting, assets, engines, the package file and the editor are in `docs/content.md`. Follow both.
- Any per-game design question (stripped-mode cuts, run start and end, cutscenes, secrets, mods) goes into `docs/games/<game-id>.md` (see `docs/games/README.md`). Never guess the answer in code.
- Design is a priority. No generic UI; match the IWBTG style and reuse art from ported games where it fits.
- The user does not write code. Every PR still gets a User check section (exact commands, steps, expected vs. broken). The user tests only what they can see or play, and does so on `main` in batches, not per PR (decided 2026-11-05). So every PR is merged by the agent or lead once its User check has run with the output pasted into the PR: `gh pr merge <n> --squash --auto` turns on auto-merge: GitHub merges it as soon as the required checks (`ci-ok`, `body`) are green (#528). A PR that changes pinned hashes is brought up to date first (`gh pr update-branch <n>`). Never `--delete-branch`: it also removes the lane worktree checked out on that branch; GitHub deletes merged branches itself; a client UI or rendering PR also needs `--screenshot` images that match its design artboard. What the user still decides: playtests, design questions and `needs-user` issues. List each merged UI change with its User check in the lead's end-of-run report, so the user can test the batch; what the user finds becomes a fix issue.
- **Several agents may work at once** (Claude Code agent teams; client/docs/dev-setup.md, Agent teams). Teammates share the lead's folder, so:
  - Each agent works only in its own git worktree, never in the main checkout (that stays on `main` for the lead). Worktrees belong to a lane, not an issue, so their `target/` stays warm: `../worktrees/<repo>-<lane>` (for example `client-ui`). Create it once with `git worktree add ../worktrees/<repo>-<lane> -b issue-<n>-<short-name> origin/main`. For the next issue, commit, then `git fetch` and `git switch -c issue-<n>-<short-name> origin/main` in the same worktree. Use its absolute path for every command and file edit. A new worktree costs a 7 to 15 minute cold build; never remove one between issues.
  - Claim an issue before starting: add the `in-progress` label and comment. Skip issues that already have it. One issue per agent at a time. Remove the label if you hand the issue back.
  - Teammates open the PR and send its link to the lead. The lead reads the diff and turns on auto-merge (`gh pr merge <n> --squash --auto`); nobody waits for CI. Lane worktrees stay. After each merge wave the lead checks main's CI (`gh run list --branch main --limit 3`; `issues.py start` also prints it and puts `MAIN IS RED` on top when it failed): two PRs that are each green can break main together (#593, #595). A red main comes first: file or find its fix issue before merging more.
  - Each lane has a brief in `docs/lanes/<lane>.md` (owned files, fast test commands, pitfalls). Read it after the issue.
  - Two agents never edit the same files at once. The lead hands out issues that touch different files.

## Builds

A cold build of the workspace is about 7 minutes, a warm incremental one seconds. CI runs the full check matrix in about 3 minutes, off this PC. So:

- While working, build and test only what you touched: `cargo test -p <crate> --test it <file>::` (each crate's integration tests are one `it` executable, #517; a new test file must be listed in `tests/it.rs`). Before the PR run `cargo xtask check` once: rustfmt, clippy and tests of the crates the branch changed plus a compile check of their dependents (`--all` for the whole workspace). CI runs the rest. `cargo xtask repin` re-pins the hashes after a deliberate sim state change, `cargo xtask regen` regenerates schemas and script API docs, and `cargo xtask pr-body` drafts the PR body with the check's real output (#529).
- Never change the build fingerprint: no `RUSTFLAGS`, `--config`, `CARGO_PROFILE_*` or feature flags that differ from the defaults. Each change rebuilds every dependency (about 5 minutes) and again when you switch back.
- Run cargo in the foreground (the workspace settings allow 45 minutes). The `build_guard` hook blocks background cargo and fingerprint changes; `# build-guard: ok (<why>)` in the command overrides it. Never end your turn "waiting for" a background command: a teammate is not woken when it finishes, and sat idle 30 to 60 minutes per wait in the 2026-11-04 team run until the lead pinged it.
- `python -I client/.claude/stats.py` (from the workspace root) shows where each session's time went.

## Issues

Work is tracked in GitHub issues across the client, research and games repos, on the Project board "IWBTG Collection". The session-start hook shows milestone progress, the next unblocked issues, answered questions and the triage queue.

- Before starting, read the issue, the issues it is blocked by and its parent. Comment on it when you start (`gh issue comment`).
- One issue, one branch `issue-<n>-<short-name>`, one PR with `Closes #<n>`. Tick the issue's Done-when boxes as they pass. Stay inside its scope.
- **File what you find.** Whenever you notice something you will not fix in this issue (a bug, a gap, a missing test, a doc that no longer matches, an idea, a risk, a follow-up), search the open issues, then file it: `gh issue create --label triage` with Goal and Done when. Never leave a finding only in chat or in a code comment. The Stop hook makes every turn that changed files end with `Findings: <issue links>` or `Findings: none`.
- **Triage.** Issues labeled `triage` get a milestone, a `size:` and an `area:` label, and `blocked by` links (`gh api -X POST repos/nugamus/<repo>/issues/<n>/dependencies/blocked_by -F issue_id=<blocking issue's id>`). Then remove `triage`, and add `agent-ready` if it is fully specified. Split anything `size:L` into sub-issues (`.../issues/<n>/sub_issues -F sub_issue_id=<id>`).
- **Questions for the user** go in an issue comment, with the `needs-user` label on the issue, and propose a default ("If you don't answer, we'll do X"). When the choice is cheap to undo, go ahead with the default and add the `revisit` label; choices that are hard to undo (format, ranked rules, design direction, real people or assets) wait. Then move on to other work. Start every comment you post with `<!-- agent -->`; the hook uses it to tell the user's replies from yours. When the user has answered, act on it and remove `needs-user`.
- **Docs in the same PR.** A PR that changes behavior updates its docs (built-in pages, format spec, CLI, product or content decisions) in the same PR. Architecture notes go on the area's own page in `docs/architecture/` (client, cli, sim, replay, traces, testing), never as new text in the `docs/architecture.md` index, so lanes don't conflict there. The client has one page per module in `docs/architecture/client/` (`client.md` is only the index): a note goes on its module's page, and a new module gets a new page plus one alphabetical line in the index (dev-setup.md, Agent teams).
- When a milestone's issues are all closed, close the milestone.

## Maintaining this harness

Any agent may edit this file and `.claude/`. Keep it lean: add a line only when it would have prevented a real mistake, and delete lines that go stale.
