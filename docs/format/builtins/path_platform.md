# `path_platform`

Kaenbyou pathPlatform: a platform that follows a path. Source: `crates/sim/src/builtins/path_platform.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `path` | path index | required | The path (`path::start`; the runner advances it each tick). |
| `speed` | real | 0 | Path speed. |
| `scale` | real | 0 | `path_scale`, applied when not 0. |
| `path_end` | `stop`, `restart`, `continue`, `reverse` | `stop` | End action. |
| `start_on_touch` | bool | false | Touch mode: the path starts on the first kid contact at `speed + 0.0001`; else in Create. |
| `keep_moving_when_kid_absent` | bool | false | With no kid in the room, the platform leaves the path and keeps moving at the path speed. |

Carry (End Step): when two horizontal lines from `x` to `x + 31` at `y + 8 - 25 * yflag` and `y + 8 - 10 * yflag` both touch a kid and the path position changed this tick, the first kid moves by the platform's displacement this tick, with no solid check.

OPEN: the original y carry also subtracts the unscaled `lengthdir_y(path_speed, direction)` (kid/B.md); this uses the displacement alone. The line probe uses the bounding box (not specified). The four room23 platforms in kaenbyou never move as placed, so no trace checks the carry.
