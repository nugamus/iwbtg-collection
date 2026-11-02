# `bow`

The Medium-mode ribbon, persistent and cosmetic. It copies the first kid's position, scales and alpha (`kid::api::bow_snapshot`) in its `follow` phase; the 1-tick lag the traces show comes from dispatch order. With no kid it keeps its place. Source: `crates/sim/src/builtins/bow.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `follow` | `step`, `begin_step`, `end_step` | `step` | Copy phase (wanna and A3: `begin_step`; eat-the-lemon, xia: `end_step`). |
| `medium_only` | bool | true | Destroyed unless the difficulty is 0. |
| `destroy_in_select` | bool | true | Destroyed in a Select-role room. |

The destroy check runs in Begin Step, Step and End Step (A3 does; the other games only name Step). OPEN: the flipped kid's `y + 1` (xia) and the lemon sprite swap.
