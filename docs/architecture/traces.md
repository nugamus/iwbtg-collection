# Comparing traces with the original

Part of [Architecture](../architecture.md).


Ground truth comes from the research repo. The Phase 2 recorder runs the original under OpenGMK with `iwc-trace.patch` and writes one CSV per case, next to its `.inputs` file, in `research/specs/engines/<engine>/traces/<game>/<case>.csv`. Selected frames are cross-checked against the real GM8 runner through the research gate.

### Recorder behavior the trace runner copies

The runner sets `DirectLatch::None` (the recorder has no `keyboard_check_direct` latch). Probe reads follow the recorder's row timing: `keyboard_check_pressed` and `keyboard_check_released` read 0 (rows are written after the frame), and `player.x` with no player instance reads 0 (uninitialized variables as 0, `[engine] uninit_reads_zero`).

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
- A case's `latch` key (column to read frames) reproduces a recorder `--gml` that copied a value (a decoded save field) into a global before chosen frames: the column holds the `vars` value of the frame before the last read, 0 before the first.
- Every ported game needs at least one trace case per mechanic it uses before it is ranked.
