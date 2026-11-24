Closes #

## What changed


## Docs updated

Which docs this PR updates, or why none are needed.

## Follow-up issues

Issues filed for findings during this work, or "none".

## Review checklist

The PR owner ticks this against the diff before requesting merge; the full list and the tier definitions are in [agent workflow](docs/agent-workflow.md#pr-review-checklist) and [fidelity tiers](docs/plans/fidelity-tiers.md).

- [ ] The F tier is stated; an F1 change names its feel contract and evidence, an F2 change names a screenshot or audio check, an F3 change says why no player can observe it.
- [ ] A ruleset, `[compat]` profile or deviation change is called out, with its board migration (new `gameplay_hash`, old board kept) and the user's decision where an outcome can move.
- [ ] Reused traces name their case and what they settle; changed contracts, hashes and pins are named in "What changed".
- [ ] Behavior docs land in this PR, and a kept difference has a deviation-log entry with a contract.

## User check

Commands to run and what to look for (expected vs. broken).
