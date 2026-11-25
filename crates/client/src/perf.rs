//! Client frame instrumentation for the performance baseline (#1024; docs/architecture/client/perf.md).
//!
//! `iwbtg-client <package> --perf <frames> --fps unlimited` runs the game for a warmup and then
//! `frames` measured frames, prints a report and exits. It records, per measured frame:
//!
//! - the frame period (this frame's `First` to the next one, which includes the render and the
//!   present call, as `--fps unlimited` runs frames back to back);
//! - the main-thread time (`First` to `Last`: every schedule of the main app);
//! - the sim time (`view::tick`'s own duration, whichever ticks ran in the frame);
//! - and, once, the live instance and sprite counts of the scene.
//!
//! What it cannot measure here, and why, is in the report and in the doc page: GPU/render-submit
//! time (the client has no timestamp queries), allocation counts (a counting global allocator is
//! `unsafe`, and the workspace forbids `unsafe_code`), and process memory (the same `unsafe`
//! rule, and no counter crate is vendored). Those numbers are left out rather than guessed.

use std::path::PathBuf;
use std::time::Instant;

use bevy::prelude::*;

use crate::view::{Game, Stats};

/// How long the warmup may take before the run measures anyway (a halted or very slow sim).
const GIVE_UP: std::time::Duration = std::time::Duration::from_secs(60);

/// The perf mode's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Default)]
pub struct PerfArgs {
    /// Measure this many frames after a warmup, print a report and exit (#1024). Use
    /// `--fps unlimited` so frames are not paced by the cap.
    #[arg(long, hide = true, value_name = "FRAMES")]
    pub perf: Option<u32>,
    /// Sim ticks to run before measuring (default 120: caches, shaders, and a churn room's
    /// population reaching equilibrium). At least 30 frames are always run first.
    #[arg(long, hide = true, value_name = "TICKS", default_value_t = 120, requires = "perf")]
    pub perf_warmup: u32,
    /// Also write the report to this file, to keep as a baseline artifact.
    #[arg(long, hide = true, value_name = "PATH", requires = "perf")]
    pub perf_write: Option<PathBuf>,
}

/// The perf mode's place in the module list (main.rs).
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = PerfArgs;

    fn prepare(
        args: &mut PerfArgs,
        startup: &crate::registration::Startup,
    ) -> Result<(), String> {
        if args.perf.is_some() {
            if startup.screenshot.is_some() {
                return Err("--perf and --screenshot cannot be combined".to_string());
            }
            if startup.package.is_none() {
                return Err("--perf needs a package to measure".to_string());
            }
            if args.perf == Some(0) {
                return Err("--perf needs at least one frame".to_string());
            }
        }
        Ok(())
    }

    fn add(
        app: &mut App,
        args: PerfArgs,
        startup: &crate::registration::Startup,
    ) {
        let Some(frames) = args.perf else {
            return;
        };
        app.insert_resource(Perf {
            frames,
            warmup: args.perf_warmup,
            write: args.perf_write,
            package: startup
                .package
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            frames_seen: 0,
            period: Vec::new(),
            main: Vec::new(),
            sim: Vec::new(),
            order: Vec::new(),
            sync: Vec::new(),
            blend: Vec::new(),
            despawn: Vec::new(),
            snapshot: Vec::new(),
            spawned: Vec::new(),
            updated: Vec::new(),
            removed: Vec::new(),
            last_first: None,
            main_start: Instant::now(),
            sim_start: None,
            sim_frame: 0.0,
            counts: None,
            ticks: 0,
            ticks_at_start: 0,
            last_stats_ticks: 0,
            frames_at_start: 0,
            gave_up: false,
            started: None,
        })
        .init_resource::<Phases>()
        .add_systems(First, (begin_frame, begin_phases))
        // `view::tick` runs in the fixed schedule, once per sim tick; the markers must too.
        .add_systems(FixedUpdate, sim_begin.before(crate::view::tick))
        .add_systems(FixedUpdate, sim_end.after(crate::view::tick))
        .add_systems(Last, end_frame);
    }
}

