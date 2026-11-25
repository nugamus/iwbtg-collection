# The client: performance measurement

Part of [the client](../client.md).

`perf.rs` (#1024) measures the running client's frames and prints a report. It is the
client-frame counterpart of the headless sim profiler (#931): the sim's own cost is one line of
the report, measured separately, and a headless tick time is never turned into an FPS figure.

## Running it

```sh
cargo build -p iwbtg-client --release
target/release/iwbtg-client <package> --perf 600 --perf-warmup 120 --fps unlimited --mute \
    --perf-write perf-<case>.txt
```

- `--fps unlimited` matters: the default cap of 60 adds its sleep to every **frame period**,
  while the main-thread and sim lines stay honest. `--perf` and `--screenshot` cannot be
  combined.
- `--perf-warmup` counts **sim ticks** (default 120, 2.4 s at 50 Hz), so a churn room's
  population has reached equilibrium and shaders and caches are warm; at least 30 frames always
  run first. A halted sim never reaches the tick count and the run measures anyway after 20,000
  frames, saying so in the report.
- `--perf-write` keeps the report as a baseline artifact.
- The flags are hidden from `--help`, like the other preview modes.

## What each line measures

| Line | Is |
|---|---|
| `frame period ms` | frame start to the next frame start: the main app, the render schedule and the present call |
| `main-thread ms` | `First` to `Last`, every schedule of the main app (excludes the render sub-app and the present) |
| `sim ms per frame` | the time inside `view::tick`, all fixed ticks the frame ran, added up |
| `scene` | live sim instances and sprite entities at the first measured frame |
| `ticks` | sim ticks inside the measured window, and their rate over the measured wall time |

Percentiles are nearest-rank over the frames after the warmup (p50, p95, p99, min, mean, max).

## What it does not measure, and why

- **GPU and render-submit time.** The client sets up no timestamp queries, so there is no GPU
  number to read. The render sub-app's CPU cost is inside `frame period` but is not separated
  from it.
- **Allocation counts and bytes.** A counting `GlobalAlloc` needs `unsafe`, and the workspace
  forbids `unsafe_code`; no allocation-counter crate is vendored. It would take a dependency
  (for example `allocation-counter`) behind a cargo feature, or a lint exception for one module.
- **Process memory (RSS).** The OS calls are `unsafe` for the same reason, and `sysinfo` is not
  vendored.
- **Variance and reference machines.** These are one machine's numbers; they are advisory until
  reference machines exist. The budgets below are not yet agreed.

These gaps are printed in every report, so an artifact never looks like it measured more than it
did.

## The stress package

`crates/client/tests/perf/render.pkg` (#1024): original synthetic content, one 8×8 dot sprite,
no game assets, no RNG. A room's dots are `decor` objects, so each one costs a sprite and nothing
else in the sim.

- `stable_500`, `stable_2000`, `stable_5000`: a spawner fills the room once at Create, 250 dots a
  tick (the engine's per-call cap), then nothing moves.
- `churn_500`, `churn_2000`, `churn_5000`: a churner creates 5, 20 or 50 dots a tick and every
  dot destroys itself after 100 ticks, so the population sits at `rate × 100` with a constant
  spawn/destroy rate.

A package's start room is fixed in `game.toml`, so a measurement uses a copy:

```sh
for room in stable_500 stable_2000 stable_5000 churn_500 churn_2000 churn_5000; do
  cp -r crates/client/tests/perf/render.pkg "/tmp/perf-$room"
  sed -i "s/\"stable_500\"/\"$room\"/g" "/tmp/perf-$room/game.toml"
done
```

## The baseline (2026-11-25)

Machine: Intel Core Ultra 5 225H (integrated Intel Arc 130T, driver 101.8991), 30.9 GiB RAM,
Windows 11 10.0.26300, Vulkan backend, rustc 1.99.1, `cargo build --release` (opt-level 3,
`overflow-checks = true`), 1000×760 window, `--perf 600 --perf-warmup 120 --fps unlimited`.

<!-- RESULTS -->
