# Comparing traces with the original

Part of [Architecture](../architecture.md).


Ground truth comes from the research repo. The Phase 2 recorder runs the original under OpenGMK with `iwc-trace.patch` and writes one CSV per case, next to its `.inputs` file, in `research/specs/engines/<engine>/traces/<game>/<case>.csv`. Selected frames are cross-checked against the real GM8 runner through the research gate.

**A trace is evidence, not the design.** A diff proves what the original did on the traced path. It is the strongest evidence for an F1 mechanic; a probe-only F3 column is recorded, not cloned. A deliberate deviation declares its F tier, names the feel contract that covers it and is logged in the game's deviation log; the case then has a documented tolerance or a named ignored/unprobed column, never a silent skip ([0005](../decisions/0005-standard-mechanics.md), [fidelity tiers](../plans/fidelity-tiers.md), [feel contracts](feel-contracts.md)).

This page is the live text of sections 9.1-9.3 of [the sim core plan](../plans/sim-core.md), which stays only as history; this page is the authority. Replay files are on [Input and replay format](replay.md), and the commands are on [The `iwbtg` CLI](cli.md).

### Trace cases

About 250 CSVs exist (2026-10-24), more arriving: kaenbyou 56, deride-kotaro-from-aslant 32, little-runner 29, machinary 27, animus 20, rely-the-needle-4-2 17, little-trap 11, go-the-spook-house 10, eat-the-lemon 9, help-the-fairy-tale 9, go-the-jun-g-world 8, poronkusema 7, clear-the-easy-miku-2 6, eat-the-rori-girl 6, eat-the-new-year 5, get-the-rainbow-feather 3, showdown 1, keep-celebrating-for-xia 0. Kaenbyou traces carry `.dllcalls.txt` and use `--stub-dlls`.

`iwbtg trace import` sorts each case into one of three kinds from the header command line:

1. **Diff targets.** `--room`, `--from`, `--x/--y`, and `--gml` that only assigns variables (`--gml 1:global.difficulty=1`, before room entry, or at a frame). Columns are GM variables, `instance_number(...)`, `keyboard_check*(...)`. These become case files and must diff clean.
2. **Probe traces that settle spec items.** `--gml` with `object_event_add` or log strings (`t-ord-1`, `t-evo-1`, `t-room-1`, `tr-room-01-a`, `zz-*`). Our sim cannot run injected GML. Research reads these and updates the spec; a golden test with the `Probe` built-in then reproduces the spec's stated result.
3. **Probe-read traces.** `--gml` that reads and decodes files into globals (`famb-save-restart-*` decrypts the save into `global.tr_*`). These map to probe columns on `SaveSlot` fields, not to setup.

Case files live in the client repo at `crates/tools/cases/<package-id>/<case>.toml` (they hold package ids and research trace names, no creator content), written by `iwbtg trace import` and finished by hand.

```toml
source = "kaenbyou/t-alarm-1"          # research/specs/engines/gm8/traces/<source>.{csv,inputs}
start = { room = "template", difficulty = 0 }   # StartPoint::Room, mapped from rTemplate by trace-map
first_frame = 2                         # recorder --from
object = "kid_bullet"                   # our kind for the traced original object
dll_mode = "stub"                       # recorder --stub-dlls
setup = [
  { frame = 0, global = "difficulty", value = 1.0 },   # frame <= first_frame: applied before room entry
  { frame = 3, target = "kid", set = { x = 96.0, y = 567.4, xprevious = 96.0, yprevious = 567.4 } },
]
vars = { "alarm[0]" = "alarm0", "player.x" = "kid.x", "player.y" = "kid.y" }
ignore = ["id", "sprite_index"]
```

