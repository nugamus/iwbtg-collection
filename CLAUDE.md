# IWBTG Collection: agent instructions

## Contracts

- Clean room: GameMaker behavior comes only from `../iwbtg-collection-research/specs/` and its traces. Never read another runner/tool's source or decompiled games. An unanswered behavior question stays OPEN and gets a spec/trace issue.
- Never commit original binaries, decompiled code or extracted assets. Original material stays in `../iwbtg-collection-research`; clean-room ports in `../iwbtg-collection-games`. Keep the existing credited Kid sprites in `crates/client/showcase.pkg` (user exception, games#34); add no more.
- `sim` is deterministic, fixed tick, and independent of Bevy, IO, clocks and threads. Timing, splits and leaderboards count ticks. Preserve what a player feels — position, death, timing, RNG on a played path, input response, respawn control — not incidental engine implementation. An invisible implementation detail is free to redesign; an F1 change needs a feel contract and, where irreversible, the user ([0005](docs/decisions/0005-standard-mechanics.md)).
- Packages are readable text with sandboxed scripts, never compiled code. Input uses client actions, never raw keys.
- Standardize by default. New games use the standard ruleset; ports choose a data-only compatibility profile that preserves their feel. One-game behavior goes to data, a script or a profile, never a shared-code quirk ([0004](docs/decisions/0004-extension-model.md), [0005](docs/decisions/0005-standard-mechanics.md)). Every deliberate deviation gets a deviation-log entry and a feel contract.
- Product/ranked/UI decisions: `docs/product.md`. Porting/assets/format/editor: `docs/content.md`. Read the relevant section when working in that area. Per-game questions belong in `docs/games/<id>.md`; never guess. Follow approved IWBTG artboards; visible rendering changes need matching `--screenshot` evidence.

## Work loop

1. Read the assigned issue, its blockers/parent, and the relevant `docs/lanes/<lane>.md`. Load relevant symbols and module docs; expand context only when a dependency or failing check requires it.
2. Claim the issue (`in-progress` plus a comment); skip existing claims. One issue/branch/PR, unless the user assigns a broader change. Keep unrelated edits intact.
3. Use one implementer for a routine issue. Additional agents need independent work that repays coordination; avoid serial reviewer-agent chains. The PR owner reviews the diff once; repeat review/checks when relevant inputs change or a finding requires it.
4. Run the narrow tests while editing. Run `cargo xtask check` once before the PR; it tests changed crates and compile-checks dependents. `--all` is the full workspace. A failed step names its complete log (`target/xtask/logs/`) and prints the unfiltered tail; read it instead of rerunning. Receipts (`target/xtask/receipts/`) reuse a step only when its recorded identity, argv and cwd match; `pr-body` marks a stale check. Record real output and disclose blocked checks.
5. Update affected docs in the same PR. Link `Closes #N`, fill Docs updated, Follow-up issues and User check. The user tests visible/playable changes on main in batches; the PR owner runs the concrete User check first and records its evidence.
6. Request `gh pr merge <N> --squash --auto` when ready; required `ci-ok`/`body` gates still apply. No polling loop or separate lead handoff for routine PRs. Update a branch that changes hash pins first. Never `--delete-branch` (lane worktrees stay).

## Build and integration discipline

- Keep lane worktrees/targets warm between issues; never share one active worktree. See [workflow details](docs/agent-workflow.md) when coordinating lanes.
- Keep the normal build fingerprint: no ad hoc RUSTFLAGS, profiles, feature flags, linker or target-directory overrides. Run cargo in the foreground; do not end a turn waiting for an unattended build. Follow the existing build/conflict guards. A cold or heavy client build takes the machine-wide slot: `python -I scripts/heavy_build.py -- cargo build -p iwbtg-client`; editing, searches and light or warm checks stay concurrent.
- `cargo xtask check` and CI share one manifest-derived plan in `scripts/ci_scope.py`; `cargo xtask plan` explains the selection without building. Unknown or shared inputs select full coverage.
- `cargo xtask repin` regenerates hash pins after an intentional sim change; take either side of pin conflicts then repin, never hand-merge. `cargo xtask regen` regenerates schemas/API docs; `cargo xtask pr-body` drafts the body from check output.
- After merge waves, check main CI; repair red main before further merges. Run `IWBTG_RESEARCH=../iwbtg-collection-research cargo xtask fidelity` after sim/format changes (private fixtures required). Never bless an unexplained regression. Every mechanic and fidelity issue gets a feel-impact tier when it is filed: F1 (feel-critical) gates Ranked and M2; F2 and F3 move to the affected game's port issue or M5. T1–T3 remain aliases (`docs/plans/fidelity-tiers.md`).

## Findings — mandatory

**File what you find.** For every unfinished bug, gap, missing test, doc drift, idea, risk or follow-up: search existing issues, then create a GitHub issue with `triage`, Goal and Done when. Never leave findings only in chat/comments. End every turn that edited files with `Findings: <issue links>` or `Findings: none`. Keep the Stop hook enforcing this.

Start issue comments with `<!-- agent -->`. Tick completed acceptance criteria. User questions go on the issue with `needs-user` and a proposed default; irreversible format/ranked/design/people/assets decisions wait. Follow-up triage, milestone and documentation placement details are in [workflow details](docs/agent-workflow.md); read them when doing that work.
