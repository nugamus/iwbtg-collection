# Comparing traces with the original

Part of [Architecture](../architecture.md).


Ground truth comes from the research repo. The Phase 2 recorder runs the original under OpenGMK with `iwc-trace.patch` and writes one CSV per case, next to its `.inputs` file, in `research/specs/engines/<engine>/traces/<game>/<case>.csv`. Selected frames are cross-checked against the real GM8 runner through the research gate.

### Recorder behavior the trace runner copies

The runner sets `DirectLatch::None` (the recorder has no `keyboard_check_direct` latch) and `TickRules::fps_zero_ticks = 51 - at` (`RECORDER_FIRST_FPS_FRAME`: the recorder's GML `fps` reads 0 up to frame 50, research kid/B.md "Play timer"; normal play never reads 0, see docs/format/builtins/world.md). Probe reads follow the recorder's row timing: `keyboard_check_pressed` and `keyboard_check_released` read 0 (rows are written after the frame), and `player.x` with no player instance reads 0 (uninitialized variables as 0, `[engine] uninit_reads_zero`).

### CSV columns

The recorder's columns, as in `research/specs/engines/gm8/traces/rely-the-needle-4-2/djump.csv`:

```
frame,room,object,id,x,y,xprevious,yprevious,hspeed,vspeed,speed,direction,
gravity,gravity_direction,friction,image_index,sprite_index,sprite_name,<game vars...>
```

- One row per frame for each traced instance (usually the player).
- The game variables after `sprite_name` are chosen per trace (`djump`, `jump`, `jump2`, `onPlatform`, `maxSpeed`, `maxVspeed`, `image_xscale` for Rely the Needle; `curJumps`, `maxJumps`, `jump[1]`, `jump[2]`, `grav` for Kaenbyou).
- Numbers are shortest round-trip decimals (for example `-7.347880794884119e-17`). Rust parses these to the exact same double, so comparison is bit-exact after parsing.
- **OPEN:** the exact point in the frame where the recorder samples (after End Step, or after Draw). It must be documented in the CLONE_SPEC, and `iwbtg trace run` samples at the same point.

### Mapping and diff

- Packages never mirror original ids (principle 9). `games/<id>/tests/trace-map.toml`, which is not shipped, maps the original's names to ours: rooms (`room1` to `stage1_01`), objects (`player` to the kid), sprites (`sprPlayerFall` to `kid_fall`), variables (`djump` to `kid.air_jumps_left`), and the frame offset between the trace's frame numbers and our ticks.
- `iwbtg trace run <pkg> <case.inputs> -o ours.csv` writes our side with the same columns, translated through the map.
- `iwbtg trace diff <original.csv> <ours.csv> --map trace-map.toml` compares every field bit for bit and stops at the first difference. It prints the frame, the field, both values in decimal and hex bits, the ULP distance, 5 frames of context on each side, and the inputs of the previous 10 frames.
- **Sub-trace mode** (`--stages`) records our state after each pipeline step, so the diff can name the step where our value first left the original's path.
- A case's `setup` entry `{ frame, target, set }` writes fields of one instance before that frame's tick. `target` is `kid` (the first kid), a kind name (its first instance), `named:<name>` (a room-file instance name) or `kid#<n>` / `<kind>#<n>`: the instance at 0-based index `n` in instance order, the order "the first" uses (R21), like GML `instance_find(obj, n)`. A recorder `--gml` `with(player){if(id>=105000)...}` that marks one of several kids by id becomes `kid#<n>` with the index of that id among the kids (eat-the-lemon famb-multikid-restart-b, #672). A target with no instance is an error; setup never creates one.
- A case's `setup` entry `{ frame, global, value }` sets a global before that frame's tick (before room entry when `frame < at`). The name resolves like a `global.<name>` probe column: engine state first, then a global the package declares. Setup can write the run's `time` (the play timer), `death`, `savenum` and `clear` (#688; eat-the-lemon famb-multikid-restart-b sets `time = 777` on frame 25, the recorder's `global.timestep=777`, so the autosave then stores 777). The difficulty is the case's `start`; the other engine globals (the kid flags, `paused`, the latched keys) are an error, never the package global of the same name, so a setup and the column it shows up in always name the same state.
- A case's `latch` key (column to read frames) reproduces a recorder `--gml` that copied a value (a decoded save field) into a global before chosen frames: the column holds the `vars` value at the end of the frame before the last read, 0 before the first. The runner reads that value after every tick whether or not the traced object has a row then (a frame where the kid is gone still latches the save, #671); only a column that needs the row's own instance (a fixed column, a `Row` field) falls back to our rows.
- A case's `split` key reads a byte-dump column (a recorder `--gml` that read a save file back as space-separated decimal bytes, such as showdown's `global.sv`) as one column per field before anything else sees the original: `split = { "global.sv" = ["room:3", "x:3", "y:3", "difficulty:1", "_:16"] }` gives `global.sv.room`, `global.sv.x`, ..., each the base-100 number of its bytes (the Group A save layout); `_` bytes are dropped, `"none"` gives empty cells and an unread `0` stays 0. The field columns then take `vars` (`save.x`), `latch` and `unprobed` entries like any other column.
- Every ported game needs at least one trace case per mechanic it uses before it is ranked.
