# The simulation

Part of [Architecture](../architecture.md).

One page per area, so two PRs on different areas never edit the same file. A note goes on its area's page; a new area gets a new page plus one line here.

- [Module layout and purity](sim/layout.md)
- [World model](sim/world.md)
- [Tick pipeline](sim/tick.md)
- [Motion](sim/motion.md)
- [Number handling](sim/numbers.md)
- [Collision](sim/collision.md)
- [Rooms](sim/rooms.md)
- [The kid](sim/kid.md)
- [Built-ins](sim/builtins.md)
- [Scripts and the script API](sim/scripts.md)
- [Random numbers](sim/rng.md)
- [State hash](sim/hash.md)
- [Determinism and purity](sim/determinism.md)
- [Sound model](sim/sound.md)

## Register

The live UNKNOWN, Inferred and Conflict register, one page per area. The **Impact** column is the fidelity tier ([fidelity tiers](../plans/fidelity-tiers.md)); section 15 of the [sim core plan](../plans/sim-core.md) keeps the original intro as history.

- [Tick and events](sim/register-tick-and-events.md)
- [Motion](sim/register-motion.md)
- [Collision](sim/register-collision.md)
- [Numbers](sim/register-numbers.md)
- [Sound](sim/register-sound.md)
- [Input](sim/register-input.md)
- [Kid and mechanics](sim/register-kid-and-mechanics.md)

## Claim ids

The GM8 spec splits into one file per feature under `research/specs/engines/gm8/runner/` (`tick.md`, `motion.md`, `collision.md`, `numbers.md`, `rooms.md`, `sprites.md`, `input.md`, `sound.md`, `particles.md`), and every claim has a stable id (`TICK-001`, `COL-031`, ...) indexed by `research/specs/INDEX.md`. A sim page names its feature file at the top; the register pages keep the register id as the row key and carry the matching claim id in their **Claim** column. An id is never renumbered, so a citation stays valid across spec edits.

`docs/architecture/sim/claim-ids.txt` lists every claim id (ids only, no spec text) so CI can check citations without the private research checkout. `scripts/test_spec_claims.py` (run by the `changes` CI job with the other `scripts/test_*.py` files) fails on a claim or register id cited in `docs/architecture/sim/**` that does not exist, and on a register row whose **Claim** cell is neither a known claim id nor an explicit `—` with a reason. Regenerate the list after a spec split or an id change:

```
IWBTG_RESEARCH=../iwbtg-collection-research python3 -I scripts/test_spec_claims.py --write
```

The command reads `$IWBTG_RESEARCH/specs/INDEX.md` and records that checkout's commit in the list header. Run the check alone with `python3 -I scripts/test_spec_claims.py`; with `IWBTG_RESEARCH` set, `python3 -m unittest discover -s scripts -p 'test_spec_claims.py'` also compares the committed list with the research index.
