# Per-game questions

There is one file per game, `<game-id>.md`, named after the package id. It holds the design questions about that game that only the user can answer:

- What stripped mode cuts.
- Where the ranked run starts and ends.
- How cutscenes and dialogue are handled.
- Which items are secrets.
- Which mods apply.
- Anything else game-specific.

Any agent that runs into such a question creates or updates the game's file and keeps working on whatever does not depend on the answer. Do not guess the answer in code.

Template:

```markdown
# <Game title> (`<game-id>`)

## Open

- **Q:** <question>. Context: <what the agent saw, with room or object names>. Options: <a, b>.

## Answered

- **Q:** <question> **A:** <user's answer> (YYYY-MM-DD)
```

When the user answers, move the item to Answered. Answers that change the package also go into the package itself.
