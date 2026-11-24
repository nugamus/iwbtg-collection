# Agent workflow details

Read this page when coordinating worktrees, triaging, or placing documentation. The short daily contract is in `CLAUDE.md`.

## Daily workflow (authoritative)

1. Read the issue, its blockers/parent and `docs/lanes/<lane>.md`; claim it with `in-progress` and a comment.
2. Work in your lane worktree. Run the narrow tests for what you touched, then `cargo xtask check` once before the PR. It streams actionable lines, keeps a complete per-step log under `target/xtask/logs/`, and reuses a step only when its `target/xtask/receipts/` identity, argv and cwd still match; read the log or rerun only the invalidated step instead of repeating everything.
3. Update affected docs; open the PR with `Closes #N`, Docs updated, Follow-up issues and the real User check output.
4. The PR owner reviews the diff, runs the User check and requests `gh pr merge <N> --squash --auto`. Contract/cross-area changes seek domain review; routine fixes need no separate lead or reviewer chain.
5. A lead only handles spawning and integration after merge waves; the user tests visible changes on `main` in batches.

## Worktrees and coordination

Keep one warm worktree per lane: `../worktrees/<repo>-<lane>`. Create it once with `git worktree add ../worktrees/<repo>-<lane> -b issue-<N>-<name> origin/main`; reuse it for the next branch after committing and fetching. Use the lane's absolute working directory for every edit/command. Do not remove a worktree between issues or change its branch while another agent owns it. Wait for the previous owner to shut down before reassigning the lane. Never let two agents edit the same files/worktree.

The PR owner performs the normal diff review against the [PR review checklist](#pr-review-checklist), executes the User check and requests auto-merge. Seek a lead/domain review when a change crosses ownership or changes a behavior contract; avoid mandatory implementer → reviewer agent → lead review chains on routine fixes. Remote required approvals/checks still apply. Leads integrate after merge waves: inspect main CI, repair breakage and run fidelity when sim/format changed. Screenshots are required for UI/rendering changes. User decisions and batched main playtests remain with the user.

## PR review checklist

The PR owner reviews the diff once against this list before requesting merge ([0005](decisions/0005-standard-mechanics.md)); repeat it when relevant inputs change or a finding requires it.

- **Is this F1?** A mechanic or fidelity change states its tier in the issue and the PR. F1 is a change to what a skilled player can do (position, death, timing, RNG on a played path, input response, respawn control); F2 is visual or audio only; F3 is unobservable.
- **What is the contract?** An F1 change names its feel-contract id and evidence; an F2 change names the screenshot or audio check; an F3 change says why no player can observe it. A reused trace names its case and what it settles.
- **Does the ruleset or profile change?** A change to a ruleset, `[compat]` profile or deviation says so, and a change that can move an outcome carries the user's decision ([versioning](architecture/versioning.md)).
- **Is the board migration explicit?** A behavior change gets a new `gameplay_hash` and board; the old board and its replays stay verifiable on the old build. No silent edit to a ranked outcome.
- **Is the docs trail complete?** Behavior docs land in the same PR, and a deliberate deviation has a deviation-log entry and a feel contract ([games](games/README.md#deviation-log)).

## Issues and triage

Issue creation is mandatory for unfinished findings; `CLAUDE.md` and the Stop hook retain that rule. Deduplicate first. Every new finding gets Goal, Done when and `triage`; include files, contract/spec pointers and the narrow test command so the next agent can start directly.

During triage, assign the phase milestone of the work blocked (not whichever is open), a `size:` and an `area:` label. Add `blocked by` relationships through the GitHub issue dependency API. Remove `triage`; add `agent-ready` only when fully specified. Split size L work into bounded sub-issues. Close a phase milestone when all its issues close. Follow `docs/roadmap.md` and fidelity tiers when placing work.

Every mechanic and every fidelity issue gets an F1/F2/F3 feel-impact tier **and a feel-contract id** in its triage comment ([fidelity tiers](plans/fidelity-tiers.md)): `Impact: F1 — <one line>` then `Contract: <area>/<name>`, or the reason none applies (F3: pinned by a hash or unit test; F2: a screenshot or audio check). An F3 detail is free to redesign; an F2 change needs a screenshot or audio check; an F1 change needs a feel contract and, where irreversible, the user. A redesign opportunity is filed as an issue, never quietly cloned into shared code: one-game behavior goes to data, a script or a profile ([0004](decisions/0004-extension-model.md), [0005](decisions/0005-standard-mechanics.md)). Every deliberate deviation gets a deviation-log entry and a feel contract; the shared mechanics stay general.

**Issue template.** An issue an agent can start from carries, in this order: a `<!-- agent -->` header; `## Goal`; `## Done when` as checkboxes; `Impact: F1|F2|F3 — <one line>` (the first line of the triage comment, not a label); the files and current behavior; the target model; the player-visible difference; the feel contract or the reason none applies (`Contract: <area>/<name>`); migration; risks, size, phase and blockers. The [public issue forms](../.github/ISSUE_TEMPLATE/) ask a reporter for the gameplay impact in plain words; the agent maps it to the F tier and adds the contract at triage.

For user questions, start comments with `<!-- agent -->`, add `needs-user` and propose a default. Cheap reversible choices may proceed with `revisit`; irreversible format, ranked, design, people or asset decisions wait. Act on an answer and remove `needs-user`.

## Documentation placement

Update behavior docs in the same PR. Put architecture notes on the affected area's page under `docs/architecture/`, never append them to the architecture index. Client and sim modules have individual pages in `docs/architecture/client/` and `docs/architecture/sim/`; a new page gets one alphabetical index entry. Sim UNKNOWN rows belong in `docs/architecture/sim/register-<area>.md`; converter requests in `docs/format/converter-requests.md`. Game-specific choices go in `docs/games/<id>.md`.

## Context and measurements

Do not preload the roadmap, all specs, every lane or historical plans for a bounded issue. Read its context pointers, the applicable lane and the functions being changed, then expand to callers/contracts/tests. Batch independent searches. Preserve useful context in the issue so each agent does not repeat discovery.

The status hook reuses successful snapshots for at most 120 seconds; `python -I .claude/hooks/issues.py start --refresh` forces a live read. Re-read a specific issue/PR before claiming or merging; a cached dashboard is not an ownership decision. Cache failure never disables issue creation or the findings Stop check.

Measure session costs with `python -I iwbtg-collection/.claude/stats.py --summary --days 7` from the multi-repo workspace. Append aggregate numbers (no transcript content) to `docs/velocity.md`. CI scope and preserved gates are described in `docs/dev-setup.md`.
