# The simulation: Module layout and purity

Part of [the simulation](../sim.md).

## Sim module layout

The layout and the owner of each file come from the [sim core plan](../../plans/sim-core.md) (section 13). WP0 created every file with its final public signatures.

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
  stripped.rs     Play mode (original or stripped), the resolved [stripped] table (#561), regions (#34)
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
