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
`overflow-checks = true`), 1000x760 window, `--perf 600 --perf-warmup 120 --fps unlimited --mute`,
built from `db9f378` (main after #1490). All times are ms.

| case | instances | sim ticks/s | frame period p50 / p95 / p99 | main-thread p50 / p95 / p99 | sim p50 / p95 / p99 |
|---|---|---|---|---|---|
| `churn_2000-off` | 1985 | 44.6 | 21.74 / 178.21 / 262.89 | 20.56 / 177.09 / 261.32 | 14.48 / 166.83 / 248.87 |
| `churn_2000-on` | 1985 | 37.9 | 20.68 / 277.78 / 477.78 | 19.57 / 276.35 / 475.15 | 13.95 / 263.52 / 458.24 |
| `churn_500-off` | 500 | 49.4 | 5.83 / 11.17 / 12.50 | 3.94 / 9.90 / 10.92 | 0.00 / 5.43 / 6.17 |
| `churn_500-on` | 500 | 49.2 | 9.53 / 16.01 / 18.17 | 6.35 / 13.82 / 15.96 | 0.00 / 7.59 / 8.82 |
| `churn_5000-off` | 4955 | 20.8 | 386.39 / 536.09 / 633.16 | 384.77 / 533.85 / 630.03 | 373.09 / 516.17 / 614.22 |
| `churn_5000-on` | 4955 | 24.9 | 330.96 / 506.45 / 703.43 | 329.31 / 504.50 / 701.25 | 318.26 / 491.13 / 681.84 |
| `stable_2000-off` | 2005 | 49.3 | 11.01 / 14.09 / 15.81 | 9.62 / 12.45 / 13.86 | 3.77 / 5.31 / 5.97 |
| `stable_2000-on` | 2005 | 49.4 | 12.38 / 15.65 / 17.47 | 10.94 / 14.05 / 15.85 | 4.29 / 5.96 / 6.67 |
| `stable_500-off` | 505 | 49.5 | 6.24 / 8.25 / 9.03 | 4.28 / 6.69 / 7.36 | 0.00 / 1.62 / 1.88 |
| `stable_500-on` | 505 | 49.4 | 7.43 / 10.48 / 14.62 | 5.36 / 8.34 / 9.65 | 0.00 / 2.00 / 2.41 |
| `stable_5000-off` | 5005 | 49.1 | 17.85 / 21.91 / 28.55 | 16.71 / 20.45 / 25.78 | 8.54 / 10.78 / 15.45 |
| `stable_5000-on` | 5005 | 49.1 | 17.16 / 21.39 / 26.52 | 16.03 / 19.94 / 24.02 | 7.89 / 10.12 / 13.15 |

Reading it:

- **The client's own per-frame work** (main-thread minus sim) is about 4.3 ms at 500 visible
  instances, 5.9 ms at 2,000 and 8.2 ms at 5,000 with a stable population: roughly 1.6 ms per
  1,000 instances beyond the first 500. The rest of the main-thread time is `view::tick`, whose
  `sim ms` line includes the interpolation snapshot when `--interpolate` is on (that snapshot is
  client work inside the tick).
- **500 and 2,000 stable hold 60 fps** on this machine (main-thread p50 4.3 and 9.6 ms, p99 7.4
  and 13.9 ms, under the 16.7 ms budget). **5,000 stable sits on the edge**: p50 16.7 ms, p99
  25.8 ms, so it misses 60 fps in most frames.
- **Churn is sim-bound, not render-bound.** At 2,000 churn instances the sim is 14.5 ms of a
  20.6 ms frame; at 5,000 it is 373 ms of 385 ms. That is the script layer of the #990 content (a
  Step call per live instance plus 5-50 creates and destroys a tick), not the client's draw path,
  and it is the sim profiler's subject (#931), not this harness's.
- **Once the sim misses the tick rate, frame cost becomes bursty.** The churn cases cannot hold
  50 ticks/s (44.6/s at 2,000, 20.8/s at 5,000): the fixed clock falls behind, each frame runs
  several ticks to catch up, and the frame cost cascades — at 2,000 the p95 is 178 ms against a
  20.6 ms p50, until Bevy's 250 ms catch-up clamp. A steady-state average hides that; the p95
  and p99 columns do not.
- **Interpolation** cost between +1 and +2.5 ms at 500 and 2,000 stable in this pass; at 5,000
  the difference was inside the run-to-run variance (an earlier pass on a busier machine showed
  about +9 ms, which did not reproduce here).
- **Variance is real on this machine**: the same cases, run in two passes an hour apart with
  other agents' builds running, differed by up to 2x on the 5,000 churn cases (tick rates moved
  with them). Treat single-case numbers as advisory; the shape (per-instance growth; churn
  sim-bound) reproduced in both passes.

### Advisory budgets

No budget is agreed with the user yet. As a starting point for the native reference machine:

- 60 fps (16.7 ms main-thread p99) at **2,000 visible instances**: met here (13.9 ms p99).
- 5,000 visible instances is a **stress ceiling**, not a target: it currently meets the 60 fps
  p50 and misses it at p99, and its sim cost alone is half the frame.
- The browser target is **unmeasured**: the wasm32 build exists, but this harness needs a window
  and a command line, and no browser profiling path was set up here. A wasm32 measurement would
  need the same flags behind the page (or a Playwright-driven build) that this client does not
  have.

### Where that leaves the optimization question

The trace shows the client's draw path is material at 5,000 instances (8.2 ms, about half the
frame) and grows with the instance count, but it cannot attribute that time to allocations, draw
order or the `Drawn` map without allocation counts, which are a gap above (see #1024's comment).
A bounded issue records the measured per-instance cost and asks for the attribution first.

