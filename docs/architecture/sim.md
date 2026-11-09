# The simulation

Part of [Architecture](../architecture.md).

## Sim module layout

The layout and the owner of each file come from the [sim core plan](../plans/sim-core.md) (section 13). WP0 created every file with its final public signatures.

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

Purity is enforced by tools wherever a tool can see the mistake, so an agent that breaks a rule gets a build error, not a silent desync. The two rules no tool checks yet are marked **review rule** below.

- `crates/sim/clippy.toml`, `disallowed-methods`: `f64::{sin, cos, tan, sin_cos, asin, acos, atan, atan2, sinh, cosh, tanh, asinh, acosh, atanh, exp, exp2, exp_m1, ln, ln_1p, log, log2, log10, powf, powi, cbrt, hypot, mul_add, round}`; `f64::{min, max}` (signed-zero result unspecified, lowered differently on x86 and WASM; use `num::gm_min`/`gm_max`), `f64::clamp` (panics on NaN), `f64::{to_degrees, to_radians}`; `std::fs::{read, write, read_to_string, read_dir, metadata, symlink_metadata, exists, copy, rename, remove_file, remove_dir, remove_dir_all, create_dir, create_dir_all, canonicalize}`, `std::fs::File::{open, create}`, `std::io::{stdin, stdout, stderr}` (`stdout` also closes the `writeln!(std::io::stdout(), ..)` gap in `print_stdout`), `std::process::exit`, `std::env::{var, var_os, vars, vars_os, args, args_os, current_dir}`, `std::thread::{spawn, scope, sleep}`.
- `disallowed-types`: `std::collections::{HashMap, HashSet}`, `std::time::{Instant, SystemTime}`, `f32`, `std::fs::{File, OpenOptions, DirEntry}`, `std::net::{TcpStream, TcpListener, UdpSocket}`, `std::process::Command`, `std::thread::Builder`, `std::hash::{DefaultHasher, RandomState}`.
- `crates/format/clippy.toml` bans the same float methods and `HashMap`/`HashSet`, `f32`, `DefaultHasher`, `RandomState`: the loader computes gameplay data (mask rasterization, sorted variable ids, ranks, `gameplay_hash`) on the client (WASM) and the server (native). IO stays allowed there, because it reads packages.
- `lib.rs` denies `clippy::print_stdout`, `clippy::print_stderr`, `clippy::float_cmp` and `clippy::cast_possible_truncation`. Test-only file and environment reads carry a scoped `#[allow]` with a reason.
- **Review rule: GML comparisons go through `num::gm_*`.** `clippy::float_cmp` only sees raw `f64` operands (`a.get() == b.get()`), and it never covers `<` or `>`. `Real`'s own `==` compares bits (it keys ordered maps and makes state comparable), so `inst.m.hspeed() == Real::ZERO` compiles without a lint and is false for `-0.0`. Wherever the original compared reals in GML, built-ins and scripts call `gm_eq`..`gm_ge` with the run's `NumRules`; `Real ==` and `<` on `.get()` are only for our own bookkeeping. Reviewers check this, and `crates/sim/tests/lint_gm_compare.rs` (part of `cargo test`, so CI) enforces the part a text scan can see: in `src/builtins/` and `src/kid/` a line that compares with `.get()` or against a `Real::` constant fails unless it carries a `gm-compare: ok (<why>)` comment on the line or the line above. There is no baseline: every site is fixed or marked. The lint cannot see a `Real == Real` comparison that never calls `.get()`.
- Integer counters (`tick`, `room_tick`, `next_id`, deaths, time, grid cells) use `checked_*` and halt with `SimError::Overflow`; the RNG wraps by definition. `[profile.release] overflow-checks = true`, so debug tests and release servers behave the same.
- `usize` never appears in serialized or hashed state (wasm32 has a 32-bit `usize`).
- **Dependencies, checked per crate.** No crate that reads entropy, clocks, threads or the GPU at run time enters the sim's graph. `crates/sim/deny.toml` bans `rand`, `rand_core`, `rand_chacha`, `tokio`, `bevy`, `glam`, `wgpu` and `getrandom`, with every feature on and dev-dependencies left out; CI runs it as the `sim-deps` job, and locally `cargo deny --manifest-path crates/sim/Cargo.toml --config crates/sim/deny.toml check bans`. The root `deny.toml` cannot do this, because it checks the whole workspace graph, which will hold Bevy once the client crate exists (it keeps GPL code out: `cargo deny check licenses`). **The one exception:** `getrandom` 0.2 is in the sim's graph through `rhai -> ahash -> const-random -> const-random-macro`, at compile time only: rhai turns on `ahash`'s `compile-time-rng`, so a proc macro picks rhai's internal hash keys once per build. The config allows `getrandom` only as a dependency of `const-random-macro` (`wrappers`); any other path to it fails the check. Nothing draws entropy at run time, but those keys differ between builds, so WP9 checks that no script-visible result depends on rhai's hash-map order. To look at it: `cargo tree -p iwbtg-sim -i getrandom --target all`.
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

