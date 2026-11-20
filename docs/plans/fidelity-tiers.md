# Fidelity impact tiers

Every mechanic, and every open fidelity question, gets a feel-impact tier. The tier says whether the work can change what a player observes, so the project can stop treating a one-game visual detail as equal to a physics rule. This page defines the tiers and the blocking rule; [content.md](../content.md#porting) (Porting, Fidelity) is the principle they apply.

> **Fidelity is feel.** What players rely on is the feel: position, death, timing, RNG on a played path, input response and respawn control. Those are never changed silently. Bugs nobody wants (crashes, softlocks, broken graphics, typos) are fixed when a feel contract and, where positions could move, a trace show the player-observable result is unchanged. Every fix and every deliberate deviation is logged in `docs/games/<game-id>.md`.

That rule asks one question of every quirk, fix and mechanic: does it change what a player can do or observe? The tiers record the answer once, in a place a porter and a reviewer can read.

## Feel-impact tiers (F1/F2/F3)

The tier is the same judgement as the fidelity tiers below, named for what it protects. Every mechanic gets one, not only an open spec question ([decision 0005](../decisions/0005-standard-mechanics.md)).

| Feel tier | Fidelity alias | Meaning |
|---|---|---|
| **F1 feel-critical** | T1 | Can change what a skilled player can do: position, death, timing, RNG on a played path, input response, respawn control. |
| **F2 feedback** | T2, visible | Only visual or audio, but affects learning or clarity; a player can see or hear the difference. |
| **F3 invisible** | T2, invisible; T3 | No player can observe it: a boot room, a temp file, an internal id, a probe-only value, an internal event order. |

F3 is free to redesign, provided determinism and the pinned hashes still hold. F2 needs a contract and screenshot/audio evidence. F1 needs a feel contract and, for a redesign, the strongest trace or playtest evidence available; an irreversible F1 change also needs the user.

A trace diff is evidence for an F1 mechanic. It is not a mandate to clone every probe column: an F3 difference is recorded and never blocks. A feel contract ([feel contracts](../architecture/feel-contracts.md)) states the outcome that must hold, including where a probe column or an F3 detail differs.

## The tiers

| Tier | Meaning |
|---|---|
| **T1** | Can change position, death, timing or RNG on a path ranked play reaches. |
| **T2** | Can change state only off ranked paths (Play original, menus, unreachable rooms) or only visuals and sound. |
| **T3** | No player can observe it: only a probe column differs. |
| **T?** | Not judged yet. Recorded and revisited; never blocks. |

Judge a question in this order:

1. **Can a ranked run reach it at all?** Menu rooms, Play original, cutscenes outside the ranked start and end, and rooms a ranked path never enters make the item **F2** (or **F3** if invisible) by scope, however large the difference.
2. **If reached, can it change state?** x or y, a death or kill, the tick count or timer, an alarm, or an RNG draw makes it **F1**.
3. **If it only changes pixels or audio, it is F2.** A difference a player can see or hear in Beta blocks Beta; one they cannot is **F3** and is logged.
4. **If only a probe column differs, it is F3.** The port behaves as the player sees it; the trace comparison sees a value no player can.

**RNG is shared, so "visual" particles are often F1.** Blood, playerface and particle systems draw from the gameplay RNG (register N10). Adding or skipping a draw shifts every later draw, which can move an enemy or change a death. A particle rule is F1 unless the spec shows it draws from a separate stream or never draws. The standard ruleset's separate effects stream is what makes a new game's particles F2/F3; a port on the shared stream keeps them F1.

**Animation can be F1.** `image_index` picks the subimage, and the subimage can be the collision mask (R7, R7c). An animation question is F1 when a mask follows it, F2 when it only changes what Draw shows.

## The rule

- **F1 blocks Ranked.** The game's Ranked status waits until every F1 item is answered and proven (a trace case, a feel contract, or the spec's confirmed rule). Ranked is where positions, deaths and timing must match.
- **F2 blocks Beta only if visible.** A visible F2 difference (a wrong sprite, a missing sound, a menu that misbehaves) blocks Beta until fixed or accepted on the game's page. An invisible one is F3 and is logged.
- **F3 is recorded and never blocks.** It stays in the register and the game's deviation log so a later change does not lose it. It is free to redesign.
- **M2's done-when counts F1 only.** M2 closes on the F1 items; an F2 or F3 item does not keep the milestone open. The register keeps every row; closing M2 does not delete the tier.
- **F2 and F3 move to the work they affect.** An F2/F3 item moves to the affected game's port issue (games repo, M5) or to M5 in its own repo. It is not closed and its labels are not removed; only its milestone changes.
- **F? is never moved and never blocks.** It stays where it is until evidence lands, then gets a tier.

A tier is a judgement, not a verdict on quality: F2 and F3 items are still fidelity work, and an F1 answer can reveal an F2 follow-up.

## Where a tier is written

| Place | Form |
|---|---|
| Register page (`docs/architecture/sim/register-*.md`) | The **Impact** column on the row. Existing rows use T1–T3; they are the F aliases. |
| Issue | The first line of the triage comment: `Impact: F1 — <one line>` (T1–T3 are accepted aliases). |
| Deviation log (`docs/games/<game-id>.md`) | Tier on every entry in Kept quirks, Fixed bugs, Known differences and Open (see [games/README.md](../games/README.md)). |
| This page | The definitions and the rule, so a later tier change has one home. |

**Trace case names use the same prefix.** The register's "Settles it" column can name trace cases such as `T1`, `T2`, `T8` (for example [register-input.md](../architecture/sim/register-input.md)). That `T` is a trace name, not a tier. The **Impact** column is the tier; read the column header, not the letter.

## Changing a tier

A tier change is an ordinary edit. Record the date and the evidence that moved it (a trace case, a feel contract, a claim id, or a spec answer) in the issue comment and the deviation-log entry. Revisit every F?/T? when the evidence that settles it lands; the register row already names that evidence in its last two columns.

## The one-time triage

#784 applied the tiers once to the open M2 fidelity questions: each got an `Impact:` comment, clear T2/T3 items moved to their game's port issue or M5, and ambiguous items were left as `T?` for a later pass. The tiers stay in force after that pass: every new mechanic or fidelity issue gets an F tier when it is filed, and every register row keeps one.
