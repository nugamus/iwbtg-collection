# `spike`

Static killer (mechanics `spike`). Its class is `killer`; the kid's "Collision with killer" kills, so the spike has no kill code. The mask is the sprite's precise mask and `dir` picks the sprite, both data. Source: `crates/sim/src/builtins/spike.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `dir` | `up`, `down`, `left`, `right` | none | Direction. Needed by `auto_rotate`. |
| `size` | real (32 or 16) | 32 | Spike size, used by `auto_rotate`. |
| `visible` | bool | kind default | Create sets `visible`. |
| `depth` | real | kind default | Create sets `depth`. |
| `destroys_bullets` | bool | false | A collision with the kid's bullet destroys the bullet (eat-the-new-year). Read from the kind, since the handler is declared per kind. |
| `auto_rotate` | bool | false | Family B (kid/B.md autoSpikeSprite): Create sets `image_angle` from `dir` and shifts the spike. |

`auto_rotate` angles and shifts (x, y): size 32 up 0 (+16, +16), down 180 (+15, +15), left 90 (+16, +15), right 270 (+15, +16); size 16 up 0 (+8, +8), down 180 (+7, +7), left 90 (+8, +7), right 270 (+7, +8).

Tier 2 adds the trap parameters (`trap_capable`).
