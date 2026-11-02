# `gravity_flip`

Source: `crates/sim/src/builtins/gravity_flip.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `direction` | `up`, `down` | required | `up` (KOT obj_ue, FT grav_up): a touch by the normal kid sets `global.grav`. `down` (obj_sita, grav_down): a touch by the flipped kid clears it. |
| `transition` | `swap_object`, `grav_sign`, `yflag_with_view_rotation` | `swap_object` | `swap_object` and `grav_sign` are the same write; the kid swaps object or flips on his next Step (kid code). `yflag_with_view_rotation` (KAE) is refused at load (OPEN: view rotation 12 degrees per tick, then the kid `yflag` flip). |
| `sound` | sound | none | FT restarts sndBlockChange; the client's. |

**Loading.** `direction` is required (an instance without it is an error), all params load on an object file or a placed instance, and `transition = "yflag_with_view_rotation"` is refused at load. Tests `wp6b3_params_*` and `a_gravity_flip_with_view_rotation_is_refused_at_load` in `crates/format/tests/wp7a_load.rs`. Converter request CR17.

Destroyed outside the room.
