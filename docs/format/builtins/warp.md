# `warp`

Room travel (mechanics `warp`, the roomChanger). Collision with either kid kind, when `on_collide`: set the transition for the client, request the room (applied at tick row 15), then in the same handler destroy the kid and the bow (when `warp_x` and `warp_y` are both 0; the new room's `kid_start` makes a fresh kid) or move the kid to `(warp_x, warp_y)`, keeping his speed. Source: `crates/sim/src/builtins/warp.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `room_to` | room | required | Destination. |
| `warp_x`, `warp_y` | real | 0, 0 | Kid position in the new room; both 0 destroys the kid. |
| `transition` | whole number | 0 | `transition_kind` (21 is fade out and in); 0 sends nothing. |
| `transition_steps` | whole number | 0 | `transition_steps`. |
| `on_collide` | bool | true | The touch does something (warp sets `onCollide` in Create). |
| `visible` | bool | kind default | Create sets `visible`. |

OPEN, not done: `gate` (LR warpToBoss, KAE warpCount, XIA objWarpfin need the boss and item flags), KAE `screen_shift` and the negative-vspeed zeroing, NY's kept djump.
