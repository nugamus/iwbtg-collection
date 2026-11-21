//! `iwbtg profile <pkg> --replay <file>`: the script budget, instance count and speed of a
//! headless run (docs/decisions/0001-script-language.md, "Step budget"; #931).
//!
//! Prints four lines:
//!
//! ```text
//! ticks: 9000
//! peak script units: 1234 / 1000000 (0.1%)
//! peak instances: 25
//! ms per tick: 0.123 (native)
//! ```
//!
//! `peak script units` is the most units one tick used over the run (`Budget::peak`), against
//! the package's tick limit; decision 0001's CI rule keeps a script under 25% of it.
//! `peak instances` is the most alive instances seen at the start and after each tick
//! (`World::iter_alive`); a spike created and destroyed inside one tick is not seen.
//! `ms per tick` is the wall time of `Sim::step` alone, summed over the run and divided by the
//! ticks, measured on the native build.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iwbtg_format::{LoadMode, PackageFiles};
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::world::SimStatus;
use iwbtg_sim::{Sim, SimOptions};

use crate::replay_cmd::{self, RunInput};
use crate::trace_cmd::{ToolError, load_package};

/// Arguments of `iwbtg profile <pkg> --replay <file>`.
#[derive(Clone, PartialEq, Eq, Debug, clap::Args)]
pub struct ProfileArgs {
    /// Package folder or `.iwbz` file.
    pub pkg: PathBuf,
    /// A `.iwbr` replay or an action-named `.inputs` file to profile.
    #[arg(long)]
    pub replay: PathBuf,
    /// Difficulty for an `.inputs` file's new game; a `.iwbr` replay carries its own start.
    #[arg(long, default_value_t = 0)]
    pub difficulty: u8,
    /// RNG seed for an `.inputs` file; a `.iwbr` replay carries its own.
    #[arg(long, default_value_t = 0)]
    pub seed: u32,
    /// Mod codes for an `.inputs` file (`SD`, `SD,NB`); a `.iwbr` replay carries its own.
    #[arg(long, value_parser = ModSet::parse, default_value = "")]
    pub mods: ModSet,
    /// Play mode for an `.inputs` file: `original` or `stripped` (ranked, `[stripped]`); a
    /// `.iwbr` replay carries its own.
    #[arg(long, value_parser = PlayMode::parse, default_value = "original")]
    pub mode: PlayMode,
    /// Map id for an `.inputs` file (default: the package's first map); a `.iwbr` replay
    /// carries its own, and a different `--map` is an error.
    #[arg(long)]
    pub map: Option<String>,
}

/// A run's profile.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Profile {
    /// Ticks run.
    pub ticks: u64,
    /// The most units any tick used (`Budget::peak`).
    pub peak_units: u64,
    /// The package's tick budget.
    pub tick_limit: u64,
    /// The most alive instances seen at the start or after a tick.
    pub peak_instances: usize,
    /// The wall time of the steps.
    pub elapsed: Duration,
}

impl Profile {
    /// The four output lines.
    pub fn lines(&self) -> String {
        let percent = if self.tick_limit == 0 {
            0.0
        } else {
            100.0 * self.peak_units as f64 / self.tick_limit as f64
        };
        let ms = if self.ticks == 0 {
            0.0
        } else {
            self.elapsed.as_secs_f64() * 1000.0 / self.ticks as f64
        };
        format!(
            "ticks: {}\npeak script units: {} / {} ({percent:.1}%)\npeak instances: {}\nms per tick: {ms:.3} (native)\n",
            self.ticks, self.peak_units, self.tick_limit, self.peak_instances
        )
    }
}

/// `iwbtg profile`: reads the package and the replay, runs [`profile`] and prints its lines.
pub fn main(args: &ProfileArgs) -> Result<(), ToolError> {
    let pkg = load_package(&args.pkg)?;
    let input = replay_cmd::read_input(&args.replay, args.seed, args.mods, args.mode, args.map.clone())?;
    let profile = profile(&pkg, &input, args.difficulty)?;
    print!("{}", profile.lines());
    Ok(())
}

/// Profiles `pkg` on `input`'s frames, from its save slot or a new game at `difficulty`. A sim
/// halt is an error, as in `replay run`.
pub fn profile(pkg: &PackageFiles, input: &RunInput, difficulty: u8) -> Result<Profile, ToolError> {
    let def = replay_cmd::load_strict(pkg, LoadMode::Strict)?;
    let options = SimOptions {
        start: input.start(&def, difficulty)?,
        seed: input.seed,
        dll_mode: DllMode::Model,
        mode: input.mode,
        map: input.map.clone(),
    };
    let mut sim = Sim::new(Arc::new(def), options)
        .map_err(|e| ToolError::Message(format!("the sim did not start: {e}")))?;
    let mut peak_instances = sim.world().iter_alive().count();
    let mut elapsed = Duration::ZERO;
    for frame in &input.frames {
        let started = Instant::now();
        let step = sim.step(*frame);
        // Time `Sim::step` only: the instance count below is the profiler's own work.
        elapsed += started.elapsed();
        peak_instances = peak_instances.max(sim.world().iter_alive().count());
        match step.status {
            SimStatus::Running => {}
            SimStatus::Ended => break,
            SimStatus::Halted(e) => {
                return Err(ToolError::Message(format!(
                    "the sim halted at tick {}: {e}",
                    step.tick
                )));
            }
        }
    }
    // A package without scripts has no engine and charged nothing; report the default limit.
    let (peak_units, tick_limit) =
        sim.scripts()
            .engine()
            .map_or((0, iwbtg_sim::script::budget::DEFAULT_TICK_BUDGET), |iws| {
                let budget = iws.budget();
                (budget.peak(), budget.tick_limit())
            });
    Ok(Profile {
        ticks: sim.world().tick,
        peak_units,
        tick_limit,
        peak_instances,
        elapsed,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use iwbtg_sim::input::InputFrame;

    use super::*;

    /// The `stripped` fixture package runs 60 ticks with its scripts and instances: the profile
    /// reports that tick count, a peak under the tick limit and a non-zero script cost.
    #[test]
    fn profiles_a_small_package() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../format/tests/valid/stripped");
        let pkg = load_package(&dir).expect("package");
        let input = RunInput {
            frames: vec![InputFrame::default(); 60],
            seed: 0,
            mods: ModSet::NONE,
            mode: PlayMode::Stripped,
            map: Some("medium".into()),
            save: None,
        };
        let p = profile(&pkg, &input, 0).expect("profile");
        assert_eq!(p.ticks, 60);
        assert!(p.peak_instances >= 1, "{p:?}");
        assert!(p.peak_units > 0, "the package's scripts charge the budget: {p:?}");
        assert!(p.peak_units <= p.tick_limit, "{p:?}");
        let text = p.lines();
        assert!(text.starts_with("ticks: 60\n"), "{text}");
        assert!(
            text.contains(&format!(
                "peak script units: {} / {} (",
                p.peak_units, p.tick_limit
            )),
            "{text}"
        );
        assert!(
            text.contains(&format!("peak instances: {}\n", p.peak_instances)),
            "{text}"
        );
        assert!(text.contains(" (native)\n"), "{text}");
    }

    /// An empty run reports zero ticks and a zero rate rather than dividing by zero.
    #[test]
    fn an_empty_run_reports_zero() {
        let p = Profile {
            ticks: 0,
            peak_units: 0,
            tick_limit: 1_000_000,
            peak_instances: 0,
            elapsed: Duration::from_millis(5),
        };
        assert!(
            p.lines().contains("ms per tick: 0.000 (native)\n"),
            "{}",
            p.lines()
        );
    }
}
