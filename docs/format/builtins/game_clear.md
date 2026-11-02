# `game_clear`

Ends the run. Source: `crates/sim/src/builtins/game_clear.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `save_with_position` | bool | true | `true`: `save::save_now` from the touching kid. `false`: counters and the clear flag only, into the existing slot (nothing when there is none). |
| `saved_position`, `fixed_x`, `fixed_y` | | as `save` | The game's position model, same words as the `save` built-in. |

A touch by either kid sets `global.clear`, runs the save, then raises the `run_end` timing trigger (`timing::raise_run_end`, read at tick row 16).

OPEN: which of the two save forms each of the nine games uses; the converter sets `save_with_position` per game.

**Loading.** Params load on an object file or a placed instance; a missing required param, an index above 15 and a wrong type are load errors (tests `wp6b4_params_*` in `crates/format/tests/wp7a_load.rs`). Converter request CR17.
