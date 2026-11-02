# `kid_bullet`

The kid's bullet (kid/A1.md Shooting and the per-game notes). Create arms alarm 0 with `life`; Alarm 0 destroys the bullet. The shooter (the kid's shoot action) sets `hspeed` (16 times the facing) after Create. Source: `crates/sim/src/builtins/kid_bullet.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `life` | real | 40 | Alarm 0 at Create. |
| `block_check` | `before_move`, `collision_destroy`, `stop_and_die` | `before_move` | How a block stops it (below). |

- `before_move` (rely, A1): Step looks at `(x + hspeed, y)` before moving. A plain block there destroys the bullet; a platform (class `platform`) only when visible.
- `collision_destroy` (RBF, FT, RORI): a Collision with class `block` destroys it, so invisible blocks and platforms stop it too. No Step check.
- `stop_and_die` (poronkusema): when a block lies at `(x + hspeed, y)`, the bullet moves pixel by pixel up to contact, sets `hspeed` to 0 and is destroyed on its next Step. OPEN: the contact-move rule at fractional positions.

The bullet limit (fewer than 4 existing) is the kid's shoot action.
