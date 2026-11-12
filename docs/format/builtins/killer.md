# `killer`

A generic killer (mechanics `spike`, `cherry`). Its class is `killer`, so the kid's "Collision with killer" kills; there is no kill code. Source: `crates/sim/src/builtins/killer.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `image_speed` | real | image default | Cherry: 1/15. |
| `hspeed`, `vspeed` | real | 0 | Create sets them. |
| `scale` | real | none | Sets both image scales (LR object698: 9). |
| `visible`, `depth` | bool, real | kind default | Create overrides. |
| `destroy_outside_room` | bool | false | Outside Room destroys it (the cherry). |
| `kill_outside_view` | bool | false | The kid dies on touching it only when the kid's origin is outside view 0 (strict: `x < view_x`, `x > view_x + view_w`, `y < view_y`, `y > view_y + view_h`; kaenbyou `borderKiller`, invisible). The kind then has no `killer` class (the kid's own killer collision would kill unconditionally) and kills in its own kid collision. On a placed instance it gives a derived kind (`killer[kill_outside_view=true]`, v0.md Params that pick a class). |

The bounce rule of the bouncing cherries (U-MECH-5) is tier 3 and not here.
