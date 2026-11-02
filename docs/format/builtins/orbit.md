# `orbit`

Killers that move on a circle. Each Step an orbiting instance is placed at `x = center_x + r cos(a)`, `y = center_y - r sin(a)`, then `a += speed`. Source: `crates/sim/src/builtins/orbit.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `mode` | `self`, `spawner` | `self` | `self`: this instance orbits (little-runner object761, poronkusema objIronball). `spawner`: Create spawns `count` instances of `child` at angles `angle + full / count * i`, then destroys itself (poronkusema objMakeIronball; kaenbyou roundingCherry gives the same positions). |
| `center_x`, `center_y` | real | required | The center. |
| `radius` | real, not negative | the distance from the instance to the center at Create | |
| `angle` | real | the direction from the instance to the center at Create | Start angle in `unit`. The default mirrors the first position across the center: object761 at (1840, 912) around (1840, 976) starts at (1840, 1040) (TR-MECH-05, not run). |
| `speed` | real | 0 | Added to the angle each Step, in `unit`. For object761 the loader writes minus its `qspd`, which the original subtracts. |
| `unit` | `degrees`, `radians` | `degrees` | `radians` (objIronball) wraps the angle into 0 to 2 pi. |
| `count`, `child` | whole 1 to 4096; kind | 1; required | Spawner only. |
| `keep_spawner` | bool | false | Spawner only: stays after spawning. |
| `destroy_outside_room` | bool | false | object761 is destroyed outside the room. |

OPEN: the wrap of a negative radians angle (it adds 2 pi here); the last bit of the trig (U-MECH-6); the objIronring chain sprite is the client's.