- `setup` is the fixed vocabulary described under Mapping and diff below: a kid field, the first instance of a kind, a named instance, the `n`-th instance in instance order, a global or a `SaveSlot` field. Entries with `frame <= first_frame` run before `start_game` enters the room.
- Frame mapping: the row at `first_frame` is the state after the first tick in the room (kaenbyou t-speed-1). Our run treats tick k as frame `first_frame + k`. Inputs come from the `.inputs` file by frame number through the engine key map (`KeyMap`).

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
- A variable holding an array records as one quoted string cell with its compact text form: `flags` = `"[1.0, [2.0], ""x""]"` (#1055). The form is a `[`-`]` list joined by `, ` with nested arrays recursing and `[]` for the empty array; a real uses the same shortest round-trip decimal as a number cell, a string is quoted with `"` doubled, an instance id is a decimal integer (ours, like the `id` column), and `Value::None` (no GML value) is `undefined`. The text is deterministic, and `diff` compares it as text, so a changed element, nesting or order shows as a difference; before #1055 an array was an empty cell and so looked like an unset variable. An original recorder trace has no array cell, so such a column can only match an original string that spells the same text, and a string variable whose text is exactly an array's compact form records identically to that array.
- A phases file (`<case>.phases.csv`, the recorder's `--phases P,...`, research `tooling/opengmk-trace/README.md`) has the same columns with a bare `phase` column after `frame` (`"begin_step"`, `"keyboard"`, `"step"`, ...) and one row per listed phase of each frame, in phase order. `iwbtg_sim::trace` parses and re-writes it like any trace (`TraceTable::phase_column` finds the column). `diff` matches its rows on (frame, phase, object, instance order), keeps context frames in the same phase, prints the phase after the frame (`frame 21 step, world #0`), and refuses to compare a phases file with a plain trace. `iwbtg trace run` writes no phases file yet, so phases files are read by spec work and the research-wide tests only (#865).
- **OPEN:** the exact point in the frame where the recorder samples (after End Step, or after Draw). It must be documented in the CLONE_SPEC, and `iwbtg trace run` samples at the same point.

### Mapping and diff

- Packages never mirror original ids (principle 9). `games/<id>/tests/trace-map.toml`, which is not shipped, maps the original's names to ours: rooms (`room1` to `stage1_01`), objects (`player` to the kid), sprites (`sprPlayerFall` to `kid_fall`), variables (`djump` to `kid.air_jumps_left`), and the frame offset between the trace's frame numbers and our ticks.
- Object `params` in the trace-map take the same values as preset params in object files: a string, number or boolean, or a list of them (`params = { gate_bosses = [1, 2] }`, #894). A list holds single values, not lists.
- `iwbtg trace run <pkg> <case.inputs> -o ours.csv` writes our side with the same columns, translated through the map.
- `iwbtg trace diff <original.csv> <ours.csv> --map trace-map.toml` compares every field bit for bit and stops at the first difference. It prints the frame, the field, both values in decimal and hex bits, the ULP distance, 5 frames of context on each side, and the inputs of the previous 10 frames.
- **Sub-trace mode** (`--stages`) records our state after each pipeline step, so the diff can name the step where our value first left the original's path.
- A case's `setup` entry `{ frame, target, set }` writes fields of one instance before that frame's tick. `target` is `kid` (the first kid), a kind name (its first instance), `named:<name>` (a room-file instance name) or `kid#<n>` / `<kind>#<n>`: the instance at 0-based index `n` in instance order, the order "the first" uses (R21), like GML `instance_find(obj, n)`. A recorder `--gml` `with(player){if(id>=105000)...}` that marks one of several kids by id becomes `kid#<n>` with the index of that id among the kids (eat-the-lemon famb-multikid-restart-b, #672). A target with no instance is an error; setup never creates one.
- A case's `setup` entry `{ frame, global, value }` sets a global before that frame's tick (before room entry when `frame < at`). The name resolves like a `global.<name>` probe column: engine state first, then a global the package declares. Setup can write the run's `time` (the play timer), `death`, `savenum` and `clear` (#688; eat-the-lemon famb-multikid-restart-b sets `time = 777` on frame 25, the recorder's `global.timestep=777`, so the autosave then stores 777). The difficulty is the case's `start`; the other engine globals (the kid flags, `paused`, the latched keys) are an error, never the package global of the same name, so a setup and the column it shows up in always name the same state.
- A case's `latch` key (column to read frames) reproduces a recorder `--gml` that copied a value (a decoded save field) into a global before chosen frames: the column holds the `vars` value at the end of the frame before the last read, 0 before the first. The runner reads that value after every tick whether or not the traced object has a row then (a frame where the kid is gone still latches the save, #671); only a column that needs the row's own instance (a fixed column, a `Row` field) falls back to our rows.
- A case's `split` key reads a byte-dump column (a recorder `--gml` that read a save file back as space-separated decimal bytes, such as showdown's `global.sv`) as one column per field before anything else sees the original: `split = { "global.sv" = ["room:3", "x:3", "y:3", "difficulty:1", "_:16"] }` gives `global.sv.room`, `global.sv.x`, ..., each the base-100 number of its bytes (the Group A save layout); `_` bytes are dropped, `"none"` gives empty cells and an unread `0` stays 0. The field columns then take `vars` (`save.x`), `latch` and `unprobed` entries like any other column.
- `iwbtg trace import` maps each original variable column to a probe column (`probe::registry::column`): plain variables, `bbox_*`, `<object>.<field>`, `global.<name>`, `instance_number(...)`, `keyboard_check*(...)`, `random_get_seed()` (the gameplay generator's state as a signed 32-bit integer, as the recorder prints it; #911) and `place_free(<x>,<y>)` with arguments of the traced instance's fields and numbers joined by `+` and `-` (`place_free(x+hspeed,y)`, the bounce tests of animus tr-mech-04, #888). `bbox_*` of an instance with no sprite and no mask reads -100000 for all four (CLONE_SPEC collision, Confirmed (recorder), #905); the sim reads `bbox_*` only through the probe, so no other path needs the rule. `mask_index`, `image_number`, `sprite_width` and `sprite_height` map too (#919); `mask_index` is our sprite id (the sprite that owns the instance's mask), not the original's GM sprite id, so a case skips it by default together with `id`, `sprite_index` and `sprite_name` (`DEFAULT_IGNORE`, #925): the GM ids never equal ours, and mapping them to ours needs a sprite-id table the trace map does not carry, added for this one column. Only the -1-versus-set signal is lost. `sprite_width` and `sprite_height` are the sprite's frame width (height) times the signed `image_xscale` (`image_yscale`), exact under any scale and independent of `image_angle`; the mask does not decide the size, a sprite with no frames reads 0, and an instance with no sprite stays an empty cell (OPEN, trace request `NUM-SPRITE-SIZE`; research#231). A column it cannot map (another function call, `*` or `div`, a built-in the probe has no field for such as `xstart` or `object_index`) goes under `[unprobed]` with the reason; `trace check` counts those as not compared.
- Every ported game needs at least one trace case per mechanic it uses before it is ranked.
- `animus/tr-mech-04` (20 bouncing `boundwhite` cherries, the killer `bounce` param, #835) runs against the converter's animus package, because the games repo has no animus package yet: from `research/`, `uv run iwc convert --game animus --out <tmp>/packages --reports <tmp>/reports` (research main with PR 204, so `objects/boundwhite.toml` has `base = "killer"` and `bounce = true`), then `iwbtg trace check <tmp>/packages/animus`. A package from before PR 204 fails on frame 2 (y 200 instead of 192). All its columns are compared, the 4 bbox and 3 `place_free(...)` columns included (#888).
- `rely_the_needle/num-rng-a` (#911) checks the converter's rely objEffect (preset `@effect`, two particle streams spawned by each room's `on_create`, research PR 225) through the `random_get_seed()` column: the particle draws share the gameplay generator with the blood, so a change to the systems, their order or their streams moves the seed on frame 2. Built from research main as for animus above (`--game rely-the-needle-4-2`), the package passes all 99 rows. The case is written by hand (import refuses the recorder's `object_event_add` probe, which draws nothing) and starts at room1 with `seed = 1598751577`, the frame 1 seed after the boot rooms' own particle draws. It ignores the world controller's own fields and, until #960, `instance_number(blood)`.
