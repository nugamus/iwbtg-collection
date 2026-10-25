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

The layout and the owner of each file come from the [sim core plan](plans/sim-core.md) (section 13). WP0 created every file with its final public signatures.

```
crates/sim/src/
  lib.rs          Module list, crate lints, SIM_VERSION
  sim.rs          Sim, SimOptions { start, seed, dll_mode }, StepResult
  error.rs        SimError: NonFinite, Overflow, Unsupported, Script
  num/            Real newtype, GM8 rounding and comparisons, soft math on libm, trig table
  rng.rs          GM8-compatible random generator (the only randomness)
  input.rs        Action, InputFrame, .inputs parser, KeyMap
  input_state.rs  This tick's frame and the keyboard_check_direct latch
  def/            Immutable game definition (GameDef) built by crates/format; GameDefBuilder
  rules/          RunnerRules: every UNKNOWN/Inferred rule as a switch (tick, motion, collide, room, num, sound)
  world/          World, Instance, the Vec instance store, Ctx (spawn, destroy, request_room, emit)
  events.rs       EventKind, EventSet, dispatch order, phases, collision phase
  tick.rs         The tick pipeline
  alarm.rs        Alarm countdown
  anim.rs         Image variables and the animation update
  timeline.rs     Timeline moments
  motion.rs       GM8 speed, direction, friction and gravity model
  path.rs         Path following
  collide/        Placed masks, overlap, queries, contact moves, solid put-back, broadphase grid
  room.rs         Room change sequence, StartPoint, start_game
  restart.rs      Restart (R) and reset (F2) models
  save.rs         Save slot and saved-position quantization
  view.rs         Views and runner view follow
  run_globals.rs  Difficulty, deaths, time, clear flag
  kid/            Kid built-in: state, api for other built-ins, presets, actions, collision runs
  builtins/       One file per built-in kind (block.rs, spike.rs, save.rs, ...)
  sound.rs        Sound model (is a sound playing) and DLL adapters (sound/)
  script/         ScriptHost trait, the Rhai host, API registration, budget, validator
  timing.rs       Start, end and stage triggers, splits
  output.rs       Audio and visual event queue for the client
  probe.rs        Read-only trace sampling and the GM name registry (probe/)
  setup.rs        Trace setup actions (feature "trace-setup", never in the server)
  replay.rs       Replay file encode and decode
  trace.rs        Trace CSV parse, write and bit-exact diff
  hash.rs         Canonical state hash
```

Each built-in is implemented once in `builtins/<kind>.rs` and documented once in `docs/format/builtins/<kind>.md`: parameters, behavior, which event phase it runs in, which games use it, and the CLONE_SPEC section it comes from.

### Sim purity

Purity is enforced by tools, not by review. An agent that breaks a rule gets a build error, not a silent desync.

- `crates/sim/clippy.toml`, `disallowed-methods`: `f64::{sin, cos, tan, asin, acos, atan, atan2, sinh, cosh, tanh, exp, exp2, exp_m1, ln, ln_1p, log, log2, log10, powf, powi, cbrt, hypot, mul_add, round}`; `f64::{min, max}` (signed-zero result unspecified, lowered differently on x86 and WASM), `f64::clamp` (panics on NaN), `f64::{to_degrees, to_radians}`; `std::fs::{read, write, read_to_string, read_dir}`, `std::fs::File::{open, create}`, `std::io::stdin`, `std::process::exit`, `std::env::var`, `std::thread::spawn`.
- `disallowed-types`: `std::collections::{HashMap, HashSet}`, `std::time::{Instant, SystemTime}`, `f32`, `std::fs::{File, OpenOptions}`, `std::net::{TcpStream, TcpListener, UdpSocket}`, `std::process::Command`, `std::hash::{DefaultHasher, RandomState}`.
- `lib.rs` denies `clippy::print_stdout`, `clippy::print_stderr`, `clippy::float_cmp` (raw `==` on reals; built-ins compare through `num::gm_*`) and `clippy::cast_possible_truncation`. Test-only file reads carry a scoped `#[allow]` with a reason.
- Integer counters (`tick`, `room_tick`, `next_id`, deaths, time, grid cells) use `checked_*` and halt with `SimError::Overflow`; the RNG wraps by definition. `[profile.release] overflow-checks = true`, so debug tests and release servers behave the same.
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

