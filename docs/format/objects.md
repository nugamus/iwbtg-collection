# Objects: `objects/<id>.toml`

Part of [format v0](v0.md).

Built-in kinds need no file. A package object file defines one of two things.

A **preset**: a built-in with fixed parameters. This is how near-duplicate original objects collapse into one (principle 9). The "hide spikes" triggers in Fed up become one preset:

```toml
#:schema ../../../client/docs/format/schema/v0/object.schema.json
# objects/hide_spikes.toml defines @hide_spikes
base = "trigger"
on = "kid_touch"             # kid_touch | shot | room_tick | flag
once = true
actions = [ { do = "hide", target = "group:{group}" } ]   # {group} comes from the instance
```

A **scripted object**: a script, with an optional built-in base for native behavior and roles.

```toml
#:schema ../../../client/docs/format/schema/v0/object.schema.json
# objects/boss1.toml defines @boss1
sprite = "boss1"
script = "boss1"             # scripts/boss1.rhai
solid = false
counts_as = ["bullet_blocker"]
collides_with = ["kid_bullet"]   # collide(other) runs only for these kinds
params = { hp = 20 }             # defaults; instances may override
timeline = "boss1_phases"        # optional; started at create
```

```toml
# objects/orb.toml defines @orb: a native killer that kills on touch
base = "killer"
sprite = "orb"
counts_as = ["killer"]
```

**OPEN:** whether presets may also override built-in parameters per instance beyond `params`.

**List params.** A built-in param whose page says "list" takes a TOML array of its values (`system = ["rain", "fx"]`, `on_room_start_except = ["stage1a", "title"]`) or a single value. Each value is checked like a single one (an unknown room or file is an error); an empty list is an error, and so is a list on any other param, a script kind's included.

Object keys: `base`, `script`, `sprite`, `mask` (an object-level collision mask sprite, GM object `mask`), `solid` (default false), `visible` (default true), `depth` (default 0), `persistent` (default false; true for the `kid` built-in, which is persistent in all 20 games, so a restart that creates the kid and changes room keeps it), `counts_as` (extra collision classes: named classes such as `block` or `killer`, or package class names; sim plan 1.1), `params`. Every other key is a param. The legacy converter key `roles` is read only by trace builds (`LoadMode::Trace`, CR11); write `counts_as` instead.

**Params that pick a class or an event set** (#659, #853): water `variant`, vine `side`, block `kid_collides`, killer `kill_outside_view`, slope `shape` and `scale_x` pick classes (`objects::class_params`). Others pick the event set, the events a kind handles: killer `bounce` and `destroy_outside_room`, block `proximity_frame`, a `mode` on several built-ins, and so on (the full list is `builtins::event_set_params`; a test checks it against every param a built-in's `event_set` reads). An instance's classes and events come from its kind, as a GM instance's come from its object. So when a placed instance sets one of these to pick other classes or another event set than its kind's, the loader gives it a kind derived for it, `<kind>[<param>=<value>,...]` (`water[variant=2]`, `killer[bounce=true]`), with those classes and events, the kind's other settings and `counts_as`, and the kind's `object_order` rank. All instances with the same values share it; a value that picks the kind's own classes and events (`variant = 1` on `water`, `bounce = false` on `killer`) keeps the kind. Without this, `{ kind = "killer", bounce = true }` would load and never bounce. This is how converted rooms place `water2` and the vines (`{ kind = "water", variant = 2 }`). Like a `[defaults.<kind>]` variant, a derived kind is its own kind in the sim; probe kind targets and the script `instances(kind)` cover it. A script `instance_create` takes the `[defaults.<kind>]` variant its params match, with the same match as a placed instance, so the loader derives a kind for every variant, placed or not (#701). One whose params pick other classes or events takes the derived kind with those values, and is a script error when the package has none (#694, #853).

Object files (presets and scripted objects) may set `depth` (default 0) and `visible` (default true); instance keys of the same name override them. A preset may set `sprite`, which also gives it that sprite's collision mask instead of the built-in's default. Decided on the user's behalf, 2026-10-24 (research/converter/FORMAT_NOTES.md, OPEN 18 and 19).
