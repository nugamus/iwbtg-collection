# `killer`

A generic killer (mechanics `spike`, `cherry`). Its class is `killer`, so the kid's "Collision with killer" kills; there is no kill code. Source: `crates/sim/src/builtins/killer.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `image_speed` | real | image default | Cherry: 1/15. Format-0 legacy alias (`animus`), moves in [#1269](https://github.com/nugamus/iwbtg-collection/issues/1269). |
| `hspeed`, `vspeed` | real | 0 | Create sets them. |
| `scale` | real | none | Sets both image scales (LR object698: 9). Format-0 legacy alias (`little-runner`), moves in [#1269](https://github.com/nugamus/iwbtg-collection/issues/1269). |
| `visible`, `depth` | bool, real | kind default | Create overrides. |
| `destroy_outside_room` | bool | false | Outside Room destroys it (the cherry). |
| `bounce` | bool | false | Step runs the Bounce action, not precise, against solid objects (`move_bounce_solid(false)`; ANI boundwhite, research CLONE_SPEC "Bounce"): `hspeed` flips when (`x + hspeed`, `y`) is blocked, `vspeed` when (`x`, `y + vspeed`) is, both when only (`x + hspeed`, `y + vspeed`) is. It never moves the cherry; the tick's motion then moves it by the new speed, after gravity, so it reverses before touching. Format-0 legacy alias (`animus`), moves in [#1269](https://github.com/nugamus/iwbtg-collection/issues/1269). |
| `kill_outside_view` | bool | false | The kid dies on touching it only when the kid's origin is outside view 0 (strict: `x < view_x`, `x > view_x + view_w`, `y < view_y`, `y > view_y + view_h`; kaenbyou `borderKiller`, invisible). The kind then has no `killer` class (the kid's own killer collision would kill unconditionally) and kills in its own kid collision. On a placed instance it gives a derived kind (`killer[kill_outside_view=true]`, objects.md, Params that pick a class or an event set). |

`bounce` adds the kind's Step event, so a placed instance that sets it gets a derived kind (`killer[bounce=true]`, objects.md, Params that pick a class or an event set, #853). Bounces against all objects or precise bounces go through a script (`i.move_bounce_solid(precise)`, `i.move_bounce_all(precise)`).