The sketch above is the original idea. The declared types are in `crates/sim/src/world/instance.rs` and `def/builtin.rs` ([plan](plans/sim-core.md) 1.1, 1.5, 6.1): per-owner `KindState` parts (kid, built-in, script, probe), `ClassMask` collision classes instead of parent objects, alarms as reals, and the built-in list from `research/specs/mechanics.md`.

- **Storage.** Instances live in a `Vec<Instance>` in creation order, so it is sorted by id. Lookup by id is a binary search. Each rank keeps a `Vec<InstId>` in creation order, for event dispatch.
- **Creation and destruction.** New instances are appended. Destroy sets `alive = false`. A dead instance runs no further events. The sweep at the end of the tick removes dead instances with `retain`, which keeps order. (GM8 removes destroyed instances after the event that destroyed them; exact timing **OPEN**, from the CLONE_SPEC.)
- **Roles replace parent objects.** Collision queries ask for roles (`solid`, `killer`, `platform`, `water`, `vine`, `stops_bullets`, ...) instead of GM parent objects. Each kind declares its roles.
- **Package objects.** A package object (`@boss1`) is `KindState::Scripted` or inherits a built-in with `base = "projectile"`, `base = "trigger"` and so on. A scripted object with a base keeps the base's native behavior and roles; the script adds events. Collision with the kid works through roles, never through script code.
- **The kid** is a built-in. Scripts read it, can kill it and can set documented knobs (for example gravity flip). They never replace its physics.
- **Globals and save slot.** Game-wide variables persist across rooms, like GM globals. The save slot (room, position, gravity, flags) is part of the sim state. The client writes it to disk between sessions. Ranked replays always start from a new game.
- **Output queue.** Audio and visual effects (`play_sound`, `play_music`, screen shake, blood) are events pushed to `output`. The client consumes them. The sim never reads anything back from audio or rendering.

## Tick pipeline

`Sim::step(InputFrame) -> StepResult` runs one tick (`tick::run_tick`). The order follows `research/specs/engines/gm8/CLONE_SPEC.md` ("tick loop", "motion", "sprites and animation", "input"); the [sim core plan](plans/sim-core.md) section 2.1 is the detailed version. Phase placement that the spec marks Inferred or UNKNOWN is a field of `TickRules` (register ids in plan section 15), never an `if` scattered in built-ins. If the spec changes, the rule changes in that one field.

| # | Phase | Status | Isolation |
|---|---|---|---|
| 0 | Latch input: build held/pressed/released for the tick; `direct_latch |= pressed` per key | Inferred (sample once before Begin Step) | `InputState::latch` |
| 1 | Capture `xprevious/yprevious` if `prev_capture = TickStart` | UNKNOWN U1 (R1) | `TickRules::prev_capture` |
| 2 | Begin Step | Confirmed order | |
| 3 | Timelines advance, moments run | UNKNOWN U6 (R6) | `TickRules::timeline_slot = AfterBeginStep` (Design) |
| 4 | Alarms: for each alarm the object handles, if value > 0 subtract 1, if now 0 fire | Confirmed order; rule Inferred (R3) | `alarm::tick` |
| 5 | Keyboard events: held, then press, then release (keys in action-bit order) | Confirmed slot; inner order UNKNOWN U11 (R4) | `events::key_event_order()` |
| 6 | Mouse events | Confirmed slot; no gameplay use in the 20 games | no-op phase kept for order |
| 7 | Step | Confirmed | |
| 8 | Motion for every instance: (capture prev if `BeforeMotion`), friction, gravity vector, path, `x += hspeed`, `y += vspeed`, polar recompute | Slot Confirmed; inner order Inferred | `motion::apply_auto_motion` |
| 8b | Animation if `anim_slot = AfterMotion` | Inferred, sprites section (Conflict with row 13, R7) | `anim::advance` |
| 9 | Outside Room, Intersect Boundary | Inferred slot (U4, R5) | `TickRules::boundary_slot` |
| 10 | Collision events with the solid put-back rule | Confirmed (manual); pairing Inferred/UNKNOWN (C7, C7b, C8, C9, R12) | `events::collision_phase`, `collide::solid_resolve` |
| 11 | End Step | Confirmed | |
| 12 | Draw slot (`Late` handlers): instances in draw order (depth descending, then creation), visible instances only | Confirmed; Draw changes state, so it runs headless too (R15) | `events::run_draw_phase` |
| 13 | Animation if `anim_slot = AfterDraw`: `image_index += image_speed`, wrap, Animation End | Inferred (Conflict with row 8b, R7); wrap `>=` and subtract once: Trace t-anim-1 | `anim::advance` |
| 14 | Destroy sweep | Design (manual: removed at end of step) | `World::sweep` |
| 15 | Room change if requested | Confirmed sequence; abort rule Inferred (R8) | `room::perform_change` |
| 16 | `tick += 1` (checked), timing triggers, checkpoint hash every 500 ticks | Design | `tick.rs`, `timing.rs`, `hash.rs` |

