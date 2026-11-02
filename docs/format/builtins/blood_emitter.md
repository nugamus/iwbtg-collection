# `blood_emitter`

Made by the kid's death. **Alarm plus Step**: Create sets alarm 0 to `life`, Alarm 0 destroys it, each Step makes the blood. The Step count follows from the tick order: made in a collision event its first Step is the next tick and it gets 19 Steps; made in a Key press event it also Steps on the creation tick and gets 20 (Confirmed (recorder), rely `num-rng-a.csv`, `t6-input-key-event-order-b.csv`). Source: `crates/sim/src/builtins/blood_emitter.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `mode` | `random_dir`, `angle_sweep`, `particles` | `random_dir` | How it makes blood (below). |
| `life` | real | 20 | Alarm 0 at Create (A3: 10). |
| `per_step` | whole | 40 (15 in `particles`) | Drops per Step. |
| `speed_max` | real | 6 | Speed is `random(speed_max)`. OPEN: the original also has a `random(8)` branch the spec does not name. |
| `chance` | real | 0.2 | `angle_sweep`: chance per angle. |
| `per_angle` | whole | 8 | `angle_sweep`: drops per chosen angle (A3 uses 10). |
| `flipped` | bool | false | The kid was flipped; passed to the blood. |
| `destroyed_by_restart` | bool | false | A Restart key press destroys it (kaenbyou, eat-the-lemon). |

- `random_dir`: per drop, the blood's Create (frame, gravity), then the emitter draws the direction `irandom(35) * 10` and the speed. Rely: 160 draws a Step.
- `angle_sweep` (A2, A3): for each angle 0, 10, ..., 360, with `chance` (one draw), `per_angle` drops with speed `random(speed_max)`. OPEN: the draw order is not in the spec.
- `particles` (family B): client output only (`BuiltinOutput::DeathParticles` each Step), no sim draws. Whether particle systems draw from the RNG is UNKNOWN (N10); with `NumRules::particles_draw_rng` it is `Unsupported`.
