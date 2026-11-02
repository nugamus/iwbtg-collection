# `difficulty_warp`

The new-game warp (warpStart, objWarpStart, kid/A1.md "New game"). Collision with either kid kind: set `global.difficulty` to `dif`, delete the save, zero the death and time counters, destroy the kid and the bow, and request the new-game room (`StartPoint::NewGame` does the same from the client, CR2). Source: `crates/sim/src/builtins/difficulty_warp.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `dif` | whole number | required | The difficulty it sets. |
| `room_to` | room | the game's new-game room | Destination. |
| `transition`, `transition_steps` | whole numbers | 0 | As in [`warp`](warp.md). |

OPEN: eat-the-new-year picks the difficulty by the kid's x in selectroom; machinary never assigns one; kaenbyou's warpStart does `room_restart` when no save exists.
