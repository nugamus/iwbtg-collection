# Architecture

Status: draft for roadmap Phase 3. Accepted by Claude on the user's behalf, 2026-10-24 (user may revisit).

This page describes how the client repo is built: the crates, the simulation, how numbers and collisions work, the first script API, the replay format, and how we prove that a port matches its original.

Related: [0001 script language](decisions/0001-script-language.md), [0002 data syntax](decisions/0002-data-syntax.md), [format v0](format/v0.md), [format principles](format/README.md), [server](server.md), [roadmap](roadmap.md).

Facts about GameMaker 8 behavior come from the engine spec, `research/specs/engines/gm8/CLONE_SPEC.md` (not written yet). Where this page names a GM8 rule, the spec is the authority. Anything marked **OPEN** waits for it.

## Goals

- One simulation, `crates/sim`, runs the game on the client, the leaderboard server and the CLI.
- The simulation is deterministic: the same package and inputs give bit-identical state on x86_64 Windows (MSVC), WASM and x86_64 Linux (the server).
- A port reproduces the original's positions tick for tick. Traces of the original prove it.
- Agents write and maintain all code. Clarity, tooling and guardrails come before cleverness.

## Crates

One Cargo workspace in `client/`, `license = "MIT OR Apache-2.0"`, toolchain pinned to an exact stable in `rust-toolchain.toml`.

