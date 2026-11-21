# `scripted`

The base of an object that has a `script` and no other `base` (1,712 placed instances in the nine games). No native behavior, no params and no state: `events::dispatch` runs the base handler (this one, which does nothing) and then the script (`script::on_event`). Source: `crates/sim/src/builtins/scripted.rs`.

**What the kind handles.** The loader adds it to the kind's event set after the definition is built (`merge_script_events` in `crates/format/src/load.rs`), from the compiled script's functions: `begin_step`, `step`, `end_step`, `late`, `alarm(n)`, `user(n)`, `outside_room`, `intersect_boundary`, `animation_end`, `room_start`, `room_end` and `moment_<name>()` (the object's [timeline](../timelines.md) supplies the name). A kind with another `base` and a script gets the union of both. `create()` always runs. A script that does not compile is a load error.

**`collides_with`** (object file key, a list of class names or package class names) names the classes whose touch runs `collide(other)`; each becomes a collision handler in list order, after the base's own. Without it `collide` never runs.

Defaults of the object's `params` table are read by the script as `this.<name>`.

**Loading.** Tests `a_scripted_kind_handles_what_its_script_defines_and_its_collides_with` and `a_scripted_object_runs_in_a_room` (a loaded package run through `Sim`, two runs hash alike) in `crates/format/tests/load_objects.rs`.

`moment_<name>()` is part of the event set when the script defines it and the object's `timeline` key names a timeline whose moment uses that call ([timelines](../timelines.md)). On a kind whose base is `trigger`, a kid collision (the built-in's own event) runs `trigger(name)` with the instance's room-file `name`; an unnamed trigger has no name to pass and keeps the ordinary `collide` mapping.
