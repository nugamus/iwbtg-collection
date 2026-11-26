# `warp`

Room travel (mechanics `warp`, the roomChanger). Collision with either kid kind, when `on_collide` and any gate is open (see Gate): set the transition for the client, request the room (applied at tick row 15), then in the same handler destroy the kid and the bow (when `warp_x` and `warp_y` are both 0; the new room's `kid_start` makes a fresh kid) or move the kid (see Moving the kid), keeping his speed. Source: `crates/sim/src/builtins/warp.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `room_to` | room | required | Destination. |
| `warp_x`, `warp_y` | real | 0, 0 | Kid position in the new room; both 0 destroys the kid. |
| `transition` | whole number | 0 | `transition_kind` (21 is fade out and in); 0 sends nothing. |
| `transition_steps` | whole number | 0 | `transition_steps`. |
| `on_collide` | bool | true | The touch does something (warp sets `onCollide` in Create). |
| `visible` | bool | kind default | Create sets `visible`. |
| `zero_axis` | `set`, `keep` | `set` | `set`: x becomes `warp_x` and y becomes `warp_y`, a 0 included. `keep`: an axis whose target is 0 is not set. |
| `screen_shift` | real | 0 | With `zero_axis = "keep"`: a 0 axis moves by this many screens, 800 px in x and 608 px in y. A nonzero value with `zero_axis = "set"` is a load error. |
| `clear_rising` | bool | false | Before the move, a negative vspeed becomes 0. Not adjusted for reversed gravity. |
| `gate_bosses` | list of whole numbers, 0 to 15 | none | Boss flags the gate counts (`MechGlobals::bosses`, the flags `boss_gate`, `boss_item` and `item_viewer` use). One number is a list of one. |
| `gate_items` | list of whole numbers, 0 to 15 | none | Item flags the gate counts (`MechGlobals::items`, as `secret_item` and `item_viewer`). |
| `gate_need` | whole number | all listed | The gate is open while at least this many listed flags are set. |
| `gate_closed` | `hide`, `destroy` | `hide` | A closed gate: `hide` is invisible and its touch does nothing; `destroy` destroys the warp. Format-0 legacy alias (`keep-celebrating-for-xia`), moves in [#1269](https://github.com/nugamus/iwbtg-collection/issues/1269). |

**Moving the kid.** When `warp_x` or `warp_y` is not 0, the handler moves the touching kid, in this order: `clear_rising`, then x, then y. The kid stays the same instance (he is persistent), so he keeps his speed, his djump and the rest of his state.

| Game | Original | Params |
|---|---|---|
| Most (roomChanger: KID-A1, A2, A3, A5) | x = warpX, y = warpY | defaults |
| NY (eat roomChanger, KID-A4) | x = XX if XX is nonzero, y = YY if YY is nonzero; keeps speed and djump | `zero_axis = "keep"` |
| KAE (roomChanger family, KID-B) | vspeed < 0 becomes 0; x = warpX, or x + screens x 800 when warpX is 0; the same for y with 608 | `zero_axis = "keep"`, `screen_shift = screens`, `clear_rising = true` |

NY's kept djump needs no param: nothing in the warp touches the kid's state. KID-B and KID-A5 also create a kid at the target "if none exists". That cannot happen here (the handler runs for a touching kid), so there is no param for it.

**Gate.** With `gate_bosses` or `gate_items`, the warp gets a Step event. It starts closed (with `hide`, Create makes it invisible). Each Step counts the listed flags that are set and opens the gate when the count reaches `gate_need`. With `hide`, Step sets `visible` to open-or-not, and the touch acts only while the gate is open as of the last Step. With `destroy`, Step destroys a closed warp, which keeps its own `visible`. A flag set during a tick (a `boss_item` touch, say) opens the gate at the next Step, not in the same tick's collisions.

The Step timing is the spec's: warpToBoss, warpCount and objWarpfin all test the flags in their Step event (mechanics builtin.warp "Gated forms"; KID-C Warps). That a flag set later in a tick waits for the next Step follows from it (Inferred: the GM event order runs Step before collisions).

Load errors: an index outside 0 to 15, a flag listed twice, `gate_need` above the number of listed flags, `gate_need` or `gate_closed` without a list, and `visible` on a `hide` gate (the gate sets it).

| Game | Original | Params |
|---|---|---|
| LR | warpToBoss: Step turns on onCollide and visible once `global.boss[bossNumber]` is set | `gate_bosses = [bossNumber]` |
| KAE | warpCount: Step recounts the boss flags (`countBoss`) and/or item flags (`countItem`); visible and usable while the count reaches `total` | `gate_bosses` and/or `gate_items` = every flag, `gate_need = total` |
| XIA | objWarpfin: Step destroys it unless boss items 1 and 2 are both taken | `gate_bosses = [1, 2]`, `gate_closed = "destroy"` |

`gate_bosses` and `gate_items` pick the event set, so a placed instance that sets them gets a derived kind (`builtins::event_set_params`, #853).
