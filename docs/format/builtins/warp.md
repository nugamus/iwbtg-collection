# `warp`

Room travel (mechanics `warp`, the roomChanger). Collision with either kid kind, when `on_collide`: set the transition for the client, request the room (applied at tick row 15), then in the same handler destroy the kid and the bow (when `warp_x` and `warp_y` are both 0; the new room's `kid_start` makes a fresh kid) or move the kid (see Moving the kid), keeping his speed. Source: `crates/sim/src/builtins/warp.rs`.

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

**Moving the kid.** When `warp_x` or `warp_y` is not 0, the handler moves the touching kid, in this order: `clear_rising`, then x, then y. The kid stays the same instance (he is persistent), so he keeps his speed, his djump and the rest of his state.

| Game | Original | Params |
|---|---|---|
| Most (roomChanger: KID-A1, A2, A3, A5) | x = warpX, y = warpY | defaults |
| NY (eat roomChanger, KID-A4) | x = XX if XX is nonzero, y = YY if YY is nonzero; keeps speed and djump | `zero_axis = "keep"` |
| KAE (roomChanger family, KID-B) | vspeed < 0 becomes 0; x = warpX, or x + screens x 800 when warpX is 0; the same for y with 608 | `zero_axis = "keep"`, `screen_shift = screens`, `clear_rising = true` |

NY's kept djump needs no param: nothing in the warp touches the kid's state. KID-B and KID-A5 also create a kid at the target "if none exists". That cannot happen here (the handler runs for a touching kid), so there is no param for it.

OPEN, not done: `gate` (LR warpToBoss, KAE warpCount, XIA objWarpfin need the boss and item flags). It waits for list params (#867).
