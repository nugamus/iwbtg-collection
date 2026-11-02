# `trigger`

Touch zone that fires a trap id. Source: `crates/sim/src/builtins/trigger.rs`. A touch by either kid does, per `model`:

| Model | Games | Touch |
|---|---|---|
| `shared_last` | KOT, FED, LT, NY, ANI, RBF | If this instance's `triggered` differs from `trg`, writes `trg` into `triggered` of every instance of its kind (and of the `group` kind). Traps read the first instance. `triggered` starts as 0. |
| `per_object` | KOT, LT, RBF | `triggered = 1` on every instance of its kind (and of `group`). |
| `global_last` | KAE | `global.triggered = trg`, then destroys itself. Create resets `global.triggered` to 0, except with `need_key`: that variant fires only while `global.triggered` equals the key. |
| `flag_array` | WAN, LR, POR, FT, SHO | `global.trigger[trg]` set (`array = "stop"`: `global.stop[trg]`); `reset` clears it instead (POR objTriggerReset); `need_key` fires only while `global.trigger[key]` is set. |
| `switch_array` | SPK | `global.switchTrigger[trg]` set. |

| Param | Type | Default | Meaning |
|---|---|---|---|
| `trg` | real | 0 | The id. |
| `model` | the words above | `shared_last` | |
| `need_key` | real | none | `global_last` and `flag_array` only. |
| `group` | kind | none | Another kind sharing `triggered` (an object and its children). |
| `array` | `trigger`, `stop` | `trigger` | `flag_array` only. |
| `reset` | bool | false | `flag_array` clears instead of sets. |
| `one_shot_destroy`, `sets_frame` | bool | false | Destroy after touch; show frame 1 (SHO). |
| `sound`, `size` | | | The client's and the loader's. |

State: `MechGlobals` (`global_triggered`, `trigger_flags`, `stop_flags`, `switches`) and the instances' `triggered`. The sim hash covers it.

OPEN: the exact read and write semantics of `triggered` (U-MECH-2; this is "all instances written, first read"); whether flag-array triggers survive R in wanna, little-runner and help-the-fairy-tale (TR-MECH-08, not run). Clearing: the `world` params `clear_triggers_to` (POR, Room Start ids 0-10) and `clear_switches_on_room_end` (SPK); `game_restart` clears every global, `MechGlobals` included, unless `globals_survive_restart` (Inferred, U-MECH-3). SHO roomChanger and loadGame clear the trigger globals too (which ones is OPEN; those live in `warp` and the save load, not here).
