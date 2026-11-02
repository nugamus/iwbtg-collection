# 0001: Script language

Status: Accepted by Claude on the user's behalf, 2026-10-24. Reopened by the user and confirmed by Claude on the user's request, 2026-11-02 (see [Revisited](#revisited-2026-11-02)).

Related: [0002 data syntax](0002-data-syntax.md), [architecture](../architecture.md), [format v0](../format/v0.md), [format principles](../format/README.md).

## Context

Most game behavior is data: built-in objects with parameters (format principle 2). Scripts are the escape hatch for what built-ins cannot express: bosses, avoidance patterns, special traps, menus and quizzes (principle 3). Of the 20 prototype games, about 10 need boss scripts, 2 need music-synced avoidance scripts and 4 need menu or text scripts (`research/shortlist.md`).

The script language must:

- Use only the API we expose. No file, network, OS, clock or entropy access.
- Have a step budget per tick, so a script cannot hang the client or the server.
- Be deterministic: bit-identical results on x86_64 Windows (MSVC), WASM and x86_64 Linux (the server).
- Stop the game cleanly on any error. Never crash the client or the server.
- Run inside `crates/sim`, which has no Bevy, and build for `wasm32-unknown-unknown` (the Bevy web target).
- Be easy for agents to read, write and review. The user does not code.

Three proposals were written (conservative, determinism, authoring). Each covered the script language, data syntax, package layout and sim architecture. Three judges scored each proposal as a whole from 1 to 10.

## Options considered

### Languages

| Option | Verdict | Reason |
|---|---|---|
| Rhai 1.27 (pure Rust) | **Chosen by all three proposals** | Builds for wasm32-unknown-unknown with no C toolchain. `Engine::new_raw()` starts with no packages. Documented operation counting with `on_progress`. Hard limits on call depth, sizes and counts. Object maps are `BTreeMap`. Fuzzed, active, MIT OR Apache-2.0. |
| Luau via mlua 0.12.2 | Rejected | Strongest proven sandbox, but WASM only through wasm32-unknown-emscripten, which a wasm-bindgen Bevy build cannot use. C++, and its math calls the platform C libm. |
| Lua 5.4 via piccolo 0.3.3 | Rejected | Pure Rust with fuel, but "very experimental", sparse stdlib, last release 2024-06. |
| Rune 0.14.2 | Rejected | Has a budget, but async-centric, less documented, small user base. |
| Koto 0.16.1 | Rejected | Very small user base. |
| Starlark 0.14.2 | Rejected | Frozen values fit config, not per-tick mutable game state. No operation budget found. |
| Boa 0.22 (JavaScript) | Rejected | Only loop and recursion limits, no total instruction budget. |
| wasmi with fuel | Rejected | Principle 1 forbids WebAssembly in packages. |
| Our own language | Plan B | Most control over cost model and semantics, but needs a parser, checker, interpreter, error messages, LSP and docs, all maintained by agents, in a language no model has seen. |

### Judges' scores per proposal

Scores are determinism / sandbox safety / WASM / readability / porting speed / maturity, with the total.

| Proposal | Judge 1 | Judge 2 | Judge 3 | Sum |
|---|---|---|---|---|
| conservative | 8/8/9/8/8/8 = 49 | 8/8/9/8/7/8 = 48 | 8/8/9/8/8/9 = 50 | 147 |
| determinism | 9/9/9/7/6/8 = 48 | 9/9/9/6/6/8 = 47 | 9/9/9/7/6/7 = 47 | 142 |
| authoring | 7/7/9/9/9/8 = 49 | 7/8/8/9/9/8 = 49 | 7/7/8/9/9/6 = 46 | 144 |

All three proposals chose Rhai, so the scores decide the engine configuration, not the language. The conservative configuration wins overall. The determinism proposal scored highest on determinism and sandbox safety, and its sandbox details are grafted in. The authoring proposal scored highest on readability and porting speed, and its hooks and tooling are grafted in.

Points the judges raised that change the merged design:

- Overloading float `==` to GM8's epsilon (conservative) is a trap: Rhai's `switch` hashes case values and never calls `==`, so a float `switch` would compare exactly while `==` would not. Two of three judges prefer explicit GM comparison functions plus a lint.
- `**` and `**=` are separate tokens. Both reach `powf` and both must be disabled.
- `AST::walk` needs the Rhai `internals` feature. The validator runs on the client and the server, so `internals` must be on in every build, not only in tools.
- `no_closure` must be on, so script state stays plain data that the state hash covers.
- `sync` must stay off. It forces `Send + Sync` closures and adds locking for no benefit in a single-threaded sim.
- The GM8 RNG must be kept (not PCG32), or rooms that use `random()` cannot match the original's traces.
- On a script error, the sim freezes where it stopped. Rolling the world back to the start of the tick would need a snapshot every tick.

## Decision

**Rhai, pinned to one exact version, on a locked-down engine profile called IWS (I Wanna Script).** The format spec defines the script language as "Rhai <version> under the IWS profile".

### Version pin

- Target: Rhai 1.27.x, pinned with `=`. 1.27.0 shipped on 2026-10-21. Pin a 1.27.x release only once it is at least 14 days old (2026-11-04 for 1.27.0).
- 1.27 is needed for `compact_str`, which removes the MPL-2.0 `smartstring` dependency (advisory RUSTSEC-2026-0249). If sim code needs Rhai before 1.27.x has aged, use 1.26.1 without `compact_str` and switch later.
- The Rhai version is part of `SIM_VERSION`. Any Rhai upgrade is a sim change: it must pass the golden replay corpus with identical results and come with a budget-usage diff.
- Use the AST interpreter. Do not enable `grain` (the Grain VM) until the conformance suite shows identical results and operation counts.

### Cargo features

```toml
rhai = { version = "=1.27.N", default-features = false, features = [
  "std", "only_i64", "no_time", "no_module", "no_custom_syntax",
  "no_closure", "compact_str", "internals",
] }
```

- `default-features = false` drops `ahash/runtime-rng`, so no getrandom.
- `internals` is on in every build, because the validator walks the AST on the client and the server.
- `metadata` is enabled only through a `sim` cargo feature `api-docs`, used by the `iwbtg` CLI to generate docs. The client and server do not need it.
- Never enable: `unchecked` (removes all limits), `sync`, `grain`, `wasm-bindgen`, `no_std`.

### Engine setup

One function, `sim::script::iws_engine()`, builds the engine for the client, the server and the CLI. It is unit-tested and documented line by line.

- Start from `Engine::new_raw()`. Register only our own packages: core language, strings, arrays, maps, our math and our API. Never register `StandardPackage` or `BasicMathPackage`: their `sin`, `cos`, `pow` and `round` call std, which is platform-dependent, and their `round` rounds half away from zero.
- `set_optimization_level(OptimizationLevel::Simple)`, fixed, because the optimizer changes operation counts.
- `set_strict_variables(true)`, so undefined variables fail at compile time.
- `set_fast_operators(false)`, needed for the `/` overload below.
- Disable symbols: `eval`, `**`, `**=`. (`import` and `export` are already gone with `no_module`.)
- `on_print` and `on_debug` write to the sim log, never to stdout. The log is capped, is not part of the state hash, and the server discards it.
- Limits: call levels 32, expression depths 64 (global) and 32 (functions), string 4,096 bytes, array 10,000 items, map 1,024 keys, variables 256, functions 512.
- Fixed hashing seed through `RHAI_HASHING_SEED` in `.cargo/config.toml [env]`. It does not change script results. It makes builds reproducible.
- No module resolver. No file access ever happens during a run. Shared behavior goes into Rust built-ins, not shared scripts (principle 9).
- All script calls go through one narrow `ScriptHost` trait in `crates/sim`. The sim core never names Rhai types, so the language can be swapped later.

## Consequences

### Sandbox mechanism

The sandbox has four layers:

1. **By construction.** A script can only call functions we register. There is no IO, clock, OS, environment or randomness API to register by mistake: clippy and cargo-deny keep them out of `sim` (see [architecture](../architecture.md#sim-purity)).
2. **Limits.** Operation budget per call and per tick, Rhai size and depth limits, and host caps on instances and spawns.
3. **Panic backstop.** Every `Sim::step` runs inside `catch_unwind` on native builds (`panic = "unwind"` in every profile). Rhai's "Don't Panic" goal is the first line; this is the second.
4. **Server isolation.** The server verifies each replay in a child process with `env_clear()`, rlimit, a wall-clock timeout and no network (`docs/server.md` rule 2). Rhai is not formally audited as a security boundary, so this layer is required.

### Step budget

- One budget unit is one Rhai operation (one `on_progress` callback). Host functions that do real work charge extra units to the same counter: for example `spawn` 50, each instance returned by `instances()` 1, each collision query 20 plus 1 per candidate tested. The cost table lives in `docs/format/script-api.md`.
- Counting: the `on_progress(|ops| ...)` closure adds 1 to a tick-wide `Cell<u64>`, which resets at the start of every tick. It returns `Some("budget")` when the tick counter passes the tick budget or when `ops` (the per-call count) passes the call budget. Rhai then ends the call with `ErrorTerminated`.
- Defaults: tick budget 1,000,000 units across all script calls in a tick. Call budget 200,000 units per event call. A package may raise its tick budget in `game.toml` up to a hard cap of 2,000,000.
- Host caps: 256 script spawns per call, 4,096 per tick, 50,000 live instances per room.
- Exceeding any budget or cap is a deterministic script error. It can only happen to a runaway script, because of the CI rule below.
- CI rule: a script's measured peak use must stay under 25% of its package's tick budget over the package's test replays. `iwbtg profile <pkg> --replay <file>` reports peak units and milliseconds per tick. The validator warns at 10%.
- Operation counts are deterministic for a fixed Rhai version, optimization level and script. That is why the version is pinned and part of `SIM_VERSION`.

### Determinism rules

These are rules for the IWS engine, the script API and every Rust built-in.

1. **No platform float math.** `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `exp`, `ln`, `log*`, `powf`, `powi`, `cbrt`, `hypot` and `mul_add` from std are banned in `sim` (clippy `disallowed-methods`). Every such function exposed to scripts or used by built-ins is implemented in Rust in `sim::num` on `libm = { version = "=0.2.16", default-features = false }`, which turns off the `arch` feature so everything is pure software.
2. **Hardware only for exact operations.** `+ - * /` and `sqrt` on f64 are correctly rounded IEEE on all three targets. Rust never contracts to FMA. x86_64 uses SSE2. `sim` is never built for i686 or other x87 targets.
3. **No `**`.** `**` and `**=` are disabled. Use `pow(x, y)` from `sim::num`.
4. **Exact comparisons, GM comparisons by name.** Rhai `==`, `!=`, `<`, `<=`, `>`, `>=` stay exact IEEE. GM8 compares reals with an epsilon of 1e-13 on the difference (to confirm in the engine CLONE_SPEC). Scripts that port a GML comparison of non-integers use `gm_eq`, `gm_ne`, `gm_lt`, `gm_le`, `gm_gt`, `gm_ge`. The validator lints raw `==`/`!=` where an operand is a float literal or a real-valued property (`x`, `y`, speeds, `direction`, `image_index` and so on), and rejects `switch` with float case values. A missed call also shows up as a trace divergence.
5. **GML division.** `int / int` returns a FLOAT, as in GML (`1 / 2 == 0.5`). Integer division is the explicit `div(a, b)`, remainder is `mod(a, b)`, both with GM semantics. Integers are i64 (`only_i64`). Integer overflow and integer division by zero are script errors. Every number the API returns is a FLOAT.
6. **No truthiness.** Rhai `if` needs a bool, so a bare number in a condition is a type error at runtime and a lint at load. GML truthiness (a real counts as true from 0.5; exact boundary to confirm in the CLONE_SPEC) is the explicit `gm_bool(x)`.
7. **No NaN or infinity in the world.** Every host setter (positions, speeds, alarms, scales and so on) rejects a non-finite value with a script error. The state hash maps every NaN to one canonical bit pattern, because WASM NaN payloads are not deterministic.
8. **One randomness source.** `random(x)`, `irandom(n)`, `random_range(a, b)` and `choose([...])` draw from the sim's GM8-compatible generator, seeded from the replay header. There is no other entropy.
9. **No clock.** `no_time` removes timestamps. Scripts see `tick()` and `room_tick()` only. Audio functions only queue events for the client; the sim never reads the audio clock, so music sync is tick-based, as in the originals.
10. **Deterministic order.** Maps are `BTreeMap`. The API never returns a HashMap. `instances(kind)` returns handles in event order: `object_order` rank, then creation order.
11. **No hidden state.** With `no_closure`, script functions hold no state between calls. Function pointers cannot be stored in instance variables (the store rejects them). All persistent script state lives in sim instance variables and globals as plain values, so savestates and state hashes are complete.
12. **No locale or Unicode tables.** `to_upper`, `to_lower` and other case mapping are not registered, because their tables change with the Rust version.
13. **Pinned tools.** The Rust toolchain is pinned in `rust-toolchain.toml` to an exact stable. Never `target-cpu=native`.

### How scripts call the API

- A scripted object has one `.rhai` file of event functions. The file may contain only `fn` and `const` items at top level.
- The host calls an event function with `this` bound (`call_fn_with_options` with `bind_this_ptr`). `this` is an `Inst` handle: an instance id plus a generation check.
- Event functions, named after GM events so ports map one to one: `create()`, `begin_step()`, `step()`, `end_step()`, `alarm(n)`, `collide(other)` (only for kinds listed in the object's `collides_with`), `trigger(name)`, `outside_room()`, `animation_end()`, `room_start()`, `room_end()`, `destroy()`, `late()` and `moment_<name>()` for timeline entries.
- `late()` runs at the position of GM8's Draw event, after End Step, and may change state. It exists because some originals change gameplay state in Draw.
- `draw(d)` runs only in the client. It gets a draw-list builder and no setters, so it can never change sim state. The server never runs it.
- Built-in variables are registered getters and setters on `Inst`: `x`, `y`, `xstart`, `ystart`, `xprevious`, `yprevious`, `hspeed`, `vspeed`, `speed`, `direction`, `gravity`, `gravity_direction`, `friction`, `sprite`, `image_index`, `image_speed`, `image_xscale`, `image_yscale`, `image_angle`, `image_alpha`, `visible`, `solid`, `depth`, `kind`. Speed setters apply the GM8 motion coupling.
- Custom variables use Rhai's indexer property fallback: `this.hp = 20`. They live in the instance's variable map in the sim. Reading an unset variable is an error, as in GM8.
- The kid is read-only to scripts, apart from `kill_kid()` and documented knobs such as flipping gravity. Scripts never replace the kid's physics.
- The first API surface is listed in [architecture](../architecture.md#script-api-first-version). The generated reference is `docs/format/script-api.md`.

Example:

```rhai
fn create() { this.hp = 20.0; set_alarm(0, 50); }

fn alarm(n) {
    if n == 0 {
        for i in 0..12 {
            spawn("@orb", this.x, this.y, #{ speed: 4.0, direction: i * 30.0 });
        }
        set_alarm(0, 25);
    }
}

fn collide(other) {
    if other.kind == "kid_bullet" {
        this.hp -= 1.0;
        destroy(other);
        if gm_le(this.hp, 0.0) { emit("boss1_dead"); destroy(this); }
    }
}
```

### Validation before running

`crates/format` runs these checks on load, on the client, the server and in `iwbtg pkg check`:

- Compile every script with `Engine::compile` under the IWS profile, with strict variables.
- Walk the AST (`internals`): every called function is registered or defined in the same file; event function names and arities are valid; top-level items are only `fn` and `const`; no disabled symbols.
- String literals passed to the API (kinds, sounds, rooms, triggers) resolve to ids in the package.
- Lints: raw float `==`, float `switch`, bare number as a condition.
- Every error and warning carries file, line and column.

### Error handling

- Any `EvalAltResult` (type error, budget exhaustion, host argument error, size limit) becomes `SimError::Script { file, line, col, object, instance, event, tick, message }`.
- The sim stops in `SimStatus::Halted` and freezes the world where it stopped. It does not roll back the tick.
- The client shows an IWBTG-styled "This game stopped" screen with a "copy report" button and "back to game select".
- The server marks the replay `invalid: script_error@<tick>`.

### Tooling for agents

- `iwbtg script api-docs` generates `docs/format/script-api.md` and `.d.rhai` stubs for the Rhai language server from Rhai metadata, so the docs never drift from the code.
- `script-api.md` has a "Porting from GML" section: division, comparisons, truthiness, `with` (use `instances()`, which snapshots the list at call time), implicit instance variables, Draw logic (use `late()`).
- A short IWS cookbook: a boss, an avoidance wave from a timeline, a trap.

### Tests

- Conformance suite in `crates/sim/tests/script_conformance/`: every IWS math function over 10,000 fixed inputs, every operator and overload, and operation-count fixtures. It runs on every CI target and its hash lists must be byte-identical.
- Benchmarks of the Rori Girl and Easy Miku 2 avoidances, native and WASM, early in Phase 3.

### Costs we accept

- Rhai is a tree-walking interpreter, and `set_fast_operators(false)` makes every operator slower. WASM is about 30% slower again. Bullets therefore stay native (the built-in `projectile` kind); scripts only spawn and steer them. Repeated patterns get promoted to built-ins.
- Agents know Rhai less well than Lua. Generated docs, stubs, the cookbook and strict compile errors make up for it.
- GM comparisons are explicit calls, which is more verbose than GML.
- Operation counts tie replays to the Rhai version. The server keeps old verifier builds for replays made with an older `SIM_VERSION`.

### When to revisit

- Rhai fails the conformance suite on a target, or an upgrade cannot be pinned: move to plan B (our own language). Scripts only use the small host API, so a mechanical translator is feasible.
- Avoidance benchmarks miss the frame budget on WASM: evaluate the Grain VM behind the conformance suite and op-count fixtures.
- The explicit GM comparison style causes repeated porting bugs: reconsider operator overloads together with a `switch` ban.

## Open items

- **OPEN:** exact GM8 comparison epsilon, truthiness boundary and bitwise semantics. Source: `research/specs/engines/gm8/CLONE_SPEC.md` (not written yet).
- **OPEN:** final host cost table and cap values, set from the benchmarks.
- **OPEN:** confirm the Rhai 1.27.x release to pin on or after 2026-11-04, and run `cargo deny check licenses` on it.

## Evidence

Checked 2026-10-24 by the proposals and judges:

- Rhai: https://crates.io/crates/rhai (1.27.0, 2026-10-21), changelog https://github.com/rhaiscript/rhai/blob/main/CHANGELOG.md, features https://rhai.rs/book/start/features.html, operation limit https://rhai.rs/book/safety/max-operations.html, progress callback https://rhai.rs/book/safety/progress.html, raw engine https://rhai.rs/book/engine/raw.html, operator overloading https://rhai.rs/book/rust/operators.html, WASM builds https://rhai.rs/book/start/builds/wasm.html, Engine API https://docs.rs/rhai/1.27.0/rhai/struct.Engine.html. Rhai source: float `**` uses `powf` (src/func/builtin.rs), the math package calls std `sin` and `round` (src/packages/math_basic.rs).
- mlua README (WASM through emscripten only): https://github.com/mlua-rs/mlua. piccolo README: https://github.com/kyren/piccolo.
- libm features (`default = ["arch"]`): https://github.com/rust-lang/compiler-builtins/blob/main/libm/Cargo.toml.
- Rust float semantics: https://rust-lang.github.io/rfcs/3514-float-semantics.html and https://doc.rust-lang.org/std/primitive.f64.html.
- GM8 numeric facts (research reference only, GPL-2.0, never copied): OpenGMK `gm8emulator/src/math.rs` and `gml/value.rs`.

## Revisited 2026-11-02

The user asked whether Rhai is the right choice and left the decision to Claude. Rhai stays.

- **Luau** is better known to people and AI (Roblox), and faster. But it is C++. Bevy's web build targets `wasm32-unknown-unknown`, and the Rust Luau bindings (mlua) do not support that target. Its math also goes through the C library and the C++ compiler, so we would lose control over bit-identical results on Windows, WASM and Linux. Replay verification and the race ghosts depend on those results.
- **A language of our own** would give full control, but it costs a compiler, a debugger, docs and editor support before the first boss works. That is months of work that ports nothing.
- **Pure-Rust Lua implementations** such as piccolo are not production-ready.
- **Rhai** is pure Rust, builds for every target, counts operations for the step budget, and routes all math through our `sim::num` functions. Its weak points can be managed:
  - Speed: bullets, projectiles and patterns are native built-ins. Scripts only decide, and native code moves things.
  - Unfamiliarity: the generated API docs, `.d.rhai` stubs and a library of example scripts teach agents and people the dialect.
  - Non-coders: they never see Rhai. The editor's room painter, trigger wiring, timeline editor and boss builder write data, not scripts.
- **Swappable.** The script host sits behind the `ScriptHost` trait, so a later switch stays possible.
- **Checkpoint.** Re-check after the first boss and the first avoidance game are ported (Little Trap and Rori Girl): peak budget per tick, and how much agent effort the scripts took. Rhai is replaced only if one of those fails.
