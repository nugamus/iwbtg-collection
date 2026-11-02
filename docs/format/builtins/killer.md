# `killer`

A generic killer (mechanics `spike`, `cherry`). Its class is `killer`, so the kid's "Collision with killer" kills; there is no kill code. Source: `crates/sim/src/builtins/killer.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `image_speed` | real | image default | Cherry: 1/15. |
| `hspeed`, `vspeed` | real | 0 | Create sets them. |
| `scale` | real | none | Sets both image scales (LR object698: 9). |
| `visible`, `depth` | bool, real | kind default | Create overrides. |
| `destroy_outside_room` | bool | false | Outside Room destroys it (the cherry). |

The bounce rule of the bouncing cherries (U-MECH-5) is tier 3 and not here.
