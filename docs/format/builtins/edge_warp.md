# `edge_warp`

Leaving the room through an edge (outsideroomChanger, NY roomout, SPK oRoomSwitch). The kid code reads it at Intersect Boundary (class `EDGE_WARP`). Source: `crates/sim/src/builtins/edge_warp.rs`. Give exactly one form.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `room_to`, `warp_x`, `warp_y` | room, real, real | | One target. Kept in a `WarpState` (`BuiltinState::Warp`), the state a `warp` has, so the kid reads `room_to`, `warp_x`, `warp_y` from it (#250). |
| `left_room`, `right_room`, `up_room`, `down_room` | room | none | The per-edge table (NY roomout h, m, u, s, kid/A4.md). `EdgeTable::target(edge)` answers for the crossed edge: left when `x < 0`, right when `x > room_width`, up when `y < 0`, down when `y > room_height`, in that order. An unset edge has no target (the kid kills; at the top he does nothing). The kid keeps its position and speeds. |
| `mode` | `boundary`, `room_assign` | `boundary` | `room_assign` (SPK oRoomSwitch, K10): a kid touch queues the room change like `room_goto` and puts the kid at `warp_x`, and at `warp_y` when it is not 0, keeping his speeds (Confirmed (recorder), trace SH-roomswitch). With no `room_to` it does nothing (the 24 switches without a table entry). The converter resolves the original table (it knows the room and side of each placed instance) and writes `room_to`, `warp_x` and `warp_y` on each placed instance, or nothing for the switches without an entry; the loader resolves the room name and checks the params (test `wp6b3_params_load_on_kinds_and_per_instance`; converter request CR17). Returns `AbortTick` as `warp` does (Inferred: the room-change tick skips one End Step). |

The kid reads `EdgeTable` at Intersect Boundary (`kid/boundary.rs`): a table instance counts room-wide (NY's roomout need not touch the kid), the first edge crossed in the order above names the room (a room change request, `AbortTick`), an unset edge kills and the top does nothing. The usual boundary wrap still runs on the surviving kid.