/// What one frame's draw path spent its time on, filled by `view.rs` when the perf mode is on
/// (`sync_instances` and `tick` write it). All seconds.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub(crate) struct Phases {
    /// `view::order::instance_z`: the sim's draw order, its sort and the z mapping.
    pub(crate) order: f64,
    /// The per-instance loop: lookups, sprite writes and spawns.
    pub(crate) sync: f64,
    /// Inside `sync`: the interpolation lookup into the previous positions.
    pub(crate) blend: f64,
    /// The removal pass and the `Drawn` map swap.
    pub(crate) despawn: f64,
    /// Inside `view::tick`: the previous-position snapshot (interpolation only).
    pub(crate) snapshot: f64,
    pub(crate) spawned: u32,
    pub(crate) updated: u32,
    pub(crate) removed: u32,
}

/// The measurement in progress.
#[derive(Resource)]
struct Perf {
    frames: u32,
    warmup: u32,
    write: Option<PathBuf>,
    package: String,
    /// Frames run so far, warmup included.
    frames_seen: u32,
    /// Measured frame periods and main-thread times, seconds.
    period: Vec<f64>,
    main: Vec<f64>,
    /// The sim's own time in each measured frame, seconds.
    sim: Vec<f64>,
    /// The draw path's phases in each measured frame, seconds.
    order: Vec<f64>,
    sync: Vec<f64>,
    blend: Vec<f64>,
    despawn: Vec<f64>,
    snapshot: Vec<f64>,
    /// Spawned, updated and removed sprites per measured frame.
    spawned: Vec<f64>,
    updated: Vec<f64>,
    removed: Vec<f64>,
    /// When the previous frame began, for its period.
    last_first: Option<Instant>,
    /// When this frame's main schedules began.
    main_start: Instant,
    /// When `view::tick` began, if it runs this frame.
    sim_start: Option<Instant>,
    /// The sim's time so far this frame.
    sim_frame: f64,
    /// Live instances and sprite entities, read once on the first measured frame.
    counts: Option<(usize, usize)>,
    /// Sim ticks counted so far, from `Stats`' per-second counter (which resets).
    ticks: u64,
    /// `ticks` when measuring started, so the report counts only measured ticks.
    ticks_at_start: u64,
    /// The last `Stats::ticks` read, for its reset-aware delta.
    last_stats_ticks: u32,
    /// Frames and ticks run before the first measured frame.
    frames_at_start: u32,
    /// A halted or too-slow sim never reaches the tick warmup: measure anyway after this long.
    gave_up: bool,
    /// When the first frame ran, for the warmup's wall-clock guard.
    started: Option<Instant>,
}

impl Perf {
    /// Whether the frame finishing now counts: the tick warmup has run, the first frames have
    /// passed, and the sim is not halted (a halted sim measures anyway after a long wait).
    fn measuring(&self) -> bool {
        (self.ticks >= u64::from(self.warmup) && self.frames_seen >= 30) || self.gave_up
    }

    /// The warmup's guard: a halted sim (or a build too slow to reach 120 ticks) is measured
    /// anyway after [`GIVE_UP`], so a run never hangs.
    fn overran(&self) -> bool {
        self.started
            .is_some_and(|start| start.elapsed() >= GIVE_UP)
    }
}

/// Starts the frame's clock; the period is the gap to the previous frame's start.
fn begin_frame(mut perf: ResMut<Perf>) {
    let now = Instant::now();
    if perf.measuring()
        && let Some(last) = perf.last_first
    {
        perf.period.push((now - last).as_secs_f64());
    }
    perf.last_first = Some(now);
    perf.main_start = now;
    perf.sim_start = None;
    perf.sim_frame = 0.0;
}

/// Clears the frame's draw phases (they are filled in `Update`).
fn begin_phases(mut phases: ResMut<Phases>) {
    *phases = Phases::default();
}

/// Records when the sim's tick starts.
fn sim_begin(mut perf: ResMut<Perf>) {
    perf.sim_start = Some(Instant::now());
}

/// Adds one frame's sim time (several ticks add up).
fn sim_end(mut perf: ResMut<Perf>) {
    if let Some(start) = perf.sim_start.take() {
        perf.sim_frame += start.elapsed().as_secs_f64();
    }
}

