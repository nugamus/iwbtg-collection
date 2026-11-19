# Agent workflow details

Read this page when coordinating worktrees, triaging, or placing documentation. The short daily contract is in `CLAUDE.md`.

## Daily workflow (authoritative)

1. Read the issue, its blockers/parent and `docs/lanes/<lane>.md`; claim it with `in-progress` and a comment.
2. Work in your lane worktree. Run the narrow tests for what you touched, then `cargo xtask check` once before the PR. It streams actionable lines and keeps a complete per-step log under `target/xtask/logs/`; read that log instead of rerunning a failed step.
3. Update affected docs; open the PR with `Closes #N`, Docs updated, Follow-up issues and the real User check output.
4. The PR owner reviews the diff, runs the User check and requests `gh pr merge <N> --squash --auto`. Contract/cross-area changes seek domain review; routine fixes need no separate lead or reviewer chain.
5. A lead only handles spawning and integration after merge waves; the user tests visible changes on `main` in batches.

## Worktrees and coordination

Keep one warm worktree per lane: `../worktrees/<repo>-<lane>`. Create it once with `git worktree add ../worktrees/<repo>-<lane> -b issue-<N>-<name> origin/main`; reuse it for the next branch after committing and fetching. Use the lane's absolute working directory for every edit/command. Do not remove a worktree between issues or change its branch while another agent owns it. Wait for the previous owner to shut down before reassigning the lane. Never let two agents edit the same files/worktree.

The PR owner performs the normal diff review, executes the User check and requests auto-merge. Seek a lead/domain review when a change crosses ownership or changes a behavior contract; avoid mandatory implementer → reviewer agent → lead review chains on routine fixes. Remote required approvals/checks still apply. Leads integrate after merge waves: inspect main CI, repair breakage and run fidelity when sim/format changed. Screenshots are required for UI/rendering changes. User decisions and batched main playtests remain with the user.

## Issues and triage

Issue creation is mandatory for unfinished findings; `CLAUDE.md` and the Stop hook retain that rule. Deduplicate first. Every new finding gets Goal, Done when and `triage`; include files, contract/spec pointers and the narrow test command so the next agent can start directly.

During triage, assign the milestone of the work blocked (not whichever is open), a `size:` and an `area:` label. Add `blocked by` relationships through the GitHub issue dependency API. Remove `triage`; add `agent-ready` only when fully specified. Split size L work into bounded sub-issues. Close a milestone when all its issues close. Follow `docs/roadmap.md` and fidelity tiers when placing work.

For user questions, start comments with `<!-- agent -->`, add `needs-user` and propose a default. Cheap reversible choices may proceed with `revisit`; irreversible format, ranked, design, people or asset decisions wait. Act on an answer and remove `needs-user`.

## Documentation placement

Update behavior docs in the same PR. Put architecture notes on the affected area's page under `docs/architecture/`, never append them to the architecture index. Client and sim modules have individual pages in `docs/architecture/client/` and `docs/architecture/sim/`; a new page gets one alphabetical index entry. Sim UNKNOWN rows belong in `docs/plans/sim-core/register-<area>.md`; converter requests in `converter-requests.md`. Game-specific choices go in `docs/games/<id>.md`.

## Context and measurements

Do not preload the roadmap, all specs, every lane or historical plans for a bounded issue. Read its context pointers, the applicable lane and the functions being changed, then expand to callers/contracts/tests. Batch independent searches. Preserve useful context in the issue so each agent does not repeat discovery.

The status hook reuses successful snapshots for at most 120 seconds; `python -I .claude/hooks/issues.py start --refresh` forces a live read. Re-read a specific issue/PR before claiming or merging; a cached dashboard is not an ownership decision. Cache failure never disables issue creation or the findings Stop check.

Measure session costs with `python -I client/.claude/stats.py --summary --days 7` from the multi-repo workspace. Append aggregate numbers (no transcript content) to `docs/velocity.md`. CI scope and preserved gates are described in `docs/dev-setup.md`.
