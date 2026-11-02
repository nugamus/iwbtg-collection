# `game_over`

The GAMEOVER display, created by the kid's death. Source: `crates/sim/src/builtins/game_over.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `offset_x`, `offset_y` | real | 400, 304 | Offset from view 0 (rely 400, 304; POR 480, 270; A2 and B 400, 300). |
| `view_relative` | bool | true | Create places it at view 0 plus the offset. |
| `delay` | real | 0 | Hidden for this many ticks (A2 and B: 30), shown by Alarm 0. |

The fades run in the Draw slot's `Late` handler, so only while visible: `image_alpha` +0.01 a tick up to 1, the black overlay alpha +0.015 up to 0.6 (`game_over::overlay_alpha`), and the bob phase +pi/100 (`game_over::bob_offset` is `32 * sin(phase)`). OPEN: whether the original updates them in Step or Draw.
