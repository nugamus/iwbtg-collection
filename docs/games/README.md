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

- **Kept quirks:** behavior players rely on, kept on purpose.
- **Fixed bugs:** crashes, softlocks, broken graphics and typos, fixed when a trace shows that positions do not change.
- **Known differences:** what still differs from the original, whether or not Beta accepts it.
- **Open:** unresolved fidelity items, including a tier still `T?` and any item moved to the port issue.

Every entry names:

1. Its **impact tier** (`T1`, `T2`, `T3` or `T?`; definitions and the blocking rule in [fidelity tiers](../plans/fidelity-tiers.md)).
2. Its **evidence**: a trace case name or a spec claim id.
3. Its **decision date** (`YYYY-MM-DD`) for a kept quirk, fixed bug or accepted known difference; the opened date for an Open item.

Template (the questions section above stays as it is; this log is added to the same file):

```markdown
## Deviation log

### Kept quirks

- **<quirk>** — Tier T1. Evidence: <trace case or spec claim id>. Decided YYYY-MM-DD. <why players rely on it and how the port keeps it>.

### Fixed bugs

- **<bug>** — Tier T1. Evidence: <trace case or claim id> shows positions do not change. Decided YYYY-MM-DD. <what was fixed>.

### Known differences

- **<difference>** — Tier T2. Evidence: <trace case or claim id>. Decided YYYY-MM-DD. <what a player sees or hears, and whether Beta accepts it>.

### Open

- **<question>** — Impact T?. Evidence needed: <trace case or spec claim id>. Opened YYYY-MM-DD. <what settles it and which port issue or milestone it moved to>.
```

- **T1 blocks Ranked; T2 blocks Beta only if visible; T3 never blocks.** A deviation accepted into Beta still gets an entry here, with the date the user or lead accepted it.
- **T2 and T3 items move to the affected game's port issue (games repo, M5) or to M5.** The entry stays here and names where it moved.
- **The game page's "known differences"** ([content.md](../content.md), Game list data) is generated from the Known differences section; do not write a second list.
- **The deviation log's Open is not the questions Open.** `## Open` above is for the user; `### Open` here is for fidelity work.
