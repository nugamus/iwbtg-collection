# `world`

The engine controller, placed in `[engine] boot`. It is not a GM port: it does what the engines' infra objects did, at the rank of the original world object. Source: `crates/sim/src/builtins/world.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `latch_cleared_by_world` | bool | false | At the end of each Step, two `keyboard_check_direct` reads (Left, Right), discarded. kaenbyou, eat-the-lemon, keep-celebrating-for-xia. |
| `pause` | `none`, `deactivate` | `none` | `deactivate` is xia's pause (below). |
| `pause_delay` | whole | 0 | Ticks after a pause toggle before the next press is accepted. |
| `mute` | bool | false | A Mute press flips `MechGlobals::music_muted` (`global.music`, true is muted). The Ctrl in Ctrl+M (wanna, xia) is the client's binding of the action. |
| `timer_per_tick` | whole | 0 | Added to the play time every Step while not paused (family B: 2, centiseconds at 50 ticks). |
| `timer_on_restart` | whole | 0 | Added to the play time on the retry, before the restart (family B: 2). |
| `clear_triggers_to` | whole | none | Room Start sets `global.trigger[0]` to `[n]` false (poronkusema: 10). |
| `clear_switches_on_room_end` | bool | false | Room End clears the switch array (go-the-spook-house). |

What it does:

- **Create**: slot 1, deaths 0, time 0.
- **Room Start**: `room_speed` becomes the game's tick rate (50), after the file's speed was in force during the kid's Create.
- **Retry (R) and reset (F2)**: `restart::run_restart` and `run_reset`, at the phase `[engine.restart]` / `[engine.reset]` `trigger` names: the Key press event, or Step (`step_held`, `step_pressed`). `step_pressed` with `skip_while_paused` skips the retry while paused (xia).
- **Boot-room load**: `restart::boot_step` every Step.
- **Pause** (`deactivate`): a Pause press in Step, once the delay counter has run out, deactivates every other active instance and remembers which; the next press reactivates exactly those and calls `io_clear` (held keys and the direct latch are dropped). The other 13 games pause with a modal message box, `keyboard_wait` (showdown) or a sleep loop (kaenbyou): they block the runner, so the client freezes the sim and the world does nothing (`none`). `world::is_paused` reads the state.
- **Real-time reads** (R28): the world reads no clock. `tick::real_time_fps` and `real_time_ms` give the fixed values for the games that display them.

OPEN (research#35): the pause-delay counter's start value and counting (the spec names a counter, not its values); the effect of kaenbyou and eat-the-lemon's cheat-detection check (`round(50 * time_span * fps / room_speed)` against 10); whether the B timer counts in menu rooms and while dead (here every Step); the sounds a mute or unpause stops (`sound::stop_all`, WP10, not implemented yet).
