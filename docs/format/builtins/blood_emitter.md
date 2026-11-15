# `blood_emitter`

Made by the kid's death. **Alarm plus Step**: Create sets alarm 0 to `life`, Alarm 0 destroys it, each Step makes the blood. The Step count follows from the tick order: made in a collision event its first Step is the next tick and it gets 19 Steps; made in a Key press event it also Steps on the creation tick and gets 20 (Confirmed (recorder), rely `num-rng-a.csv`, `t6-input-key-event-order-b.csv`). Source: `crates/sim/src/builtins/blood_emitter.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `mode` | `random_dir`, `angle_sweep`, `particles` | `random_dir` | How it makes blood (below). |
| `life` | real | 20 | Alarm 0 at Create (A3: 10). |
| `per_step` | whole | 40 (15 in `particles`) | Drops per Step; in `particles`, the stream per tick (1 or more). |
| `speed_max` | real | 6 | Speed is `random(speed_max)`. OPEN: the original also has a `random(8)` branch the spec does not name. |
| `chance` | real | 0.2 | `angle_sweep`: chance per angle. |
| `per_angle` | whole | 8 | `angle_sweep`: drops per chosen angle (A3 uses 10). |
| `flipped` | bool | false | The kid was flipped; passed to the blood. |
| `destroyed_by_restart` | bool | false | A Restart key press destroys it (kaenbyou, eat-the-lemon). |

- `random_dir`: per drop, the blood's Create (frame, gravity), then the emitter draws the direction `irandom(35) * 10` and the speed. Rely: 160 draws a Step.
- `angle_sweep` (A2, A3): for each angle 0, 10, ..., 360, with `chance` (one draw), `per_angle` drops with speed `random(speed_max)`. OPEN: the draw order is not in the spec.
- `particles` (family B): the client draws the particles (`BuiltinOutput::DeathParticles` each Step); the Step draws nothing. With `NumRules::particles_draw_rng` (N10, on in engine_rev 0) Create also builds the emitter's particle system in the sim (#766), as the original's Create does (kid/B.md Death): one type with size, speed (1 to 5) and direction (0 to 360) ranges and life 50 (4 draws), one emitter on a linear rectangle (2 draws) streaming `per_step`. Each tick's particle update then draws `per_step` x 6 (90), from the creation tick until alarm 0 (CLONE_SPEC numbers "Particle draw rules"; Confirmed (recorder): kaenbyou `num-part-rng-3-k1.csv` frames 45-50 replay bit for bit, test `particles_rng::blood_emitter::k1_seed_column_replays_bit_for_bit`). The original removes the emitter at alarm 0 and destroys the system on Restart and Room End; here alarm 0 destroys the instance and its Destroy destroys the system, which draws the same (the type has no step or death particles, so the particles left over draw nothing). Room End destroys the system too. A death in a collision or Key press event streams in 20 updates (the alarm phase comes first). `per_step` 0 is refused in this mode (a stream of 0 is not traced).