The trace recorder samples after row 13 and before row 15. The previous version of this table (xprevious at tick start, timelines before alarms, no Draw slot) is replaced by this one.

Rules that apply to every step:

- **Dispatch order.** `events::dispatch_order` decides it (R2). Default: `object_order` rank, then creation order (Inferred); alternatives `Creation` and `DepthThenRankThenCreation`. Each phase snapshots the order once at its start; instances created during the phase join the later phases of the tick (R9). "The first instance" of a query, the kid, view follow and saveGame always comes from `World::first_of` (R21).
- **Room change requests.** `room_goto`, `room_restart`, `game_restart` and `game_end` set one pending request (last wins, R13); the requesting handler finishes, then no further events run this tick (R8, `TickRules::room_request`).
- **Immediate mutation.** Built-ins and scripts change the world immediately, as GML does. Create runs inside `spawn`; Destroy runs inside `destroy`.
- **Room speed.** `room_speed` is sim state: the room file's speed on entry, 50 after the world's Room Start (R14). The client runs steps at that rate with its own accumulator (not Bevy's `Time<Fixed>`, because the rate can change per room). Timing counts ticks. Displayed real time is the sum of `1 / room_speed` over the ticks.
- **GM 8.0 and 8.1.** `RunnerRules::for_runner(runner, engine_rev)` returns the same rules for both until a trace shows a difference (U12, R11); any difference becomes a field, never a branch in built-ins.
- **Halting.** A script error, a non-finite value, an overflow or an unsupported case sets `SimStatus::Halted(SimError)` and freezes the world where it stopped. The sim never panics on any input (wasm32-unknown-unknown aborts on panic).

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
- **Rules, not constants.** Every number rule with OpenGMK provenance or an open spec status (compare epsilon, truthiness, `sign`, `min`/`max` ties, signed zeros, alarm and byte conversions, `string()` rounding, float model) is a field of `NumRules` in `RunnerRules`, never a `const`, so a rule settled later becomes a new `engine_rev` (plan section 3.2).
- **NaN and infinity** (plan 10.2, N15). `Real`'s `Eq` compares raw bits, and NaN payloads and signs differ between x86_64, wasmtime and browser engines. So every `Motion`, `Image`, alarm, view and variable setter, called from a built-in or a script, rejects a non-finite value with `SimError::NonFinite`, which halts the sim. Intermediate non-finite values inside one `num` function are allowed only if the function's result is finite. The state hash also maps every NaN to one bit pattern.
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
- **Hidden consumers draw in the sim.** In the originals, blood picks its frame and gravity with `choose` and `random`, and playerface draws `irandom(3)`, from the gameplay generator (CLONE_SPEC numbers, Confirmed). So the blood emitter, blood and playerface built-ins make those draws in the sim, in the original's order (blood Create draws subimage then gravity inside `spawn`, then the emitter draws direction and speed), and send the drawn values to the client as output events. Only drawing stays in the client.
- Particle systems: whether they draw from the gameplay generator is UNKNOWN (N10, `NumRules::particles_draw_rng`, default false as a placeholder). Until a trace shows otherwise, particle effects are client output only.
- **OPEN (user decision, Phase 8):** how ranked runs get their seed: random per run as in the originals, a fixed seed per game, or a server-issued seed. The server can flag reused seeds either way.

## Input and replay format

### Actions and input frames

- Actions: `left`, `right`, `up`, `down`, `jump`, `shoot`, `restart`, `suicide`, `skip`, `accept`, `back`, `menu`, `pause` (P), `mute` (Ctrl+M), `reset` (F2), plus one reserved bit. One `u16` bit each. Pause, mute and reset change sim state in the originals (kaenbyou's jump ignores a press while P is pressed, mute changes `sound_stop_all` calls, F2 restarts the game), so they are sim input. Quitting (Escape, Alt+F4) stays client-only.
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

- `hash::state_hash(&World) -> [u8; 32]` is BLAKE3 over the **postcard** encoding of a canonical `HashView` struct borrowed from `World`: every hashed field in a fixed order (tick, room, room speed, RNG state, instances in id order with every field, custom variables and globals in `BTreeMap` order, engine globals, views, pending request, persistent rooms, save slot, sound state, status, `setup_applied`). `Real` serializes its canonical bits, so every NaN hashes the same.
- Postcard writes lengths as varints, so wasm32 and x86_64 give the same bytes. `std::hash::Hash` and its hashers are never used for state hashing: slices write their length with `write_usize` (4 bytes on wasm32, 8 on x86_64), and `DefaultHasher`/`RandomState` are unspecified across Rust versions (both are banned in the sim).
- No `usize` in hashed state, no pointers, no hash-map order. The output queue and the broadphase grid are not hashed.
- `setup_applied` is hashed, so a trace run that used setup actions can never match a clean verification.
- Checkpoints store the first 16 bytes.

## Sound model

Audio output is the client's job, but whether a sound **is playing** is sim state: `sound_isplaying` changes game state in 15 of the 20 games (CLONE_SPEC sound, Inferred), and DLL getters decide fader object lifetimes in four. So `sim::sound` keeps a model ([plan](plans/sim-core.md) section 7):

- Playing copies start at the call's tick and end after the sound's `length_ticks` (song length times tick rate, from the package). At most one MULTIMEDIA copy. `sound_isplaying` latency and the second MULTIMEDIA start are UNKNOWN switches in `SoundRules` (S1-S3); game_restart keeps sounds (Inferred, S4).
- SuperSound (kaenbyou, eat-the-lemon) and GMFMODSimple (go-the-spook-house, xia) are adapters whose getters return the last value set (S5). `SimOptions::dll_mode = Stub` reproduces the recorder's `--stub-dlls` traces; `Model` is for play and replays.
- The `mute` action flips the game's music flag through the world built-in; music objects then call `stop_all`, which is sim state.
- The state is hashed. The client receives `SoundOutput` events to play, stop and fade; nothing is read back from audio.

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

Every GM rule the sim needs that the spec marks UNKNOWN, Inferred or Conflict is tracked with its switch, default and settling trace in the [plan's register](plans/sim-core.md#15-unknown-and-inferred-register). The items below are the broad ones.

- **OPEN:** exact GM8 event order, collision handling for solid instances, destroy timing, room-change timing and 8.0 versus 8.1 differences. Source: `research/specs/engines/gm8/CLONE_SPEC.md`.
- **OPEN:** comparison epsilon, truthiness boundary, rounding and conversion rules, motion coupling details. Source: the same spec.
- **OPEN:** whether the real GM8 runner's trig and precision match OpenGMK's plain f64. Source: Phase 2 cross-check.
- **OPEN:** the built-in kind list and each kind's parameters. Source: `research/specs/mechanics.md` (not written yet).
- **OPEN:** whether any room really runs at a speed other than 50 (editor values 30, 60 and 5 exist, but all 20 games set 50 in code).
- **OPEN:** ranked RNG seed policy (user decision).
- **OPEN:** spike mask type and rotated precise masks.
