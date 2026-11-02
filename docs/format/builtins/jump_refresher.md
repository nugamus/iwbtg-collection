# `jump_refresher`

Showdown moreJumping and moreJumping2. A kid touching it gets the air jump back (`kid::api::refresh_air_jump`), and it hides at (0, 0) for `hide_ticks`, then returns to the position it was created at (Alarm 0). Source: `crates/sim/src/builtins/jump_refresher.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `hide_ticks` | real, at least 1 | 50 | Ticks it stays hidden (the room speed in showdown). |
| `tjump` | `none`, `set`, `clear` | `none` | The triple-jump flag the original also sets (moreJumping clears it, moreJumping2 sets it). Only `none` loads. |

OPEN, not done: `tjump` needs a setter in `kid::api` (kid lane, #309), so `set` and `clear` are refused at load. The sound is the client's.
