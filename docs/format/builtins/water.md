# `water`

Collision with either kid (one event per kid). In this order: `fall_cap`, then `refresh`, then `air_jump`. Source: `crates/sim/src/builtins/water.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `variant` | 1, 2, 3 | 1 | Picks the class `water1`, `water2` or `water3` (the loader, `objects.rs`). |
| `fall_cap` | real | 2 | A downward speed above the cap is set to it. For the flipped kid "down" is up (speed times gravity sign). -1 is no cap (LR tipDJump, RBF object945). |
| `refresh` | `none`, `on_touch` | `none` | `on_touch` refreshes the air jump (`kid::api::refresh_air_jump`). |
| `air_jump` | `none`, `press_if_no_djump`, `djump_ignoring_flag` | `none` | `press_if_no_djump`: Jump pressed this tick and no double jump left sets `vspeed` to minus the jump speed (LR tipWater, SHO mizu01, ANI mugenjump). `djump_ignoring_flag`: the same jump whether or not `djump` is set, then `djump` is cleared (JUN objwater2). |
| `cur_jumps` | 0 to 3 | none | Sets the kid's family B jump counter on touch (kaenbyou objWater2: 2). |
| `release_same_tick` | bool | false | When Jump was released this tick, runs the kid's variable-jump release now (`kid::api::run_jump_release`; WAN water, ANI mugenjump, RBF object945). |
| `air_jump_speed` | `fixed_7`, `kid_jump2` | `fixed_7` | The air jump speed: 7, or the kid's second jump speed. |
| `ground_jump` | bool | false | Accepted. The full jump from 1 px above the water is the kid's: it tests the class (`water1` allows it, `water2` and `water3` do not), set by the loader. |

OPEN, not done:
- NY's rectangle `area` (tests the kid's origin against a rectangle in Step) is not done.
- The double-jump sound is the client's.
