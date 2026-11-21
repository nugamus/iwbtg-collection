# Per-game pages

There is one file per game, `<game-id>.md`, named after the package id. It holds two things:

- The design questions about that game that only the user can answer (below).
- The **deviation log**: the quirks the port keeps, the bugs it fixes, the differences a player can still see, and the open fidelity items ([deviation log](#deviation-log)).

Any agent that runs into a user question or a fidelity deviation creates or updates the game's file and keeps working on whatever does not depend on the answer. Do not guess the answer in code.

## Per-game questions

These are the questions only the user can answer:

- What stripped mode cuts.
- Where the ranked run starts and ends.
- How cutscenes and dialogue are handled.
- Which items are secrets.
- Which mods apply.
- Anything else game-specific.

Template:

```markdown
# <Game title> (`<game-id>`)

## Open

- **Q:** <question>. Context: <what the agent saw, with room or object names>. Options: <a, b>.

## Answered

- **Q:** <question> **A:** <user's answer> (YYYY-MM-DD)
```

When the user answers, move the item to Answered. Answers that change the package also go into the package itself.

## Deviation log

[content.md](../content.md#porting) (Porting, Fidelity) requires every kept quirk and every fixed bug to be logged in `docs/games/<game-id>.md`. The log uses four fixed sections so a porter, a reviewer and the game page can read the same record:

- **Kept quirks:** behavior players rely on, kept on purpose. To add one, name the quirk, give it an F tier, name the feel contract that proves the felt outcome (F1), and say why players rely on it.
- **Fixed bugs:** crashes, softlocks, broken graphics and typos, fixed when a feel test shows the change is not felt (and, where positions could move, a trace shows they do not). To add one, name the F tier, the contract that proves the fix is not felt, and the trace or claim that shows positions do not change.
- **Known differences:** what still differs from the original, whether or not Beta accepts it, including every deliberate standardization. To add one, state the F tier and say whether a player sees or hears it (F2, with a screenshot or audio check) or cannot (F3, with the hash or unit test that pins it).
- **Open:** unresolved fidelity items, including a tier still `F?` and any item moved to the port issue. To add one, say which contract, trace case or spec claim would settle it, and name where it moved.

Every entry names:

1. Its **impact tier** (`F1`, `F2`, `F3` or `F?`; `T1`–`T3` are aliases; definitions and the blocking rule in [fidelity tiers](../plans/fidelity-tiers.md)).
2. Its **feel contract**: the contract id (`<area>/<name>` under `crates/tools/feel/`, [feel contracts](../architecture/feel-contracts.md)) that proves the tier, or why none applies (`F3`: pinned by a hash or unit test; `F2`: a screenshot or audio check; `Open`: the contract still to be written). An F1 entry without a contract is an unfinished entry, not an accepted one.
3. Its **evidence**: the trace case, spec claim id or dated playtest behind the tier.
4. Its **decision date** (`YYYY-MM-DD`) for a kept quirk, fixed bug or accepted known difference; the opened date for an Open item.

Template (the questions section above stays as it is; this log is added to the same file):

```markdown
## Deviation log

### Kept quirks

- **<quirk>** — Tier F1. Contract: `<area>/<name>`. Evidence: <trace case or spec claim id>. Decided YYYY-MM-DD. <why players rely on it and how the port keeps it>.

### Fixed bugs

- **<bug>** — Tier F1. Contract: `<area>/<name>` shows the change is not felt. Evidence: <trace case or claim id> shows positions do not change. Decided YYYY-MM-DD. <what was fixed>.

### Known differences

- **<difference>** — Tier F2 (visible) or F3 (invisible). Contract: `<area>/<name>` (F2) or none — pinned by <hash/unit test> (F3). Evidence: <screenshot/audio check, trace case or claim id>. Decided YYYY-MM-DD. <what a player sees or hears, and whether Beta accepts it>.

### Open

- **<question>** — Impact F?. Contract needed: `<area>/<name>`. Evidence needed: <trace case or spec claim id>. Opened YYYY-MM-DD. <what settles it and which port issue or milestone it moved to>.
```

- **F1 blocks Ranked; F2 blocks Beta only if visible; F3 never blocks.** A deviation accepted into Beta still gets an entry here, with the date the user or lead accepted it.
- **Existing entries** keep their text and gain a `Contract:` line the next time they are touched. An entry written before this field existed and still without a contract is a to-do in `### Open`, not an invalidated accepted difference.
- **F2 and F3 items move to the affected game's port issue (games repo, M5) or to M5.** The entry stays here and names where it moved.
- **The game page's "known differences"** ([content.md](../content.md), Game list data) is generated from the Known differences section; do not write a second list.
- **The deviation log's Open is not the questions Open.** `## Open` above is for the user; `### Open` here is for fidelity work.
