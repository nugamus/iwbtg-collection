# Rooms: `rooms/<room_id>.toml`

Part of [format v0](v0.md).

```toml
#:schema ../../../client/docs/format/schema/v0/room.schema.json
size = [800, 608]
file_speed = 50              # room_speed on entry (R14); default is game.toml tick_rate
role = "play"                # boot | title | menu | select | play; default play
no_restart = false           # true: R does nothing in this room (animus room52); default false
djump_grants_tjump = false   # true: the double jump also grants the third jump (showdown room028); default false
physics = "needle"           # optional; the room profile physics/needle.toml (medleys); default the game's
music = "stage1"             # optional; "keep" keeps the current track
background_color = "#000000"
backgrounds = [
  { image = "sky", speed = [1, 0] },
  { image = "fog", foreground = true, offset = [0, 512], tile = [true, false] },
]
persistent = false
views_enabled = true
views = [
  { visible = true, view = [0, 0, 800, 608], port = [0, 0, 800, 608], follow = "kid", border = [32, 32], speed = [-1, -1] },
]
on_create = [                # room creation code, as data (sim plan 4.2 step 6)
  { do = "set_global", var = "difficulty", value = 1 },
  { do = "spawn", kind = "@intro", x = 0, y = 0, params = { lines = 3 } },
  { do = "set_global", var = "stage", index = { global = "savenum" }, value = 29 },
  { do = "move_kid", x = 241, y = 503, when = { difficulty_below = 3 } },
  { do = "save_game", when = { difficulty_below = 3 } },
]
instances = [
  { kind = "kid_start", x = 48, y = 552, gm_id = 104180 },
  { kind = "block", x = 0, y = 576, run = [25, 1] },
  { kind = "spike", x = 320, y = 544, dir = "up" },
  { kind = "spike", x = 352, y = 544, dir = "up", group = "trap1" },
  { kind = "@hide_spikes", x = 288, y = 448, w = 32, h = 96, group = "trap1" },
  { kind = "water", x = 480, y = 384, variant = 2, run = [4, 6] },
  { kind = "vine", x = 640, y = 256, side = "left" },
  { kind = "save", x = 64, y = 512, name = "save_a" },
  { kind = "trigger", x = 600, y = 0, w = 32, h = 608, on = "kid_touch", emits = "stage1_clear" },
  { kind = "warp", x = 768, y = 512, to = { room = "stage1_04", at = "spawn_left" } },
  { kind = "@boss1", x = 400, y = 200, hp = 30 },
]
```