The sketch above is the original idea. The declared types are in `crates/sim/src/world/instance.rs` and `def/builtin.rs` ([plan](../plans/sim-core.md) 1.1, 1.5, 6.1): per-owner `KindState` parts (kid, built-in, script, probe), `ClassMask` collision classes instead of parent objects, alarms as reals, and the built-in list from `research/specs/mechanics.md`.

- **Storage.** Instances live in a `Vec<Instance>` in creation order, so it is sorted by id. Lookup by id is a binary search. The list is private to `world`: other code reads it through `World::instances` and changes an instance only through `World::get_mut`, which keeps the broadphase grid in step (see Collision). Event dispatch does not keep per-rank lists: each phase snapshots `events::dispatch_order` once (plan 1.3).
- **Creation and destruction.** New instances are appended. Destroy runs the Destroy event, then sets `alive = false`; the rest of the destroying event still runs, and the instance gets no later events (CLONE_SPEC tick loop, Confirmed (recorder)). Destroyed instances stay in the list until the end of the step (Confirmed, manual 403_05), so the sweep at the end of the tick removes them with `retain`, which keeps order. Whether queries still see a destroyed-this-step instance is R22.
- **Roles replace parent objects.** Collision queries ask for roles (`solid`, `killer`, `platform`, `water`, `vine`, `stops_bullets`, ...) instead of GM parent objects. Each kind declares its roles.
- **Package objects.** A package object (`@boss1`) is `KindState::Scripted` or inherits a built-in with `base = "projectile"`, `base = "trigger"` and so on. A scripted object with a base keeps the base's native behavior and roles; the script adds events. Collision with the kid works through roles, never through script code.
- **The kid** is a built-in. Scripts read it, can kill it and can set documented knobs (for example gravity flip). They never replace its physics.
- **Globals and save slot.** Game-wide variables persist across rooms, like GM globals. The save slot (room, position, gravity, flags) is part of the sim state. The client writes it to disk between sessions. Ranked replays always start from a new game.
- **Sampling the tick of a room change (#450).** `tick::run_tick` is `run_rows` (rows 0-14) then `finish_tick` (rows 15-16, the room change); `Sim::step_rows` and `Sim::finish_step` expose the split. The recorder's row of a room-change tick shows the old room's instances (kid/B.md Restart timing, CLONE_SPEC D/E retry), so the trace runner samples between the two when a room change is pending. `Sim::step` and the world after it are unchanged.

- **Retry, reset and saves (WP4b).** The `world` built-in calls `restart::run_restart` (R) and `run_reset` (F2) at its rank, in the phase `[engine.restart].trigger` names. `game_restart_via_boot` moves (or creates) the kid at the saved spot, counts a death when `death_on_restart = "save_counters"`, requests `game_restart` and sets `RunGlobals::boot_load`, the stand-in for the temp file: it survives the restart, `restart::boot_step` (world Step) counts `boot_ticks` down and on the last boot tick loads the slot and requests the saved room (timeline N, N+1, N+2). `boot_ticks = 0` is `Unsupported` (R17). The other kinds recreate the kid and request the saved room at once; showdown has no `game_restart` (R26: kid/A3.md wins over the rooms table). Kid-side extras (B's timer and `reverse`, showdown's `frozen`) wait for `kid::api`. `save::quantize` has six position models; `save_now` reads the kid through its instance only. `view::follow_update` runs the runner view follow (border halved when larger than half the view, clamp, speed limit; speed 0 freezes an axis).
- **Output queue.** Audio and visual effects (`play_sound`, `play_music`, screen shake, blood) are events pushed to `output`. The client consumes them. `Sim::step` clears `output` first, except the first step (tick 0), which keeps what `Sim::new` produced (the first room's start: music, sounds); read `output` after each step and never after `Sim::new`. The sim never reads anything back from audio or rendering.

## Tick pipeline

`Sim::step(InputFrame) -> StepResult` runs one tick (`tick::run_tick`). The order follows `research/specs/engines/gm8/CLONE_SPEC.md` ("tick loop", "motion", "sprites and animation", "input"); the [sim core plan](../plans/sim-core.md) section 2.1 is the detailed version. Phase placement that the spec marks Inferred or UNKNOWN is a field of `TickRules` (register ids in plan section 15), never an `if` scattered in built-ins. If the spec changes, the rule changes in that one field.

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

- **Dispatch order.** `events::dispatch_order` decides it (R2). Default: `object_order` rank, then creation order (Inferred); alternatives `Creation` and `DepthThenRankThenCreation`. Each phase snapshots the order once at its start. Instances created during a phase take part in the later phases of the tick. Whether they also run the current phase is R9: the spec now records (recorder) that a new instance runs the current phase when its object comes later in the order than the creator's, and not when it is the creator's own object (CLONE_SPEC tick loop, Confirmed (recorder): tr-room-03-b-log, a3-t17-b, t-evo-1). `TickRules::new_instances_join_phase` defaults to `JoinPhase::LaterInOrder`, which follows it; `Never` and `Always` stay as switches. "The first instance" of a query, the kid, view follow and saveGame always comes from `World::first_of` (R21).
- **Room change requests.** `room_goto`, `room_restart`, `game_restart` and `game_end` set one pending request (last wins, R13); the requesting handler finishes, then no further events run this tick (R8, `TickRules::room_request`).
- **Immediate mutation.** Built-ins and scripts change the world immediately, as GML does. Create runs inside `spawn`; Destroy runs inside `destroy`.
- **Room speed.** `room_speed` is sim state: the room file's speed on entry, 50 after the world's Room Start (R14). The client runs steps at that rate with its own accumulator (not Bevy's `Time<Fixed>`, because the rate can change per room). Timing counts ticks. Displayed real time is the sum of `1 / room_speed` over the ticks.
- **GM 8.0 and 8.1.** `RunnerRules::for_runner(runner, engine_rev)` returns the same rules for both until a trace shows a difference (U12, R11); any difference becomes a field, never a branch in built-ins.
- **Halting.** A script error, a non-finite value, an overflow, a broken invariant or an unsupported case sets `SimStatus::Halted(SimError)` and freezes the world where it stopped. Handlers return `Result<Flow, SimError>`; `events::dispatch` turns an error into `Ctx::halt`, which sets the status and returns `Flow::Halt`. A phase that gets `Halt` stops at once, and `tick::run_tick` returns without the sweep, the room change or `tick += 1`. The sim never panics on any input (wasm32-unknown-unknown aborts on panic).

## Number handling

GM8 reals are IEEE binary64 doubles. Our sim uses f64 for every real.

- **`Real` newtype.** Every float in built-ins goes through `sim::num::Real(f64)`. If the float model ever has to change, it changes in one place.
- **Exact operations only.** `+ - * /` and `sqrt` are correctly rounded IEEE on all three targets. Rust never contracts to FMA (RFC 3514). x86_64 uses SSE2.
- **Rounding.** `num::round` is banker's rounding (`f64::round_ties_even`), which matches Delphi and GM8. It places bounding boxes and converts reals to integers where GM8 rounds. `f64::round` (half away from zero) is banned. Truncation and other conversions are named helpers (`to_int_trunc`, `to_int_round`, `frac`, `gm_mod`, `gm_div`).
- **Comparisons.** `num::gm_eq/ne/lt/le/gt/ge` and `gm_bool` take the run's `NumRules`: an epsilon of 1e-13 on the difference by default, or exact comparison (N1, N2; the spec infers exact, the default is the recorder's value until a trace settles it). Built-in code uses them wherever the spec says the original compared reals in GML. Scripts call the same functions by name ([0001](../decisions/0001-script-language.md)).
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
- **Broadphase.** One uniform grid per room, 64 px cells, each cell holding instance ids, built at room entry. Anything outside the room bounds goes in a separate list. Position and image setters live on `Motion` and `Image`, which cannot reach the grid, so `World::get_mut` (the only way to change an instance) marks the instance as moved; every query also tests all moved instances, and `World::flush_grid` re-buckets them between handlers or phases. A stale bucket costs a test, never a missed instance (`collide/grid.rs`).
- **Order never depends on the grid.** Query results are sorted by (rank, id) before use.
- **Queries.** `place_meeting`, `place_free`, `collision_point`, `collision_rect`, `collision_line`, `instance_place` and `move_contact_solid` follow GM8 semantics (1 px steps, step limits) per the CLONE_SPEC.
- **Scale.** 1,288 water tiles in one room (Fairy tale) and 28,900 instances over 116 rooms (qoqoqo) are well within this design. A benchmark covers both.
- **OPEN:** spike masks (precise triangle or bounding box) per engine.

## Scripts

Scripts are Rhai under the IWS profile ([0001](../decisions/0001-script-language.md)). The sim talks to them only through the `ScriptHost` trait, so the sim core never names Rhai types.

- A scripted object's events run in the same pipeline slots as built-in events, in the same (rank, id) order.
- During a script call the world is lent to the Rhai host: `on_event` swaps a placeholder into `ctx.w` and parks the live world in the host (`RefCell<Option<World>>`). Each API function borrows it briefly; a function that needs a whole `Ctx` (spawn, destroy, kill) takes the world out, builds a `Ctx` and puts it back, so a spawned instance's Create can run its own script. The world is restored also on errors. Single thread, no `unsafe`, no `sync` feature.
- Built-ins call scripts only through defined events: `collide`, `trigger`, `animation_end`, timeline moments, and the `call` trigger action.
- The budget counter, caps and error handling are in [0001](../decisions/0001-script-language.md#step-budget).
- **The engine (WP9.1, `script/host.rs`, `script/api.rs`, `script/budget.rs`).** `script::host::Iws::new(NumRules)` builds the IWS engine: `Engine::new_raw()` with the core, arithmetic, logic, string, array and map packages only, `eval`, `**` and `**=` disabled, the 0001 limits, strict variables, the simple optimizer and fast operators off. The numeric API is `num` under the run's `NumRules` (`gm_eq`..`gm_ge`, `gm_bool`, `gm_repeat`, `div`, `mod`, rounding, libm transcendentals, `random`, `irandom`, `random_range`, `irandom_range`, `choose`, `tick`, `room_tick`); `/` on two integers is a FLOAT. **Rhai's built-in float comparisons (1.26.1 and 1.27.0) use a relative epsilon**, which is neither IEEE nor GM8's rule, so the engine overrides `== != < <= > >=` with exact IEEE (a test pins `0.1 + 0.2 == 0.3` false and `gm_eq` true). The RNG and the tick counters are lent per tick (`begin_tick`, `end_tick`), so the scripts' draws are the sim's draws. The tick budget (`budget.rs`: 1,000,000 units, 200,000 per call) counts one unit per Rhai operation and ends a runaway call with a deterministic error.
- **The host (WP9.2, `script/host.rs`, `script/inst.rs`).** `ScriptRuntime::new(def, rules)` compiles every script (a compile error is a load-time `SimError::Script` with the file and position) and keeps the functions each defines. `script::on_event` (called by `events::dispatch` after the base built-in) maps the event to a function (`create`, `begin_step`, `step`, `end_step`, `late`, `alarm(n)`, `user(n)`, `collide(other)`, `outside_room`, `intersect_boundary`, `animation_end`, `room_start`, `room_end`, `game_start`, `destroy`, `end_of_path`) and calls it with `this` bound to the instance; a function the script does not define is skipped. `ScriptRuntime::event_set(id)` gives the events a script handles (the loader merges it into the kind's `EventSet`; collision classes stay the kind's business). The instance API is `Inst` handles (ids are never reused, so a stale handle is a script error, and `exists(h)` tells) with the built-in fields as properties, the package's custom variables through the same syntax (`this.hp`, mapped by `GameDef::var_names`; an unknown name is an error, an unset one reads 0 when `uninit_reads_zero`), `set_alarm(n, steps)`, `get_alarm(n)`, `instance_create`, `destroy`, `exists`, `instances(kind)` (event order), `kid()`, `emit(name)` and `kill_kid()`. Scripts nest at most 8 deep. A script call may create at most 256 instances and all scripts 4,096 per tick (`budget::MAX_SPAWNS_PER_CALL`, `MAX_SPAWNS_PER_TICK`; a Create a spawn runs counts toward the spawning call); exceeding either halts the sim with a script error that names the cap, file, line and tick. No more than 50,000 instances are live at once (`world::store::MAX_LIVE_INSTANCES`, checked in `World::insert` for every spawn, script or built-in): the 50,001st halts the sim with `instance cap exceeded`. The tick loop calls `ScriptRuntime::begin_tick` at row 0 to reset the budget. An error becomes `SimError::Script` with `file: message (line, position) [kind, instance, event, tick]` and halts the sim.
- **The validator (WP9.3, `script/validate.rs`).** `script::validate::validate(&GameDef)` checks every script without running it and returns `Issue { script, line, column, severity, message }`; the format loader turns them into diagnostics (`scripts/boss.rhai:2:9: error: ...`), so the client, the server and `iwbtg pkg check` refuse the same scripts. Errors: the script does not compile (strict variables, `eval` and `**` disabled); a top-level item that is not `fn` or `const`; an event function (`step`, `alarm`, `moment_<name>`, ...) with the wrong argument count; a call of a function that is neither registered nor defined in the file, or with a wrong argument count; a string literal passed to `instance_create` or `instances`, or compared with `.kind`, that names no kind (`NAME_ARGS` also resolves sounds and rooms when an API function takes them); a call that draws from the RNG, changes state or calls a script function that does, in a right operand of `&&` or `||` (Rhai short-circuits, GML evaluates both; the left operand always runs, so it is fine; use `&` or `|` on bools, or evaluate into a variable first); `switch` on float case values (read from the token stream, because Rhai hashes case values away); a real-valued property or number function used as a condition (`if this.hspeed`; a number literal is already a Rhai compile error). Warning: raw `==` or `!=` where an operand is a float literal or a real property (use `gm_eq`/`gm_ne`). The walk runs on the AST the engine compiles with the simple optimizer, so code the optimizer removes (an empty `if`, an unused `let` of a pure call) is not checked. The function classification lists (`RNG_FNS`, `STATE_FNS`, `NUMBER_FNS`, `OTHER_PURE_FNS`, `REAL_PROPS`) are pinned to the registry by tests: a new API function must be added to one of them.
- **API descriptions (#364, `script/docs.rs`).** Each registered function, operator and property has a one-line description added next to its registration with `docs.add(name, text)` (overloads share one). `iwbtg script api-docs` prints them into `docs/format/script-api.md` and fails when a registered name has none, so a new API function cannot land undocumented.

### Script API (first version)

This is the starting surface. The generated reference is `docs/format/script-api.md`, and the code is the authority; a name marked *planned* below is not registered yet, and a script that calls it fails validation. Every number the API returns is a FLOAT. Every function that takes a kind, sound, room or trigger name is checked at load when the name is a string literal.

**Event functions** a script file may define: `create()`, `begin_step()`, `step()`, `end_step()`, `alarm(n)`, `collide(other)`, `trigger(name)`, `outside_room()`, `animation_end()`, `room_start()`, `room_end()`, `destroy()`, `late()`, `moment_<name>()`, and the client-only `draw(d)`.

**`this` and other instances** (`Inst` handles):

- Read and write: `x`, `y`, `hspeed`, `vspeed`, `speed`, `direction`, `gravity`, `gravity_direction`, `friction`, `sprite`, `image_index`, `image_speed`, `image_xscale`, `image_yscale`, `image_angle`, `image_alpha`, `visible`, `solid`, `depth`.
- Read only: `xstart`, `ystart`, `xprevious`, `yprevious`, `kind`, `name`, `group`, `id`.
- Custom variables: `this.hp = 20.0`. Unset reads are errors.
- Parameters: the map given to `instance_create` sets them. Reading a parameter set in the room file or the object file from a script is *planned* (there is no `param` function).

**Instances**

| Function | Notes |
|---|---|
| `instance_create(kind, x, y)` and `instance_create(kind, x, y, #{ ... })` (`spawn` is a reserved word in Rhai) | Returns the new `Inst`. The map sets built-in variables and parameters. |
| `destroy(i)` | |
| `exists(i)` | |
| `instances(kind)` | Array in event order, snapshotted at call time. Replaces GML `with`. Groups are *planned*. |
| `count(kind_or_group)` | *Planned.* |
| `nearest(kind, x, y)` | *Planned.* Ties broken by (rank, id). |
| `find(name)` | *Planned.* The instance with that `name` in the current room. |

**Collision** (*planned*): `place_meeting(i, x, y, role_or_kind)`, `place_free(i, x, y)`, `collision_point(x, y, role_or_kind)`, `collision_rect(x1, y1, x2, y2, role_or_kind)`, `collision_line(x1, y1, x2, y2, role_or_kind)`. Each returns a bool or the first `Inst` in (rank, id) order.

**Kid**: `kid()` returns the kid's `Inst` handle, or `()` if there is no kid (the view of `facing`, `alive` and `on_ground` is *planned*). `kill_kid()`. `set_kid_gravity(dir)` for gravity flips is *planned*. Other knobs are added only with a CLONE_SPEC reason.

**Rooms, triggers and timing**: `emit(name)`, `this.set_alarm(n, ticks)` (a method of the instance, so `other.set_alarm(...)` works too), `this.get_alarm(n)`, `tick()` and `room_tick()` are registered. *Planned*: `room_goto(room)`, `room_restart()`, `start_timeline(i, timeline)`, `stop_timeline(i)`, `room_speed()`.

**State** (*planned*): `global_get(name)`, `global_set(name, v)`, `flag(name)`, `set_flag(name, v)` (flags go into the save slot).

**Input** (*planned*; for menus and quizzes): `pressed(action)`, `held(action)`, `released(action)`.

**Audio and effects** (queued for the client, never read back): `play_sound(name)`, `play_music(name)` (a loop; another track stops or blocks it by the run's MULTIMEDIA rule), `stop_music()` and `shake(ticks, strength)` are registered; sound names are checked at load, and a pack sound is `engine:<pack>/<id>`. *Planned*: `play_music(name, #{ from_tick: n })`.

**Menus and text** (*planned*): `text_box(...)`, `menu(...)`. Exact shape **OPEN**, from the quiz, battle menu and difficulty menu in the shortlist games.

**Random**: `random(x)`, `irandom(n)`, `random_range(a, b)`, `irandom_range(a, b)`, `choose([...])`. All draw from the sim's GM8 generator.

**Math** (all in `sim::num`): `sin`, `cos`, `tan`, `arctan2`, `dsin`, `dcos`, `darctan2`, `sqrt`, `pow`, `exp`, `ln`, `abs`, `sign`, `floor`, `ceil`, `round` (banker's), `frac`, `min`, `max`, `clamp`, `lerp`, `div`, `mod`, `point_direction`, `point_distance`, `lengthdir_x`, `lengthdir_y`, `gm_eq`, `gm_ne`, `gm_lt`, `gm_le`, `gm_gt`, `gm_ge`, `gm_bool`, plus `sqr`, `degtorad`, `radtodeg`, `to_float`, `to_int` and `gm_repeat`.

**Draw** (*planned*; client only, inside `draw(d)`): `d.sprite(...)`, `d.rect(...)`, `d.text(...)`, `d.bar(...)`. No setters on anything.

## Random numbers

- One gameplay generator in `sim::rng`, compatible with GM8's Delphi-style 32-bit LCG, so rooms that use `random()` can match the original's traces. Constants and the float derivation come from the CLONE_SPEC (OpenGMK uses multiplier 0x08088405, increment 1, step 2^-32; **OPEN** until confirmed).
- The seed is in the replay header. It is the only randomness in the sim.
- Each built-in documents how many random draws it consumes, so the stream stays aligned with the original.
- **Hidden consumers draw in the sim.** In the originals, blood picks its frame and gravity with `choose` and `random`, and playerface draws `irandom(3)`, from the gameplay generator (CLONE_SPEC numbers, Confirmed). So the blood emitter, blood and playerface built-ins make those draws in the sim, in the original's order (blood Create draws subimage then gravity inside `spawn`, then the emitter draws direction and speed), and send the drawn values to the client as output events. Only drawing stays in the client.
- Particle systems (N10, `NumRules::particles_draw_rng`). The spec now records (recorder) that the runner draws from the gameplay generator for particles: 2 draws on most rows of rely room1, where its particle emitters run, and more when particles spawn (CLONE_SPEC numbers, Confirmed (recorder), num-rng-a.csv; the UNKNOWN entry is "partly answered"). Whether the real runner draws the same number is open. The engine_rev 0 default `false` predates that result, so rooms with particle emitters will not match the traces' RNG stream until WP11 changes the default and the particle built-ins make those draws.
- Ranked runs use a random seed per run, as in the originals; the server flags reused seeds (user decision, 2026-11-02, [product](../product.md)).

## State hash

- `hash::state_hash(&World) -> [u8; 32]` is BLAKE3 over the **postcard** encoding of a canonical `HashView` struct borrowed from `World`: every field of `World` except `output` and `grid`, in declaration order: tick, room, `room_tick`, room speed, instances in id order with every field, `next_id`, run-time `var_names`, globals (both in `BTreeMap` order), engine globals, RNG state, `input` (this tick's frame and the direct-read latch, which changes outcomes per I1), views, pending request, persistent rooms, save slot, sound state, timing state, status, `setup_applied`. A field added to `World` is hashed unless it is a cache. `Real` serializes its canonical bits, so every NaN hashes the same.
- Postcard writes lengths as varints, so wasm32 and x86_64 give the same bytes. `std::hash::Hash` and its hashers are never used for state hashing: slices write their length with `write_usize` (4 bytes on wasm32, 8 on x86_64), and `DefaultHasher`/`RandomState` are unspecified across Rust versions (both are banned in the sim).
- No `usize` in hashed state, no pointers, no hash-map order. The output queue and the broadphase grid are not hashed.
- `setup_applied` is hashed, so a trace run that used setup actions can never match a clean verification.
- Checkpoints store the first 16 bytes.
- `hash::HASH_LAYOUT` versions the encoding. Adding fields to a state struct (for example `KidGlobals`) changes the hash but not the layout: re-pin the vectors in `tests/hash_vectors.rs` and the final hash in `tests/wp1_determinism.rs`. Bump `HASH_LAYOUT` only when the encoding itself changes.

## Sound model

Audio output is the client's job, but whether a sound **is playing** is sim state. 15 of the 20 games call `sound_isplaying` (CLONE_SPEC sound, Confirmed), and its result can change game state, not only audio: machinary's playMusic2 sets an instance flag from it, and Step-based music objects call `sound_stop_all` when it returns false (Inferred). The DLL getters matter too: the GMFMODSimple getters give the start value, and so the lifetime, of the fader objects in go-the-spook-house and keep-celebrating-for-xia, and the SuperSound volume getter gives the start of kaenbyou's and eat-the-lemon's death fade. So `sim::sound` keeps a model ([plan](../plans/sim-core.md) section 7):

- Playing copies start at the call's tick and end after the sound's `length_ticks` (song length times tick rate, from the package). At most one MULTIMEDIA copy. `sound_isplaying` latency and the second MULTIMEDIA start are UNKNOWN switches in `SoundRules` (S1-S3); game_restart keeps sounds (Inferred, S4).
- SuperSound (kaenbyou, eat-the-lemon) and GMFMODSimple (go-the-spook-house, xia) are adapters whose getters return the last value set (S5). `SimOptions::dll_mode = Stub` reproduces the recorder's `--stub-dlls` traces; `Model` is for play and replays.
- The `mute` action flips the game's music flag through the world built-in; music objects then call `stop_all`, which is sim state.
- Built-in copies play the ticks `start .. start + length_ticks` (looping: until stopped). `sound::is_playing` computes that itself; tick row 16 (`sound::tick`) only drops ended copies, SuperSound sounds and FMOD instances. S1 (latency) delays only a start; S2 and S3 govern a second MULTIMEDIA start; S4 (`keep_on_restart`) is read by `game_restart`. The built-in model runs in `Stub` mode too: the recorder stubs only the DLLs.
- DLL adapters (`sound::{supersound, gmfmod, cleanmem}`) are called by the script host with the arguments of the GML `external_call`. SuperSound returns strings (`"1"`, `"0"`, `"-1"` for is-playing); FMOD returns numbers (0 is an error); in `Stub` mode every call returns `""` or 0 and changes nothing. The DLLs do not know a sound length, so the host passes `length_ticks` at load (0 = unknown, plays until stopped). OPEN: where the package supplies those lengths (follow-up).
- The state is hashed. The client receives `SoundOutput` events to play, stop and fade; nothing is read back from audio.
