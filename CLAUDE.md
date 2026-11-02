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
- The user does not write code. Every PR still gets a User check section (exact commands, steps, expected vs. broken). The user tests only what they can see or play: client UI and rendering, playtests, design, and `needs-user` issues. Those PRs wait for the user to merge. Any other PR (sim, format, tools, research, docs) you merge yourself (`gh pr merge --squash --delete-branch`) once CI is green and you have run its User check and pasted the output into the PR.
- One agent works at a time. Finish or hand off the current issue before starting another.

## Issues

Work is tracked in GitHub issues across the client, research and games repos, on the Project board "IWBTG Collection". The session-start hook shows milestone progress, the next unblocked issues, answered questions and the triage queue.

- Before starting, read the issue, the issues it is blocked by and its parent. Comment on it when you start (`gh issue comment`).
- One issue, one branch `issue-<n>-<short-name>`, one PR with `Closes #<n>`. Tick the issue's Done-when boxes as they pass. Stay inside its scope.
- **File what you find.** Whenever you notice something you will not fix in this issue (a bug, a gap, a missing test, a doc that no longer matches, an idea, a risk, a follow-up), search the open issues, then file it: `gh issue create --label triage` with Goal and Done when. Never leave a finding only in chat or in a code comment. The Stop hook makes every turn that changed files end with `Findings: <issue links>` or `Findings: none`.
- **Triage.** Issues labeled `triage` get a milestone, a `size:` and an `area:` label, and `blocked by` links (`gh api -X POST repos/nugamus/<repo>/issues/<n>/dependencies/blocked_by -F issue_id=<blocking issue's id>`). Then remove `triage`, and add `agent-ready` if it is fully specified. Split anything `size:L` into sub-issues (`.../issues/<n>/sub_issues -F sub_issue_id=<id>`).
- **Questions for the user** go in an issue comment, with the `needs-user` label on the issue. Then move on to other work. Start every comment you post with `<!-- agent -->`; the hook uses it to tell the user's replies from yours. When the user has answered, act on it and remove `needs-user`.
- **Docs in the same PR.** A PR that changes behavior updates its docs (built-in pages, format spec, CLI, product or content decisions) in the same PR.
- When a milestone's issues are all closed, close the milestone.

## Maintaining this harness

Any agent may edit this file and `.claude/`. Keep it lean: add a line only when it would have prevented a real mistake, and delete lines that go stale.
