# `drop_platform`

A platform that falls or rises when the kid stands on it. Source: `crates/sim/src/builtins/drop_platform.rs`. The kid is the first kid in the room.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `mode` | `fall`, `rise`, `rise_kae` | `fall` | See below. |
| `speed` | real | 2 | LR names it `spd`. |
| `solid` | bool | kind data | Applied in Create when given. |

- `fall` (KOT blockFall): each Step, when the kid is met at `(x, y - kid.vspeed - 1)`, the platform's `vspeed` becomes `speed`.
- `rise` (KOT blockRise): under the same test `yspeed = -speed` and the kid's `y` moves by `-speed`. Every Step `y += yspeed`, so it keeps rising after the kid leaves.
- `rise_kae` (KAE blockRise, a solid block): Step: when `yspeed` is 0, the kid is falling (`vspeed > 0`) and met at `(x, y + vsp - 3)`, `yspeed = vsp` (the kid's vspeed). End Step: when the kid is met at `(x, y + vsp - 3)`, the kid's `y += yspeed`; then the block's `y += yspeed`.

OPEN: the flipped kid (the specs test one kid object only); the spec text "vspeed becomes 2" for `fall` is read as the platform's own `vspeed`.
