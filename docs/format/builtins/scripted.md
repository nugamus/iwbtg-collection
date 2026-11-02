# `scripted`

The base of an object that has a `script` and no other `base` (1,712 placed instances in the nine games). No native behavior, no params and no state: `events::dispatch` runs the base handler (this one, which does nothing) and then the script (`script::on_event`). Source: `crates/sim/src/builtins/scripted.rs`.

**What the kind handles.** The loader adds it to the kind's event set after the definition is built (`merge_script_events` in `crates/format/src/load.rs`), from the compiled script's functions: `begin_step`, `step`, `end_step`, `late`, `alarm(n)`, `user(n)`, `outside_room`, `intersect_boundary`, `animation_end`, `room_start`, `room_end`. A kind with another `base` and a script gets the union of both. `create()` always runs. A script that does not compile is a load error.

**`collides_with`** (object file key, a list of class names or package class names) names the classes whose touch runs `collide(other)`; each becomes a collision handler in list order, after the base's own. Without it `collide` never runs.

Defaults of the object's `params` table are read by the script as `this.<name>`.

**Loading.** Tests `a_scripted_kind_handles_what_its_script_defines_and_its_collides_with` and `a_scripted_object_runs_in_a_room` (a loaded package run through `Sim`, two runs hash alike) in `crates/format/tests/wp7a_load.rs`.

OPEN (#371): the `trigger(name)` and `moment_<name>()` handlers are not part of the event set yet (timelines are WP8a).