| Crate | What it does | Depends on |
|---|---|---|
| `crates/sim` | The deterministic simulation: world, built-ins, kid, collision, tick pipeline, script host, replays, state hash. No Bevy, no IO, no threads, no clock. `#![forbid(unsafe_code)]`. | `rhai` (exact pin), `libm` (exact pin, no default features), `blake3`, `smallvec`, `postcard`, `serde`. Optional later: `rustc_apfloat` (see [numbers](#number-handling)). |
| `crates/format` | Reads, validates, hashes and writes game packages. Turns TOML files into `sim::def::GameDef`. Parses from an in-memory map of path to bytes (`PackageFiles`), so the client, the server and WASM share one code path with no filesystem. | `sim`, `toml`, `toml_edit`, `serde`, `serde_spanned`, `schemars`, `png`, `blake3`, `sha2`. |
| `crates/tools` | The `iwbtg` command-line tool (see below). Later also the music preview picker and the level editor. | `sim`, `format`. |
| `crates/client` | The Bevy app: game select, rendering, input, audio, settings. Owns one `sim::Sim`, feeds it one input frame per tick and draws its snapshots. | `sim`, `format`, `bevy`. |
| `crates/server` | Phase 8. Leaderboard backend. Verifies replays by running `sim` in a child process ([server](server.md)). | `sim`, `format`, `axum`, `sqlx`. |

The sim never parses text and never touches files. `format` hands it a validated `GameDef`. The client and server hand it input frames.

### The `iwbtg` CLI

One binary for agents and CI:

- `iwbtg pkg check <dir>`: full validation, one readable error list with file, line and column.
- `iwbtg pkg fmt <dir>`: canonical formatting ([0002](decisions/0002-data-syntax.md)).
- `iwbtg pkg hash <dir>`: prints the gameplay hash and content hash.
- `iwbtg pkg upgrade <dir>`: migrates a package to the newest format version.
- `iwbtg room show <dir> <room>`: prints an ASCII map of a room.
- `iwbtg replay run <dir> <replay>`: headless run. Prints the tick count, splits and final state hash (roadmap Phase 3).
- `iwbtg replay dump <replay>`: prints a replay as text.
- `iwbtg replay verify <dir> <replay>`: what the server runs.
- `iwbtg trace run <dir> <inputs> -o ours.csv` and `iwbtg trace diff <original.csv> <ours.csv>`: see [traces](#comparing-traces-with-the-original).
- `iwbtg profile <dir> --replay <file>`: peak script budget units and milliseconds per tick.
- `iwbtg determinism report <corpus>`: runs the golden corpus and prints one hash list.
- `iwbtg script api-docs`: generates `docs/format/script-api.md` and `.d.rhai` stubs.

## Sim module layout

```
crates/sim/src/
  lib.rs          Sim, SimStatus, StepResult, SIM_VERSION
  num/            Real newtype, GM8 rounding and comparisons, soft math on libm, x87 trig table
  rng.rs          GM8-compatible random generator
  input.rs        Action, InputFrame
  def/            Immutable game definition (GameDef) built by crates/format
  world/          Instance store, per-rank order lists, room state, globals, save slot
  motion.rs       GM8 speed, direction, friction and gravity model
  collide/        Masks, bounding boxes, broadphase grid, place_meeting and friends
  kid/            Kid state, physics profile, the three rule sets (classic, max_jumps, obj_player)
  builtins/       One file per built-in kind (block.rs, spike.rs, save.rs, ...)
  events.rs       Event dispatch in (rank, instance id) order
  tick.rs         The tick pipeline
  timeline.rs     Timeline moments
  room.rs         Room load, transitions, persistent instances
  trigger.rs      Built-in trigger actions (data-driven traps)
  script/         ScriptHost trait, the Rhai host (iws_engine), API registration, budget
  timing.rs       Start, end and stage triggers, splits
  output.rs       Audio and visual event queue for the client
  replay.rs       Replay file encode and decode
  trace.rs        Trace rows for comparison with the originals
  hash.rs         Canonical state hash
  error.rs        SimError
```

Each built-in is implemented once in `builtins/<kind>.rs` and documented once in `docs/format/builtins/<kind>.md`: parameters, behavior, which event phase it runs in, which games use it, and the CLONE_SPEC section it comes from.

### Sim purity

Purity is enforced by tools, not by review. An agent that breaks a rule gets a build error, not a silent desync.

- `crates/sim/clippy.toml`, `disallowed-methods`: `f64::{sin, cos, tan, asin, acos, atan, atan2, sinh, cosh, tanh, exp, exp2, exp_m1, ln, ln_1p, log, log2, log10, powf, powi, cbrt, hypot, mul_add, round}`, `std::time::*`, `std::env::*`, `std::fs::*`, `std::thread::spawn`.
- `disallowed-types`: `std::collections::{HashMap, HashSet}`, `std::time::{Instant, SystemTime}`, `f32`.
- `usize` never appears in serialized or hashed state (wasm32 has a 32-bit `usize`).
- `cargo deny check bans` keeps `rand`, `getrandom`, `tokio`, `bevy` and `glam` out of the sim's dependency graph. `cargo deny check licenses` keeps GPL code out of the whole workspace.
- CI runs `cargo clippy -D warnings` on every target.
- The sim is never built for i686 or any other x87 target. Never `target-cpu=native`.

## World model

The sim is shaped like GameMaker, not like an ECS. Rust enums and plain structs, matched exhaustively, so an agent sees every case and the compiler flags a missing one.

```rust
struct Instance {
    id: InstId,               // u64, from a counter starting at 100001, never reused
    kind: KindId,             // built-in kind or package object
    rank: u16,                // position of the kind in object_order
    // GM built-in variables, all Real (f64)
    x, y, xstart, ystart, xprevious, yprevious,
    hspeed, vspeed, speed, direction, gravity, gravity_direction, friction,
    image_index, image_speed, image_xscale, image_yscale, image_angle, image_alpha,
    depth: Real,
    sprite: SpriteId, mask: MaskId,
    visible: bool, solid: bool, persistent: bool,
    alarms: [i32; 12],
    group: Option<GroupId>, name: Option<NameId>,
    state: KindState,         // per-kind data, see below
    script: Option<ScriptRef>,
    vars: BTreeMap<VarName, Value>, // custom variables; plain values only
    alive: bool,
}

enum KindState {
    Kid(KidState), Block, FakeBlock, HiddenBlock { shown: bool },
    Spike { dir: Dir }, Killer, Save { .. }, Warp { to: WarpTarget }, KidStart,
    Platform(PlatformState), MovingBlock(MovingState), Water { variant: u8 },
    Vine { side: Side }, JumpRefresher { .. }, Slope { dir: Dir }, GravityFlip { up: bool },
    Ladder, BulletBlocker, Trigger(TriggerState), Projectile, KidBullet, Camera(CameraState),
    MusicZone { .. }, Scripted,
}
```

The kind list above is a starting point. The real list comes from `research/specs/mechanics.md` (**OPEN**).

- **Storage.** Instances live in a `Vec<Instance>` in creation order, so it is sorted by id. Lookup by id is a binary search. Each rank keeps a `Vec<InstId>` in creation order, for event dispatch.
- **Creation and destruction.** New instances are appended. Destroy sets `alive = false`. A dead instance runs no further events. The sweep at the end of the tick removes dead instances with `retain`, which keeps order. (GM8 removes destroyed instances after the event that destroyed them; exact timing **OPEN**, from the CLONE_SPEC.)
- **Roles replace parent objects.** Collision queries ask for roles (`solid`, `killer`, `platform`, `water`, `vine`, `stops_bullets`, ...) instead of GM parent objects. Each kind declares its roles.
- **Package objects.** A package object (`@boss1`) is `KindState::Scripted` or inherits a built-in with `base = "projectile"`, `base = "trigger"` and so on. A scripted object with a base keeps the base's native behavior and roles; the script adds events. Collision with the kid works through roles, never through script code.
- **The kid** is a built-in. Scripts read it, can kill it and can set documented knobs (for example gravity flip). They never replace its physics.
- **Globals and save slot.** Game-wide variables persist across rooms, like GM globals. The save slot (room, position, gravity, flags) is part of the sim state. The client writes it to disk between sessions. Ranked replays always start from a new game.
- **Output queue.** Audio and visual effects (`play_sound`, `play_music`, screen shake, blood) are events pushed to `output`. The client consumes them. The sim never reads anything back from audio or rendering.

## Tick pipeline

`Sim::step(InputFrame) -> StepResult` runs one tick. Steps marked **[GM8]** mirror GameMaker 8's frame order. The order below follows the GM8 frame loop as OpenGMK's emulator documents it (research reference only, GPL-2.0, never copied). **The exact order, and every detail inside each step, will come from `research/specs/engines/gm8/CLONE_SPEC.md`**, and traces of the originals confirm it. If the spec disagrees with this list, the spec wins and this page gets updated.

| # | Step | GM8 | Notes |
|---|---|---|---|
| 0 | Latch the input frame for this tick | | Held, pressed and released bits come from the replay. A tap shorter than one tick arrives as pressed and released in the same tick. |
| 1 | `xprevious = x`, `yprevious = y` for every instance | **[GM8]** | |
| 2 | Begin Step events | **[GM8]** | |
| 3 | Timelines advance and fire moments | **[GM8]** | |
| 4 | Alarms count down and fire at 0 | **[GM8]** | |
| 5 | Keyboard events: press, held, release | **[GM8]** | The kid's jump and jump release run here or in Step, as its rule set says (`input_phase` in the physics profile). |
| 6 | Step events: built-ins and scripts | **[GM8]** | |
| 7 | Motion: friction, then gravity along `gravity_direction`, then `x += hspeed`, `y += vspeed`, then paths | **[GM8]** | Order inside this step from the CLONE_SPEC. |
| 8 | Outside-room and boundary events | **[GM8]** | The kid dies outside the room, depending on the engine. |
| 9 | Collision events | **[GM8]** | For each rank with collision handlers, each instance in order, each target rank, each target in order. Pairs are re-tested right before each event, because an earlier event may have moved them. Solid-instance handling (revert to previous position, run the event, reapply speeds) per the CLONE_SPEC. The kid's block collision (step 1 px at a time to contact, side snap, diagonal case, djump restore) runs here. |
| 10 | End Step events | **[GM8]** | Some engines check killers here. |
| 11 | `late()` script events | **[GM8]** position of Draw | Draw itself is not in the sim. `late()` exists for gameplay state that originals changed in Draw. |
| 12 | Animation: `image_index += image_speed`, animation-end events | **[GM8]** | Stays in the sim, because `image_index` can change a precise mask. Exact position relative to Draw **OPEN**. |
| 13 | Destroy sweep | | |
| 14 | Room change, if one was requested: room end events, keep persistent instances, create the new room's instances in file order, creation code, room start events | **[GM8]** | Whether the rest of the tick is skipped after a mid-tick request **OPEN** (OpenGMK suggests it is). |
| 15 | `tick += 1`. Timing triggers record splits as (stage, tick) | | |
| 16 | State-hash checkpoint every 500 ticks when recording or verifying | | |

Rules that apply to every step:

- **Dispatch order.** Within an event, kinds run in `object_order` rank, and instances of a kind run in creation order. This reproduces GM8's "object index, then instance" order. GM 8.0 and 8.1 may differ (`runner = "gm80" | "gm81"` in the manifest, **OPEN**).
- **Immediate mutation.** Built-ins and scripts change the world immediately, as GML does. Nothing is staged.
- **Room speed.** `room_speed` is sim state. The client runs steps at that rate with its own accumulator (not Bevy's `Time<Fixed>`, because the rate can change per room). Timing counts ticks. Displayed real time is the sum of `1 / room_speed` over the ticks.
- **Halting.** On a script error, the sim stops in `SimStatus::Halted` with the world frozen where it stopped.

## Number handling

GM8 reals are IEEE binary64 doubles. Our sim uses f64 for every real.

- **`Real` newtype.** Every float in built-ins goes through `sim::num::Real(f64)`. If the float model ever has to change, it changes in one place.
- **Exact operations only.** `+ - * /` and `sqrt` are correctly rounded IEEE on all three targets. Rust never contracts to FMA (RFC 3514). x86_64 uses SSE2.
- **Rounding.** `num::round` is banker's rounding (`f64::round_ties_even`), which matches Delphi and GM8. It places bounding boxes and converts reals to integers where GM8 rounds. `f64::round` (half away from zero) is banned. Truncation and other conversions are named helpers (`to_int_trunc`, `to_int_round`, `frac`, `gm_mod`, `gm_div`).
- **Comparisons.** `num::gm_eq/ne/lt/le/gt/ge` implement GM8's epsilon comparison (OpenGMK uses 1e-13 on the difference; **OPEN** until the CLONE_SPEC confirms it). Built-in code uses them wherever the spec says the original compared reals in GML. Scripts call the same functions by name ([0001](decisions/0001-script-language.md)).
- **Transcendental math.** `sin`, `cos`, `atan2`, `exp`, `ln`, `pow` and the rest are software functions from `libm` with no default features (no arch-specific paths), wrapped in `num`. They give the same bits on every platform.
- **Trig fidelity for built-ins.** GM8 runs on the x87 FPU, whose FSIN and FCOS differ from libm. The kid's gravity uses `gravity_direction` trig, and the drift this causes is a real trick (the "28 second trick", `research/knowledge/physics-core.md`). The traces already show it: with `gravity_direction = 270` and gravity 0.4, the kid's hspeed becomes `-7.347880794884119e-17` (`research/specs/engines/gm8/traces/rely-the-needle-4-2/djump.csv`). That trace was recorded under OpenGMK, which uses plain f64. So:
  - `num::dsin` and `num::dcos` check a small table of exact bit patterns for the angles built-ins use (0, 90, 180, 270 and any others found), and use libm for every other angle.
  - Each table entry records where its value came from: an OpenGMK trace, or a measurement on the real GM8 runner.
  - **OPEN:** whether the real runner gives the same bits as OpenGMK. Settled by a Phase 2 cross-check through the research gate. Scripts use plain libm trig, because bullets do not need x87 parity.
- **x87 fallback.** If traces show that GM8's extended-precision intermediates matter, the affected built-in formulas switch to `rustc_apfloat` X87DoubleExtended (pure Rust, the same on every platform) behind the same `Real` API, selected by `float_model = "x87_ext"` in the physics profile. Evidence so far says plain f64 is enough: OpenGMK dropped its x87 code in 2021 and still syncs TASes. **OPEN** until our own traces confirm it.
- **Literals.** Constants in Rust are decimal literals, which Rust rounds correctly, or `from_bits` when a trace demands it. Data files may write `{ bits = "..." }` for the same reason (Delphi may parse GML literals through extended precision).
- **NaN and infinity.** Setters reject non-finite values. The state hash maps every NaN to one bit pattern.
- **Motion coupling.** As in GM8, setting `hspeed` or `vspeed` recomputes `speed` and `direction`, and setting `speed` or `direction` recomputes `hspeed` and `vspeed`. The trace shows this: `speed = 7.347880794884119e-17`, `direction = 180` once vspeed is 0. Details (the 0.0001 snap, direction wrap) come from the CLONE_SPEC.

## Collision

- **Masks.** Each sprite frame has a `Mask`: a bounding box in sprite space and an optional bitset (rows as `u64` words) for precise masks. `crates/format` builds masks from PNG alpha at load, using the sprite's `collision` mode (`precise`, `rect`, `none`) and `alpha_tolerance`. The sim receives finished bitmasks; image data never reaches it.
- **Bounding box in the room.** Computed exactly as GM8 does: position, origin, scale and bbox offsets, then banker's rounding. The kid's box in all 20 games is the rectangle [12, 11, 22, 31] with origin (17, 23) (`research/shortlist.md`).
- **Precise test.** Integer pixels in the intersection of two boxes, mapped back through each instance's scale. Rotated precise masks are rejected by the validator in format v0 unless a CLONE_SPEC defines the rule (**OPEN**).
- **Broadphase.** One uniform grid per room, 64 px cells, each cell holding instance ids. Static instances (no motion, no trap link, no script, never destroyed) are bucketed once at room start. Moving instances re-bucket when their box changes. Anything outside the room bounds goes in a separate list.
- **Order never depends on the grid.** Query results are sorted by (rank, id) before use.
- **Queries.** `place_meeting`, `place_free`, `collision_point`, `collision_rect`, `collision_line`, `instance_place` and `move_contact_solid` follow GM8 semantics (1 px steps, step limits) per the CLONE_SPEC.
- **Scale.** 1,288 water tiles in one room (Fairy tale) and 28,900 instances over 116 rooms (qoqoqo) are well within this design. A benchmark covers both.
- **OPEN:** spike masks (precise triangle or bounding box) per engine.

## Scripts

Scripts are Rhai under the IWS profile ([0001](decisions/0001-script-language.md)). The sim talks to them only through the `ScriptHost` trait, so the sim core never names Rhai types.

- A scripted object's events run in the same pipeline slots as built-in events, in the same (rank, id) order.
- During a script call the world is lent to the Rhai host through an `Rc<RefCell<World>>`. Each API function borrows it briefly. Single thread, no `unsafe`, no `sync` feature.
- Built-ins call scripts only through defined events: `collide`, `trigger`, `animation_end`, timeline moments, and the `call` trigger action.
- The budget counter, caps and error handling are in [0001](decisions/0001-script-language.md#step-budget).

### Script API (first version)

This is the starting surface. The generated reference is `docs/format/script-api.md`, and the code is the authority. Every number the API returns is a FLOAT. Every function that takes a kind, sound, room or trigger name is checked at load when the name is a string literal.

**Event functions** a script file may define: `create()`, `begin_step()`, `step()`, `end_step()`, `alarm(n)`, `collide(other)`, `trigger(name)`, `outside_room()`, `animation_end()`, `room_start()`, `room_end()`, `destroy()`, `late()`, `moment_<name>()`, and the client-only `draw(d)`.

**`this` and other instances** (`Inst` handles):

- Read and write: `x`, `y`, `hspeed`, `vspeed`, `speed`, `direction`, `gravity`, `gravity_direction`, `friction`, `sprite`, `image_index`, `image_speed`, `image_xscale`, `image_yscale`, `image_angle`, `image_alpha`, `visible`, `solid`, `depth`.
- Read only: `xstart`, `ystart`, `xprevious`, `yprevious`, `kind`, `name`, `group`, `id`.
- Custom variables: `this.hp = 20.0`. Unset reads are errors.
- `this.param("hp")`: a parameter set on the instance in the room file or in the object file.

**Instances**

| Function | Notes |
|---|---|
| `spawn(kind, x, y)` and `spawn(kind, x, y, #{ ... })` | Returns the new `Inst`. The map sets built-in variables and parameters. |
| `destroy(i)` | |
| `exists(i)` | |
| `instances(kind_or_group)` | Array in (rank, id) order, snapshotted at call time. Replaces GML `with`. |
| `count(kind_or_group)` | |
| `nearest(kind, x, y)` | Ties broken by (rank, id). |
| `find(name)` | The instance with that `name` in the current room. |

**Collision**: `place_meeting(i, x, y, role_or_kind)`, `place_free(i, x, y)`, `collision_point(x, y, role_or_kind)`, `collision_rect(x1, y1, x2, y2, role_or_kind)`, `collision_line(x1, y1, x2, y2, role_or_kind)`. Each returns a bool or the first `Inst` in (rank, id) order.

**Kid**: `kid()` returns a read-only view (`x`, `y`, `hspeed`, `vspeed`, `facing`, `alive`, `on_ground`), or `()` if there is no kid. `kill_kid()`. `set_kid_gravity(dir)` for gravity flips. Other knobs are added only with a CLONE_SPEC reason.

**Rooms, triggers and timing**: `room_goto(room)`, `room_restart()`, `emit(trigger_name)`, `start_timeline(i, timeline)`, `stop_timeline(i)`, `set_alarm(n, ticks)`, `alarm_left(n)`, `tick()`, `room_tick()`, `room_speed()`.

**State**: `global_get(name)`, `global_set(name, v)`, `flag(name)`, `set_flag(name, v)` (flags go into the save slot).

**Input** (for menus and quizzes): `pressed(action)`, `held(action)`, `released(action)`.

**Audio and effects** (queued for the client, never read back): `play_sound(id)`, `play_music(id)`, `play_music(id, #{ from_tick: n })`, `stop_music()`, `shake(ticks, strength)`.

**Menus and text**: `text_box(...)`, `menu(...)`. Exact shape **OPEN**, from the quiz, battle menu and difficulty menu in the shortlist games.

**Random**: `random(x)`, `irandom(n)`, `random_range(a, b)`, `choose([...])`. All draw from the sim's GM8 generator.

**Math** (all in `sim::num`): `sin`, `cos`, `tan`, `atan2`, `dsin`, `dcos`, `darctan2`, `sqrt`, `pow`, `exp`, `ln`, `abs`, `sign`, `floor`, `ceil`, `round` (banker's), `frac`, `min`, `max`, `clamp`, `lerp`, `div`, `mod`, `point_direction`, `point_distance`, `lengthdir_x`, `lengthdir_y`, `gm_eq`, `gm_ne`, `gm_lt`, `gm_le`, `gm_gt`, `gm_ge`, `gm_bool`.

**Draw** (client only, inside `draw(d)`): `d.sprite(...)`, `d.rect(...)`, `d.text(...)`, `d.bar(...)`. No setters on anything.

## Random numbers

- One gameplay generator in `sim::rng`, compatible with GM8's Delphi-style 32-bit LCG, so rooms that use `random()` can match the original's traces. Constants and the float derivation come from the CLONE_SPEC (OpenGMK uses multiplier 0x08088405, increment 1, step 2^-32; **OPEN** until confirmed).
- The seed is in the replay header. It is the only randomness in the sim.
- Each built-in documents how many random draws it consumes, so the stream stays aligned with the original.
- Cosmetic randomness (blood, particles that do not affect gameplay) lives in the client with its own generator.
- **OPEN (user decision, Phase 8):** how ranked runs get their seed: random per run as in the originals, a fixed seed per game, or a server-issued seed. The server can flag reused seeds either way.

## Input and replay format

### Actions and input frames

- Actions: `left`, `right`, `up`, `down`, `jump`, `shoot`, `restart`, `suicide`, `skip`, `accept`, `back`, `menu`, plus reserved bits. One `u16` bit each.
- `InputFrame { held: u16, pressed: u16, released: u16 }`, one per tick. The edge bits keep sub-tick taps: GM8 sees a press and a release inside one frame as both events in the same step, which gives a different jump than holding for one tick.
- The client collects raw OS input events between ticks into these bits. Games map actions, never raw keys.
- Each kid rule set defines how it reads the bits (for example YoYoYo versions differ on sub-frame taps).

### Replay file (`.iwbr`)

A postcard-encoded, versioned struct. Postcard is a small, documented, stable binary encoding, so we do not hand-write a codec.

- **Header:** magic `IWBR`, `replay_format: u16`, `sim_version: u32`, package id, `gameplay_hash: [u8; 32]`, `engine_rev: u16`, rng seed `u64`, start mode (`NewGame` or `FromSave(bytes)`; only `NewGame` is ranked), client build id.
- **Body:** run-length encoded input: a list of `(run_length: varint, InputFrame)`.
- **Trailer:** total ticks, splits as `(stage index, tick)`, checkpoints as `(tick, [u8; 16])` (BLAKE3 state hash truncated to 16 bytes) every 500 ticks, and the final state hash.
- **Canonical.** The decoder re-encodes what it read and rejects the file if the bytes differ (for example two adjacent runs of the same frame). Identical inputs therefore always give identical bytes, which helps exact-copy detection.
- **Size.** An hour at 50 ticks per second is 180,000 ticks. RLE keeps that to a few KB, far under the server's 1 MiB cap.
- **Text form.** `iwbtg replay dump` prints one run per line for humans and diffs.

### Text inputs for tests (`.inputs`)

Tests and traces use the same line format as the research trace recorder:

```
frames 10-29: hold jump     # first jump, released on frame 30
frames 32-41: hold jump     # double jump
frame 109: end
```

The research files name original keys (`hold Shift`). `iwbtg trace run` maps keys to actions with the engine's default key map (for example Shift to `jump`). Our own test files use action names. Both compile to the same input frames.

### Verification

The server decodes the replay, checks bounds (ticks at most the game's `max_ticks`), looks up the package by gameplay hash, and runs the sim headless in a child process (no `draw`, no audio). It requires the end trigger, compares splits and the final tick with what was claimed, and stores the verified tick count and its own checkpoint hashes. It never trusts hashes from the client; they only help find where a replay desynced.

### Versions

- `SIM_VERSION` changes whenever a sim change could alter any outcome, including any Rhai upgrade.
- Fixes to a built-in that change outcomes keep the old path behind `engine_rev < N`, the way osu! keeps old scoring code. A package declares the `engine_rev` it was verified against. Each branch is covered by the corpus.
- For Rhai upgrades and other changes that cannot be branched, the server keeps old verifier builds for replays made with an older `SIM_VERSION`.
- Before any sim release, CI re-verifies the whole stored replay corpus.

## State hash

- BLAKE3 over a canonical little-endian walk of the world: tick, room, room speed, RNG state, instances in id order with every field (`Real` as `to_bits`, NaN canonicalized), custom variables in `BTreeMap` order, globals, flags, save slot, timeline and alarm state.
- No `usize`, no pointers, no hash-map order. The script log and the output queue are not hashed.
- Checkpoints store the first 16 bytes.

## Comparing traces with the original

Ground truth comes from the research repo. The Phase 2 recorder runs the original under OpenGMK with `iwc-trace.patch` and writes one CSV per case, next to its `.inputs` file, in `research/specs/engines/<engine>/traces/<game>/<case>.csv`. Selected frames are cross-checked against the real GM8 runner through the research gate.

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
- Every ported game needs at least one trace case per mechanic it uses before it is ranked.

## Testing strategy

| Layer | What | Where | When |
|---|---|---|---|
| Unit tests | `num` (rounding, comparisons, trig table), `rng`, motion coupling, masks and bounding boxes, collision queries, input edges, replay round trip | `crates/sim`, `crates/format` | every `cargo test` |
| Script conformance | Every IWS math function over 10,000 fixed inputs, every operator and overload, operation-count fixtures | `crates/sim/tests/script_conformance/` | every CI target |
| Validator fixtures | A set of broken packages, each expecting one specific error with file, line and column | `crates/format/tests/invalid/` | every `cargo test` |
| Engine test rooms | Small packages we make ourselves (no creator content) exercising each built-in, with golden hash lists | `crates/sim/tests/golden/` | every CI target |
| Trace tests | Every research trace case, run and diffed bit for bit | research path given by the `IWBTG_RESEARCH` environment variable; skipped when absent | locally, in a research CI job, before ranking a game |
| Replay corpus | Each ported game's full-clear replay, plus every stored leaderboard replay once Phase 8 exists | games repo, server | every sim change |
| Cross-platform equality | `iwbtg determinism report` on golden rooms, conformance suite and replay corpus on `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, `wasm32-wasip1` under wasmtime and `wasm32-unknown-unknown` through wasm-bindgen-test in Node. A final job requires all hash lists to be byte-identical. | GitHub Actions matrix | every PR touching `sim` or `format` |
| Fuzzing | Replay decoder, package loader, script host | `cargo-fuzz`, native | nightly |
| Budget and speed | Peak script units under 25% of the budget; benchmarks of the Rori Girl and Easy Miku 2 avoidances and a 28,900-instance load, native and WASM | `iwbtg profile`, `cargo bench` | CI, early in Phase 3 |
| Lints | `cargo clippy -D warnings` with the sim bans, `cargo deny check`, `cargo fmt --check` | CI | every PR |

The roadmap Phase 3 check is the trace-test row plus the cross-platform row: the Phase 2 inputs reproduce the original traces exactly on Windows, WASM and Linux x86_64.

## Open items

- **OPEN:** exact GM8 event order, collision handling for solid instances, destroy timing, room-change timing and 8.0 versus 8.1 differences. Source: `research/specs/engines/gm8/CLONE_SPEC.md`.
- **OPEN:** comparison epsilon, truthiness boundary, rounding and conversion rules, motion coupling details. Source: the same spec.
- **OPEN:** whether the real GM8 runner's trig and precision match OpenGMK's plain f64. Source: Phase 2 cross-check.
- **OPEN:** the built-in kind list and each kind's parameters. Source: `research/specs/mechanics.md` (not written yet).
- **OPEN:** whether any room really runs at a speed other than 50 (editor values 30, 60 and 5 exist, but all 20 games set 50 in code).
- **OPEN:** ranked RNG seed policy (user decision).
- **OPEN:** spike mask type and rotated precise masks.
