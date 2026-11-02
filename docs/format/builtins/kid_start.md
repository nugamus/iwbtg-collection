# `kid_start`

The player start (mechanics `player_start`). Room Start creates the kid when no kid (either kind, alive and active) exists. The kid's Create runs inside it and sees the room's file `room_speed` (R14). Source: `crates/sim/src/builtins/kid_start.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `offset_x`, `offset_y` | real | 17, 23 | Spawn offset (machinary 16, 23). |
| `flipped_offset_x`, `flipped_offset_y` | real | `offset_y`, `offset_x` (23, 17) | Offset of the flipped kid. |
| `creates` | `player`, `player_or_player2` | `player` | `player_or_player2` (animus): `global.grav` 1 creates the flipped kid (engine role `kid_flipped`). Needs `shift_at = "room_start"`. |
| `grav_gated` | bool | true | With `creates = "player"`: create nothing while `global.grav` is 1 (A1, A2). False for RORI, showdown, A3/A4. |
| `shift_at` | `create`, `room_start` | `create` | Where the offset is added: Create moves the start itself (rely), or Room Start adds it to the new kid's position. |
