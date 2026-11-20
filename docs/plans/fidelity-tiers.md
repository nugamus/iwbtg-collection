# Fidelity impact tiers

Every open fidelity question gets an impact tier. The tier says whether the answer can change a ranked run, so the project can stop treating a one-game visual detail as equal to a physics rule. This page defines the tiers and the blocking rule; [content.md](../content.md#porting) (Porting, Fidelity) is the principle they apply.

> **Fidelity.** Quirks that players rely on are never fixed. Bugs that nobody wants (crashes, softlocks, broken graphics, typos) are fixed when a trace shows that positions do not change. Every fix is logged in `docs/games/<game-id>.md`.

That rule asks one question of every quirk and every fix: does it change what a ranked player can do? The tiers record the answer once, in a place a porter and a reviewer can read.

## The tiers

| Tier | Meaning |
|---|---|
| **T1** | Can change position, death, timing or RNG on a path ranked play reaches. |
| **T2** | Can change state only off ranked paths (Play original, menus, unreachable rooms) or only visuals and sound. |
| **T3** | No player can observe it: only a probe column differs. |
| **T?** | Not judged yet. Recorded and revisited; never blocks. |

Judge a question in this order:

1. **Can a ranked run reach it at all?** Menu rooms, Play original, cutscenes outside the ranked start and end, and rooms a ranked path never enters make the item **T2** by scope, however large the difference.
2. **If reached, can it change state?** x or y, a death or kill, the tick count or timer, an alarm, or an RNG draw makes it **T1**.
3. **If it only changes pixels or audio, it is T2.** A difference a player can see or hear in Beta blocks Beta; one they cannot is logged.
4. **If only a probe column differs, it is T3.** The port behaves as the player sees it; the trace comparison sees a value no player can.

**RNG is shared, so "visual" particles are often T1.** Blood, playerface and particle systems draw from the gameplay RNG (register N10). Adding or skipping a draw shifts every later draw, which can move an enemy or change a death. A particle rule is T1 unless the spec shows it draws from a separate stream or never draws.

**Animation can be T1.** `image_index` picks the subimage, and the subimage can be the collision mask (R7, R7c). An animation question is T1 when a mask follows it, T2 when it only changes what Draw shows.

## The rule

- **T1 blocks Ranked.** The game's Ranked status waits until every T1 item is answered and proven (a trace case or the spec's confirmed rule). Ranked is where positions, deaths and timing must match.
- **T2 blocks Beta only if visible.** A visible T2 difference (a wrong sprite, a missing sound, a menu that misbehaves) blocks Beta until fixed or accepted on the game's page. An invisible one is logged and does not block.
- **T3 is recorded and never blocks.** It stays in the register and the game's deviation log so a later change does not lose it.
- **M2's done-when counts T1 only.** M2 closes on the T1 items; a T2 or T3 item does not keep the milestone open. The register keeps every row; closing M2 does not delete the tier.
- **T2 and T3 move to the work they affect.** A T2/T3 item moves to the affected game's port issue (games repo, M5) or to M5 in its own repo. It is not closed and its labels are not removed; only its milestone changes.
- **T? is never moved and never blocks.** It stays where it is until evidence lands, then gets a tier.

A tier is a judgement, not a verdict on quality: T2 and T3 items are still fidelity work, and a T1 answer can reveal a T2 follow-up.

## Where a tier is written

| Place | Form |
|---|---|
| Register page (`docs/architecture/sim/register-*.md`) | The **Impact** column on the row. |
| Issue | The first line of the triage comment: `Impact: T1 — <one line>`. |
| Deviation log (`docs/games/<game-id>.md`) | Tier on every entry in Kept quirks, Fixed bugs, Known differences and Open (see [games/README.md](../games/README.md)). |
| This page | The definitions and the rule, so a later tier change has one home. |

**Trace case names use the same prefix.** The register's "Settles it" column can name trace cases such as `T1`, `T2`, `T8` (for example [register-input.md](../architecture/sim/register-input.md)). That `T` is a trace name, not a tier. The **Impact** column is the tier; read the column header, not the letter.

## Changing a tier

A tier change is an ordinary edit. Record the date and the evidence that moved it (a trace case, a claim id, or a spec answer) in the issue comment and the deviation-log entry. Revisit every T? when the trace or spec answer that settles it lands; the register row already names that evidence in its last two columns.

## The one-time triage

#784 applied the tiers once to the open M2 fidelity questions: each got an `Impact:` comment, clear T2/T3 items moved to their game's port issue or M5, and ambiguous items were left as `T?` for a later pass. The tiers stay in force after that pass: every new fidelity issue gets a tier when it is filed, and every register row keeps one.