/// Samples the frame's costs and, on the last measured frame, writes the report and exits.
#[allow(clippy::too_many_arguments, reason = "a Bevy system: one parameter per resource")]
fn end_frame(
    mut perf: ResMut<Perf>,
    game: Option<NonSend<Game>>,
    sprites: Query<&Sprite>,
    stats: Res<Stats>,
    draw: Option<Res<Phases>>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) {
    let main = perf.main_start.elapsed().as_secs_f64();
    // `Stats::ticks` counts the current second and resets, so add its delta (wrapping to 0).
    let now_ticks = stats.ticks;
    perf.ticks += u64::from(if now_ticks >= perf.last_stats_ticks {
        now_ticks - perf.last_stats_ticks
    } else {
        now_ticks
    });
    perf.last_stats_ticks = now_ticks;
    perf.started.get_or_insert_with(Instant::now);
    let measuring = perf.measuring();
    if !measuring && perf.overran() {
        perf.gave_up = true;
    }
    perf.frames_seen += 1;
    if !measuring {
        return;
    }
    let first = perf.main.is_empty();
    perf.main.push(main);
    let sim = perf.sim_frame;
    perf.sim.push(sim);
    if let Some(draw) = draw.as_deref() {
        perf.order.push(draw.order);
        perf.sync.push(draw.sync);
        perf.blend.push(draw.blend);
        perf.despawn.push(draw.despawn);
        perf.snapshot.push(draw.snapshot);
        perf.spawned.push(f64::from(draw.spawned));
        perf.updated.push(f64::from(draw.updated));
        perf.removed.push(f64::from(draw.removed));
    }
    if first {
        let alive = game
            .as_ref()
            .map(|g| g.sim.world().instances().iter().filter(|i| i.alive).count())
            .unwrap_or(0);
        perf.counts = Some((alive, sprites.iter().count()));
        perf.ticks_at_start = perf.ticks;
        perf.frames_at_start = perf.frames_seen.saturating_sub(1);
    }
    if perf.period.len() as u32 >= perf.frames {
        let report = report(&perf, window.iter().next().map(|w| w.physical_size()));
        print!("{report}");
        if let Some(path) = &perf.write
            && let Err(e) = std::fs::write(path, &report)
        {
            eprintln!("perf: could not write {}: {e}", path.display());
        }
        exit.write(AppExit::Success);
    }
}

/// The report text: one line per metric, for a file or the terminal.
fn report(perf: &Perf, window: Option<UVec2>) -> String {
    let mut out = String::new();
    out.push_str(&format!("perf package: {}\n", perf.package));
    if let Some(size) = window {
        out.push_str(&format!("perf window: {}x{}\n", size.x, size.y));
    }
    out.push_str(&format!(
        "perf frames: {} measured after {} frames and {} sim ticks of warmup{}
",
        perf.frames,
        perf.frames_at_start,
        perf.ticks_at_start,
        if perf.gave_up {
            " (the sim never reached it; measuring anyway)"
        } else {
            ""
        }
    ));
    if let Some((instances, sprites)) = perf.counts {
        out.push_str(&format!(
            "perf scene: {instances} live instances, {sprites} sprite entities\n"
        ));
    }
    let frames = perf.frames.max(1) as f64;
    let ticks = perf.ticks.saturating_sub(perf.ticks_at_start);
    out.push_str(&format!(
        "perf ticks: {ticks} sim ticks over {:.0} measured frames ({:.1} ticks/s of measured time)\n",
        frames,
        ticks as f64 / perf.period.iter().sum::<f64>().max(f64::EPSILON)
    ));
    for (name, samples) in [
        ("frame period ms", &perf.period),
        ("main-thread ms", &perf.main),
        ("sim ms per frame", &perf.sim),
        ("draw order ms", &perf.order),
        ("sprite sync ms", &perf.sync),
        ("blend ms (in sync)", &perf.blend),
        ("despawn ms", &perf.despawn),
        ("snapshot ms (in tick)", &perf.snapshot),
    ] {
        let Some(p) = Percentiles::of(samples) else {
            continue;
        };
        out.push_str(&format!(
            "perf {name}: p50 {:.2} p95 {:.2} p99 {:.2} min {:.2} mean {:.2} max {:.2}\n",
            p.p50 * 1000.0,
            p.p95 * 1000.0,
            p.p99 * 1000.0,
            p.min * 1000.0,
            p.mean * 1000.0,
            p.max * 1000.0
        ));
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    out.push_str(&format!(
        "perf sprites per frame: {:.1} updated, {:.1} spawned, {:.1} removed
",
        mean(&perf.updated),
        mean(&perf.spawned),
        mean(&perf.removed)
    ));
    out.push_str(
        "perf not measured: GPU and render-submit time (no timestamp queries in the client); \
allocation counts and bytes (a counting global allocator needs `unsafe`, which the workspace \
forbids, and no counter crate is vendored); process memory (same rule). These are recorded as \
gaps, not estimated.\n",
    );
    out
}

/// Sorted-sample percentiles of one metric, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Percentiles {
    p50: f64,
    p95: f64,
    p99: f64,
    min: f64,
    max: f64,
    mean: f64,
}

