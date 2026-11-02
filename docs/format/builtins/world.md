# `world`

The engine controller, placed in `[engine] boot`. It is not a GM port: it does what the engines' infra objects did, at the rank of the original world object. Source: `crates/sim/src/builtins/world.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `latch_cleared_by_world` | bool | false | At the end of each Step, two `keyboard_check_direct` reads (Left, Right), discarded. kaenbyou, eat-the-lemon, keep-celebrating-for-xia. |

What it does:

- **Create**: slot 1, deaths 0, time 0.
- **Room Start**: `room_speed` becomes the game's tick rate (50), after the file's speed was in force during the kid's Create.
- **Retry (R) and reset (F2)**: `restart::run_restart` and `run_reset`, at the phase `[engine.restart]` / `[engine.reset]` `trigger` names: the Key press event, or Step (`step_held`, `step_pressed`).
- **Boot-room load**: `restart::boot_step` every Step; it finishes a `game_restart_via_boot` retry on the last boot tick.

OPEN, not done yet: pause (P; xia deactivation), the mute flag, the family B timer, fixed real-time reads (R28), `skip_while_paused`.
