# `world`

The engine controller, placed in `[engine] boot`. It is not a GM port: it does what the engines' infra objects did, at the rank of the original world object. Source: `crates/sim/src/builtins/world.rs`.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `latch_cleared_by_world` | bool | false | At the end of each Step, two `keyboard_check_direct` reads (Left, Right), discarded. kaenbyou, eat-the-lemon, keep-celebrating-for-xia. |
| `pause` | `none`, `deactivate` | `none` | `deactivate` is xia's pause (below). |
| `pause_delay` | whole | 0 | Ticks after a pause toggle before the next press is accepted. |
| `mute` | bool | false | A Mute press flips `MechGlobals::music_muted` (`global.music`, true is muted). The Ctrl in Ctrl+M (wanna, xia) is the client's binding of the action. |
| `timer_per_tick` | whole | 0 | Added to the play time every Step while not paused and the gates below are open (family B: 2, centiseconds at 50 ticks). |
| `timer_play_rooms_only` | bool | false | No per-tick add in rooms whose role is not `play` (boot, title, menu, select). Family B: true. |
| `timer_needs_fps` | bool | false | No per-tick add while GML `fps` reads 0 (see Real-time reads). Family B: true. |
| `timer_stops_on_clear` | bool | false | No per-tick add while the game is cleared (`RunGlobals::clear`, `global.clear`). Family B: true. |
| `clear_triggers_to` | whole | none | Room Start sets `global.trigger[0]` to `[n]` false (poronkusema: 10). |
| `clear_switches_on_room_end` | bool | false | Room End clears the switch array (go-the-spook-house). |

**Loading.** The params go on the game's world object file (`base = "world"`) that `[engine.roles] world = "@<id>"` names, and are checked like any built-in's. Tests `wp6b2_params_load` and `wp6b2_params_are_checked` in `crates/format/tests/wp7a_load.rs`. The converter request for them is on research#31.

What it does:

- **Create**: slot 1, deaths 0, time 0.
- **Room Start**: `room_speed` becomes the game's tick rate (50), after the file's speed was in force during the kid's Create.
- **Retry (R) and reset (F2)**: `restart::run_restart` and `run_reset`, at the phase `[engine.restart]` / `[engine.reset]` `trigger` names: the Key press event, or Step (`step_held`, `step_pressed`). `step_pressed` with `skip_while_paused` skips the retry while paused (xia).
- **Boot-room load**: `restart::boot_step` every Step.
- **Pause** (`deactivate`): a Pause press in Step, once the delay counter has run out, deactivates every other active instance and remembers which; the next press reactivates exactly those and calls `io_clear` (held keys and the direct latch are dropped). The other 13 games pause with a modal message box, `keyboard_wait` (showdown) or a sleep loop (kaenbyou): they block the runner, so the client freezes the sim and the world does nothing (`none`). `world::is_paused` reads the state.
- **Play timer** (family B, research kid/B.md "Play timer"): Step adds `timer_per_tick` when not paused and each gate the params turn on is open. There is no kid check: it counts while the kid is dead and during the game over. The retry's +2 is `[engine.restart] kid_steps.add_time = 2`, not a world param (the earlier `timer_on_restart` is gone, so a converter cannot write both). R's room change ends the tick before world Step (AbortTick), so the R tick adds 2, not 4.
- **Real-time reads** (R28): the world reads no clock. `tick::real_time_fps` and `real_time_ms` give the fixed values for the games that display them. `fps` is the tick rate, except that it reads 0 while `World::tick` is below `TickRules::fps_zero_ticks` (`tick::fps_reads_zero`). Normal play leaves that at 0, so `fps` is never 0 there (Design: the real GM8 runner's first non-zero frame is UNKNOWN, and real play spends over a second in the boot and menu rooms). `iwbtg trace run` and `check` set it to `51 - at`, because the recorder's `fps` reads 0 up to frame 50 (Confirmed (recorder): research traces/kaenbyou/famb-timer-fps-gate-r.phases.csv).

OPEN (research#35): the pause-delay counter's start value and counting (the spec names a counter, not its values); the effect of kaenbyou and eat-the-lemon's cheat-detection check (`round(50 * time_span * fps / room_speed)` against 10); the sounds a mute or unpause stops (`sound::stop_all`, WP10, not implemented yet).
