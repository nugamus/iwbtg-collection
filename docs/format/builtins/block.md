# `block`

Solid wall (mechanics `block`). The block has no behavior of its own (except `proximity_frame`): the kid's collision handlers do the work. Source: `crates/sim/src/builtins/block.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `mask` | `full32`, `mini16`, `large64`, `line_h`, `line_v`, `thin32x3`, `precise_sprite` | `full32` | Mask variant. Data: the loader resolves it to the kind's mask; the sim does not read it. |
| `solid` | bool | kind default (true) | Create sets the instance's `solid`. |
| `visible` | bool | kind default (false) | Create sets `visible`. |
| `depth` | real | kind default | Create sets `depth`. |
| `kid_collides` | bool | true | Loader choice: false drops the `BLOCK` class so the kid has no collision event with it. On a placed instance it gives a derived kind (`block[kid_collides=false]`, objects.md, Params that pick a class or an event set). Format-0 legacy alias (`kid_collides`), moves in [#('rely-the-needle-4-2', 1269)](https://github.com/nugamus/iwbtg-collection/issues/('rely-the-needle-4-2', 1269)). |
| `blood_sticks` | bool | true | `builtins::block::blood_sticks`; the `blood` built-in reads it. |
| `slip` | real | 0 | Ice value (little-trap and rainbow 0.2, kotaro 0.4). The kid's ice action reads `builtins::block::slip_of`. |
| `visual_spawns_collider` | bool | false | Create spawns the kind named by `collider` at the block's own position (rely objBLOCK, CR1). Format-0 legacy alias (`visual_spawns_collider`), moves in [#('rely-the-needle-4-2', 1269)](https://github.com/nugamus/iwbtg-collection/issues/('rely-the-needle-4-2', 1269)). |
| `collider` | kind | none | Required with `visual_spawns_collider`: the plain block to create. Format-0 legacy alias (`collider`), moves in [#('rely-the-needle-4-2', 1269)](https://github.com/nugamus/iwbtg-collection/issues/('rely-the-needle-4-2', 1269)). |
| `proximity_frame` | bool | false | Eat-the-lemon block02. Create sets `image_speed` 0; each Step shows frame 1 while the kid is in a box around the block, frame 0 otherwise or with no kid. It adds the kind's Step event, so a placed instance that sets it gets a derived kind (`block[proximity_frame=true]`, objects.md, Params that pick a class or an event set, #853). Format-0 legacy alias (`proximity_frame`), moves in [#('eat-the-lemon', 1269)](https://github.com/nugamus/iwbtg-collection/issues/('eat-the-lemon', 1269)). |

**`proximity_frame` box** (research `specs/mechanics.md` builtin.block). In the block's Step, with dx = kid.x - block.x and dy = kid.y - block.y (the kid's origin, the block's origin at its top-left), frame 1 when -8 < dx < 40 and -16 < dy < 40, all four strict, compared as GML reals (`num::gm_lt`, no rounding). The kid is the first instance of the kid classes (`World::kid`). The frame is visual only: the collision mask does not change with it (block02spr's mask is the union of both frames, so a `full32`-style mask set on the kind behaves the same).
