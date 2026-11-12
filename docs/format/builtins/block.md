# `block`

Solid wall (mechanics `block`). The block has no behavior of its own: the kid's collision handlers do the work. Source: `crates/sim/src/builtins/block.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `mask` | `full32`, `mini16`, `large64`, `line_h`, `line_v`, `thin32x3`, `precise_sprite` | `full32` | Mask variant. Data: the loader resolves it to the kind's mask; the sim does not read it. |
| `solid` | bool | kind default (true) | Create sets the instance's `solid`. |
| `visible` | bool | kind default (false) | Create sets `visible`. |
| `depth` | real | kind default | Create sets `depth`. |
| `kid_collides` | bool | true | Loader choice: false drops the `BLOCK` class so the kid has no collision event with it. On a placed instance it gives a derived kind (`block[kid_collides=false]`, v0.md Params that pick a class). |
| `blood_sticks` | bool | true | `builtins::block::blood_sticks`; the `blood` built-in reads it. |
| `slip` | real | 0 | Ice value (little-trap and rainbow 0.2, kotaro 0.4). The kid's ice action reads `builtins::block::slip_of`. |
| `visual_spawns_collider` | bool | false | Create spawns the kind named by `collider` at the block's own position (rely objBLOCK, CR1). |
| `collider` | kind | none | Required with `visual_spawns_collider`: the plain block to create. |

OPEN: `proximity_frame` (eat-the-lemon block02: frame 1 when the kid is in a box around it) is not supported; the spec does not give the box.
