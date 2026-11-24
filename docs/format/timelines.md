# Timelines: `timelines/<id>.toml`

Part of [format v0](v0.md).

```toml
#:schema ../../../client/docs/format/schema/v0/timeline.schema.json
loop = false
moments = [
  { t = 0, call = "intro" },     # runs moment_intro() on the instance running the timeline
  { t = 150, call = "wave1" },
  { t = 312, call = "wave2" },
]
```

`t` counts ticks from the timeline's start. Moments must ascend strictly; `call` is a function name. `loop = true` sets `TimelineDef::looping`: a run started through `timeline::start_with_def` begins with `timeline_loop` set. The `call` names are kept in `TimelineDef::calls` for the script host. Avoidance patterns are synced to music by these tick counts, as in the originals.

An object starts a timeline with the `timeline = "<id>"` key in its [object file](objects.md): every instance of that kind starts the timeline at Create, at position 0, and its first tick runs moment 0. Each moment calls `moment_<call>()` (no arguments) on the instance's script; a call the script does not define does nothing. A script's `create()` may replace the start with `timeline::start_with_def`, the same function the trigger actions use.
