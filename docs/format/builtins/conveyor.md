# `conveyor`

A solid block child that pushes the kid sideways. The loader gives it the `conveyor` class and the block's solidity. Source: `crates/sim/src/builtins/conveyor.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `h` | real | required | The horizontal speed. |
| `mode` | `add_each_tick`, `set_on_top_edge` | `add_each_tick` | `add_each_tick` (rainbow-feather object830 and object831, h -2 and 2): `h` is added to a touching kid's `hspeed` on every collision tick. `set_on_top_edge` (showdown BlockHidari and BlockMigi, h -4 and 4): in Step, a kid touching the line from (x + 32, y - 1) to (x, y) gets `hspeed = h`. |

OPEN: whether `hspeed` accumulates or resets each Step (TR-MECH-06, not run); showdown does the same to pushBlock, which is not a built-in; the line probe uses the bounding box (`prec` false; the spec does not say).