impl Percentiles {
    /// `None` for no samples. Nearest-rank: the smallest sample at or above the rank.
    fn of(samples: &[f64]) -> Option<Percentiles> {
        if samples.is_empty() {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let at = |q: f64| {
            let rank = (q * sorted.len() as f64).ceil() as usize;
            sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
        };
        let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
        Some(Percentiles {
            p50: at(0.50),
            p95: at(0.95),
            p99: at(0.99),
            min: sorted[0],
            max: sorted[sorted.len() - 1],
            mean,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_are_nearest_rank() {
        let samples: Vec<f64> = (1..=100).map(|n| n as f64).collect();
        let p = Percentiles::of(&samples).expect("samples");
        assert_eq!((p.p50, p.p95, p.p99), (50.0, 95.0, 99.0));
        assert_eq!((p.min, p.max), (1.0, 100.0));
        assert_eq!(p.mean, 50.5);
        assert_eq!(Percentiles::of(&[]), None);
        let one = Percentiles::of(&[0.25]).expect("one sample");
        assert_eq!((one.p50, one.p99, one.max), (0.25, 0.25, 0.25));
    }

    /// The report names every measured metric and every gap it cannot measure here.
    #[test]
    fn the_report_names_the_metrics_and_the_gaps() {
        let perf = Perf {
            frames: 2,
            warmup: 0,
            write: None,
            package: "game.pkg".to_string(),
            frames_seen: 2,
            period: vec![0.004, 0.006],
            main: vec![0.001, 0.002],
            sim: vec![0.0005, 0.0006],
            order: vec![0.001, 0.001],
            sync: vec![0.002, 0.002],
            blend: vec![0.0, 0.0],
            despawn: vec![0.0, 0.0],
            snapshot: vec![0.0, 0.0],
            spawned: vec![0.0, 0.0],
            updated: vec![500.0, 500.0],
            removed: vec![0.0, 0.0],
            last_first: None,
            main_start: Instant::now(),
            sim_start: None,
            sim_frame: 0.0,
            counts: Some((500, 500)),
            ticks: 11,
            ticks_at_start: 1,
            last_stats_ticks: 0,
            frames_at_start: 30,
            gave_up: false,
            started: None,
        };
        let text = report(&perf, Some(UVec2::new(1600, 1000)));
        for want in [
            "perf package: game.pkg",
            "perf window: 1600x1000",
            "perf frames: 2 measured after 30 frames and 1 sim ticks of warmup",
            "perf scene: 500 live instances, 500 sprite entities",
            "perf ticks: 10 sim ticks",
            "perf frame period ms: p50 4.00 p95 6.00 p99 6.00",
            "perf main-thread ms:",
            "perf sim ms per frame:",
            "perf draw order ms:",
            "perf sprite sync ms:",
            "perf blend ms (in sync):",
            "perf despawn ms:",
            "perf snapshot ms (in tick):",
            "perf sprites per frame: 500.0 updated, 0.0 spawned, 0.0 removed",
            "perf not measured: GPU and render-submit time",
            "allocation counts and bytes",
        ] {
            assert!(text.contains(want), "missing {want:?} in:\n{text}");
        }
    }
}
