# `trap`

Trigger-launched mover. Source: `crates/sim/src/builtins/trap.rs`. It launches once, when its trigger has fired and it is idle: `hspeed` and `vspeed` both 0, and (with `flag_guard`, a stop chain or a path) its own launch flag still 0.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `trigger_id` | real | 0 | The id it waits for. With `shared_last` and an unset trigger (reads 0) an id of 0 launches at once. Format-0 legacy alias (`go-the-spook-house`), moves in [#1267](https://github.com/nugamus/iwbtg-collection/issues/1267). |
| `trigger_model` | `shared_last`, `per_object`, `global_last`, `flag_array`, `switch_array` | `shared_last` | As `trigger`. Format-0 legacy alias (`go-the-spook-house`), moves in [#1267](https://github.com/nugamus/iwbtg-collection/issues/1267). |
| `trigger_kind`, `_b`, `_c` | kind | none | The trigger kinds read by `shared_last` and `per_object` (freetrigger, objswitch, freetriggerneedkey). |
| `launch` | `velocity`, `angle`, `path`, `move_to`, `step_toward` | `velocity` | The form. Format-0 legacy alias (`go-the-spook-house`), moves in [#1267](https://github.com/nugamus/iwbtg-collection/issues/1267). |
| `hspeed`, `vspeed` | real | 0 | `velocity` (the fixed-speed forms too). SHO spikeMoveUp: the loader folds `ue`, `sita`, `hidari`, `migi` into these. Format-0 legacy alias (`showdown`), moves in [#1267](https://github.com/nugamus/iwbtg-collection/issues/1267). |
| `dir`, `speed` | real | 0 | `angle`: `motion_set(dir, speed)` (NY object321, object322). |
| `path`, `path_speed`, `path_end`, `path_absolute`, `path_scale` | | | `path`: Step sets alarm 0 to 1, Alarm 0 starts the path and sets the flag. `path` is the path index (loader). `path_end`: `stop`, `restart`, `continue`, `reverse`. `path_scale` applies when not 0. |
| `endx`, `endy`, `hspd`, `vspd` | real | 0 | `move_to` (LR object749, object751-755): runs only while `endx` or `endy` is 0 and `global.stop[trg]` is set. Per axis with a target it moves by the step toward it, then within half a step snaps to the target, zeroes the step and replaces the target with 1. Format-0 legacy alias (`little-runner`), moves in [#1267](https://github.com/nugamus/iwbtg-collection/issues/1267). |
| `to_x`, `to_y` | real | 0 | `step_toward` (go-the-spook-house playerKiller switch spikes, with `trigger_model = switch_array`): Create sets a target of 0 to the instance's own x (y). End Step, while the trigger fires, moves 1 px toward (`to_x`, `to_y`): x first, y only once x equals `to_x`, never both in one tick. No launch flag and no idle test; when the switch goes off it stops where it is. The kind handles End Step instead of Step. Format-0 legacy alias (`go-the-spook-house`), moves in [#1267](https://github.com/nugamus/iwbtg-collection/issues/1267). |
| `alm`, `alm2`, `alm3`, `alm4`, `h2`, `v2`, `h3`, `v3` | real | 0 | The stop chain (below); `velocity` launch only. |
| `flag_guard` | bool | false | Needs its own flag 0 to launch (FT object882, RBF). |
| `stop_on_stopper` | `none`, `zero_speed`, `flush` | `none` | Touching a `trap_stopper` stops it; `flush` first snaps it to the stopper edge on the side it moves toward, horizontal first (RBF object1024). Format-0 legacy alias (`get-the-rainbow-feather`), moves in [#1267](https://github.com/nugamus/iwbtg-collection/issues/1267). |
| `depth_reveal` | bool | false | Create sets depth 1000002; once launched and more than 32 px from the start in x or y, depth becomes 0. |
| `destroy_outside_room` | bool | false | |
| `kind`, `sound`, `sound_if_trigger` | | | The class the loader gives it; sound is the client's. |

**Loading.** Every param loads on an object file or a placed instance. `trigger_kind`, `trigger_kind_b` and `trigger_kind_c` name kinds and must exist; `path` is a path index (the position of `paths/<id>.toml` in file path order), written as a number. Tests `wp6b2_params_load` and `wp6b2_params_are_checked` in `crates/format/tests/load_objects.rs`. Converter request CR17.

## Stop chain

Launch sets flag 1, then arms Alarm 0 to `alm` (only when above 0) and sets flag 2. Alarm 0 stops it and arms Alarm 1 to `alm2`. Alarm 1 relaunches with `h2`, `v2` when idle, flag 2 and the trigger still matches (flag 3), then arms Alarm 2 to `alm3` (flag 4). Alarm 2 stops it and arms Alarm 3 to `alm4`. Alarm 3 relaunches with `h3`, `v3` when idle, flag 4 and the trigger still matches (flag 5). If the trigger no longer matches the chain ends.

OPEN (nugamus/iwbtg-collection-research#49): kaenbyou pathFree (End Step converts a running path to straight motion when no kid exists); the path start tick (U-MECH-4, TR-MECH-02) and stop chain timing (U-MECH-8, TR-MECH-03) follow the generic alarm and path rules until traces answer them; the order of the move-to snap test against the move is the spec text read literally. `step_toward` moves a full 1 px even when the target is less than 1 px away, and its timing is the spec text until TR-MECH-11 is run.