- **Order is creation order.** The list order is the GM creation order, which decides event and collision order within a kind. Tools never sort it.
- **Common instance keys:** `kind`, `x`, `y`, `name`, `group`, `xscale`, `yscale`, `angle`, `visible`, `depth`, `run`, `step`, `gm_id`. Any other key is a parameter of the kind and is checked against its schema.
- **`gm_id`** (optional, #680) is the GM8 instance id the original's room file gives this instance, a whole number from 0 to 2147483647. Ids are unique inside one room (a repeat is an error) but may repeat in other rooms, as GM8 rooms share ids. An instance with `gm_id` cannot have `run`. GM8 rules the sim follows with them (research `specs/engines/gm8/CLONE_SPEC.md`, Instances, "Placed instance ids"; the sim side is #669): a placed instance is created with its `gm_id`, not a new id; a placed instance is **not created when an instance with its id already exists** (a persistent kid carried in from an earlier room or kept across a restart; whether an instance destroyed in the same step still counts is OPEN, #670); runtime ids start at `[engine].first_runtime_id`, only grow and are never reused, so every runtime id is above every placed id (the loader checks `first_runtime_id` is above every `gm_id` of the package); where list order and id order differ inside one kind, `with` and `object.variable` follow id order, while Create events still run in list order. The research converter writes `gm_id` on every placed instance and `first_runtime_id` (research#141); hand-made packages leave both out.
- **`run = [columns, rows]`** expands into a grid of identical instances in row-major order. Spacing is `step = [dx, dy]`, default `[32, 32]`. It expands into separate instances at load and is never merged into one rectangle.
- **Warps** point at `{ room, at = <instance name> }` or `{ room, x, y }`.
- **Room creation code** is `on_create`, a list of rows run in order in step 6 of the room start sequence (after every Create and Game Start, before every Room Start, also when no kid exists yet). The rows (research `specs/engines/gm8/creation-code.md`, #629):
  - `set_global` (`var`, `value`): `global.<var> = value`, a number, string or boolean (true is 1). A global named `difficulty` also sets the engine difficulty that built-ins such as the bow read, like a ranked start's, so the loader requires a whole number from 0 to 255. With `index`, `global.<var>[index] = value`: `index` is a whole number from 0 or `{ global = "<name>" }`, read when the row runs (`savenum` is the engine's save slot; any other name is a package global that must then hold a whole number from 0). Element 0 is the variable itself: GM8 dereferences a bare global to its element 0, so `global.stage[0] = 29` sets `global.stage`, and a bare `global.stage` read sees a value written at index 0 (research#130, Confirmed). Element 1 and above are their own globals, `<var>[<n>]`; negative and non-whole indices are UNKNOWN (`NUM-ARRAY-IDX`), and the sim rejects them as `Unsupported`.
  - `spawn` (`kind`, `x`, `y`, optional `params`): `instance_create`.
  - `load_game`: the engine's `loadGame` (R23).
  - `save_game`: the game's save, as its save point runs it (`[save]` position model in `physics.toml`). With no kid it saves x 0, y 0 without an error (Confirmed (recorder), research kid/A3.md "Saves", showdown a3-t20-a).
  - `save_death_time`: the game's death and time save; it updates the counters of an existing save and leaves its room and position.
  - `move_kid` (`x`, `y`): moves the first kid there (`player.x = ...; player.y = ...`); with no kid it does nothing (showdown a3-t18-a, a3-t20-a).
  - `destroy` (`kind`): `with (<kind>) instance_destroy()`: every instance of that kind runs Destroy and is removed. A GM parent's children are separate kinds, one row each.
  - `play_sound`, `loop_sound`, `stop_sound` (`sound`): `sound_play`, `sound_loop`, `sound_stop`.
  - `destroy_self`: `instance_destroy()`, which destroys nothing; the rest of the code runs (R24, Confirmed). The converter drops it.
- **`when`** on any row runs it only while every key it sets holds: `difficulty_below = n` (`global.difficulty < n`), `kid_exists = true` (a kid exists; `false`: none does), `kind_absent = "<kind>"` (no instance of that kind exists). A `when` with no key is an error. Room creation code the rows cannot say (an `if` on other values, `with` on other code) stays OPEN in the converter's report.
- **Instance creation code** is instance keys: whatever the original's creation code set is written on the instance. `depth`, `visible`, `xscale`, `yscale` and `angle` are common keys for every kind and override the kind's own (they are never params, so `pkg check` accepts them on every built-in); other variables (`roomTo`) are params of the kind. A variable whose name is an instance key (`kind`, `name`, `group`, `run`, `step`, ...: go-the-jun-g-world yuno_warp sets `kind`) goes in the instance's `params` table: `{ kind = "@yuno_warp", x = 0, y = 0, params = { kind = 20 } }`. `params` holds only such names; any other param is written as a key of its own.
- **Views:** `views_enabled` and up to 8 `views` (`visible`, `view = [x, y, w, h]`, `port` default the view's size at (0, 0), `follow` a kind, `border` default `[32, 32]`, `speed` default `[-1, -1]` for no limit) carry the room file's view data and GM's built-in follow (`follow`, `border`, `speed`) **only where the room sets a followed object**. In the 20 games that is eat-the-lemon (5 rooms, `player`) and animus (8 rooms: `player` in 7, `rocketcamera` in 1); every other room fixes its view or moves it with a camera object (research#14).
  - A game camera object becomes a `camera` instance ([camera](builtins/camera.md)): it moves view 0 (the snap modes write `view_xview`/`view_yview`; `smooth_follow` sets the speed that moves them), and the view size and port stay as room data.
  - A room uses one mechanism or the other. Do not put `follow` in a camera room's `views`: the runner follow and the camera's event would both move view 0 (the runner follow runs in the view phase, the camera's event in its own phase, in that order). help-the-fairy-tale places the most camera instances (32, `mode = "screen_snap"`, 800 by 608), none in a room with a followed object.
- **Backgrounds:** `background_color = "#rrggbb"` fills the room behind everything; without it the room clears to nothing of its own (GM "draw background color" off). `backgrounds` lists up to 8 layers in drawing order (GM's background slots 0 to 7, visible ones only): `image` (a `backgrounds/<id>` id), `foreground` (default false: drawn after every instance and tile instead of before them), `offset = [x, y]` (where the image's top-left corner sits in the room, default `[0, 0]`), `speed = [x, y]` (scroll in pixels per tick, default `[0, 0]`), `tile = [horizontal, vertical]` (repeat the image along that axis, default `[true, true]`) and `stretch` (scale the image to the room's size, default false; with `tile` the stretched image repeats). `background = "<id>"` is short for `backgrounds = [{ image = "<id>" }]`; a room sets one or the other. A missing image is a load error.
- **Roles** (`role`) mark boot, title, menu, select and play rooms. Restart and reset read them (`excluded_roles`). A room without `role` is a play room. `no_restart = true` turns R off in that one room (a play room where the original disables the key); F2 is not affected.
- **`physics = "<id>"`** gives the room its own kid physics, `physics/<id>.toml` (see [Room profiles](physics.md#room-profiles)), for medleys whose rooms come from games with different physics.
- **`djump_grants_tjump = true`** makes the double jump also set `tjump` in that room, so a third jump follows (showdown room028, whose playerJump tests the room by name). It only matters with `[jump] air = "djump_then_tjump"` in `physics.toml`.

Decorative tiles go in `rooms/<room_id>.tiles.toml`:

```toml
#:schema ../../../client/docs/format/schema/v0/tiles.schema.json
layers = [
  { depth = 1000000, tileset = "stage1_tiles", tiles = [
    { x = 0, y = 576, src = [0, 0], run = [25, 1] },
    { x = 64, y = 64, src = [32, 0, 16, 16], run = [3, 1], step = [24, 0] },
  ] },
]
```

Tiles never affect the sim. A layer has a `depth` and a `tileset` (a `backgrounds/<id>` id; a missing image is a load error). A tile is the rectangle of the tileset image at `src`: `[x, y]` for a tile of the tileset's `tile_size`, or `[x, y, w, h]` for a tile of its own size (GM 8 stores a size per tile, and 1 in 60 converted tiles differs from its tileset's usual size). It is drawn at `x, y` (top-left) at its own size; `run = [columns, rows]` repeats it in a grid with spacing `step = [dx, dy]`, default the tile's size.

A tileset may have a sidecar `backgrounds/<id>.toml`:

```toml
#:schema ../../../client/docs/format/schema/v0/background.schema.json
tile_size = [32, 32]   # default [32, 32]: the size of a tile written as src = [x, y]
separation = [0, 0]    # default [0, 0]: pixels between tiles in the image
```

`separation` is for editors that cut the image into a tile palette; drawing never uses it, since every tile names its own source. GM 8 marks no background of the 20 games as a tileset, so the converter derives `tile_size` from the tiles that use the image (the most common size).

**How room art is drawn** (`iwbtg_format::art` reads it, `crates/client/src/art.rs` draws it; order from the GM8 CLONE_SPEC, Rooms). First `background_color`, then the layers that are not foregrounds in list order, then instances and tile layers by `depth` (a larger depth is farther back; tiles share the order with instances), then the foreground layers in list order. A layer's image starts at `offset` plus `speed` times the ticks since the room started, repeats along each tiled axis and is cut at the room's edge. OPEN: whether GM 8 moves a scrolling background before the room's first frame and keeps fractions (CLONE_SPEC, Rooms OPEN); the client draws the first frame at `offset` and keeps fractions. A game's code that changes `background_x` or `background_hspeed` at run time (animus) is not modelled.
