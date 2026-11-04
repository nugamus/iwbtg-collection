# `gravity_flip`

Source: `crates/sim/src/builtins/gravity_flip.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `direction` | `up`, `down` | required | `up` (KOT obj_ue, FT grav_up): a touch by the normal kid sets `global.grav`. `down` (obj_sita, grav_down): a touch by the flipped kid clears it. |
| `transition` | `swap_object`, `grav_sign`, `yflag_with_view_rotation` | `swap_object` | `swap_object` and `grav_sign` are the same write; the kid swaps object or flips on his next Step (kid code). `yflag_with_view_rotation` (KAE): see below. |
| `sound` | sound | none | FT restarts sndBlockChange; the client's. |

**Loading.** `direction` is required (an instance without it is an error), all params load on an object file or a placed instance, and Tests `wp6b3_params_*` and `a_gravity_flip_with_view_rotation_loads` in `crates/format/tests/wp7a_load.rs`. Converter request CR17.

Destroyed outside the room.

**`yflag_with_view_rotation` (kaenbyou objGravUp, objGravDown; kid/B.md "Gravity flip (kaenbyou)").** `up` acts while the kid is not reversed, `down` while reversed. Every tick of contact sets `global.frozen2`; the first touch of an activation arms Alarm 0 (1 tick). Alarm 0 adds 12 degrees to view 0's angle and re-arms itself while the angle is below 180 (`down`: below 360 and not 0). The first Alarm 0 past that (`down`: it sets the angle to 0) arms Alarm 1, which runs in the same alarm pass: `playerReverse` on every kid, `frozen2` cleared, the activation ends. So the flip is 15 turning ticks after the touch plus the arming tick (famb-gravflip: touch f39, angle 180 on f54, flip on f55; kaenbyou trace check OK with the preset carrying the param). The sound is the client's. Tests: `kaenbyou_arrow_rotates_the_view_then_flips_the_kid` in `crates/sim/tests/wp6b_drop_flip_edge_blocker_slope.rs`. OPEN: the converter does not write the param yet.
