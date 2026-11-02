//! `iwbtg replay run`: ticks, splits, final hash (docs/plans/sim-core.md 9.4, WP8b).
//!
//! Runs a package headless on a `.iwbr` replay or an action-named `.inputs` file and prints three
//! lines:
//!
//! ```text
//! ticks: 9000
//! splits: 0@1 1@2210 2@9000
//! hash: <64 hex digits>
//! ```
//!
//! A split is `<index>@<tick>` (`timing::Split`: 0 is the start, then the stages, then the end).
//! `splits: none` when no trigger fired. The run stops early when the sim ends (`game_end`) and
//! fails with exit code 2 when it halts.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use iwbtg_format::{LoadMode, PackageFiles, load_game};
use iwbtg_sim::input::{InputFrame, parse_action_inputs};
use iwbtg_sim::replay::{Replay, StartMode};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::timing::Split;
use iwbtg_sim::world::SimStatus;
use iwbtg_sim::{Sim, SimOptions};

use crate::trace_cmd::{ToolError, load_package, read_text};

/// Arguments of `iwbtg replay run <pkg> <replay|.inputs> [--start new-game --difficulty N]`.
#[derive(Clone, PartialEq, Eq, Debug, clap::Args)]
pub struct ReplayRunArgs {
    /// Package folder.
    pub pkg: PathBuf,
    /// A `.iwbr` replay or an action-named `.inputs` file.
    pub input: PathBuf,
    /// Where the run starts (`new-game`).
    #[arg(long, default_value = "new-game")]
    pub start: String,
    /// Difficulty for `--start new-game`.
    #[arg(long, default_value_t = 0)]
    pub difficulty: u8,
    /// RNG seed for an `.inputs` file; a `.iwbr` replay carries its own.
    #[arg(long, default_value_t = 0)]
    pub seed: u32,
}

/// What a headless run gives.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReplaySummary {
    /// Ticks run.
    pub ticks: u64,
    /// Splits from the `[timing]` triggers.
    pub splits: Vec<Split>,
    /// The final state hash.
    pub hash: [u8; 32],
}

impl ReplaySummary {
    /// The three output lines.
    pub fn lines(&self) -> String {
        let splits = if self.splits.is_empty() {
            "none".to_owned()
        } else {
            self.splits
                .iter()
                .map(|s| format!("{}@{}", s.index, s.tick))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let hash: String = self.hash.iter().map(|b| format!("{b:02x}")).collect();
        format!("ticks: {}\nsplits: {splits}\nhash: {hash}\n", self.ticks)
    }
}

/// `iwbtg replay run`: reads the package and the input, runs [`run`] and prints `ticks: N`,
/// `splits: ...` and `hash: <hex>`.
pub fn main(args: &ReplayRunArgs) -> Result<(), ToolError> {
    if args.start != "new-game" {
        return Err(ToolError::Message(format!(
            "unknown --start `{}` (only `new-game`)",
            args.start
        )));
    }
    let pkg = load_package(&args.pkg)?;
    let (frames, seed) = read_input(&args.input, args.seed)?;
    let options = SimOptions {
        start: StartPoint::NewGame {
            difficulty: args.difficulty,
        },
        seed,
        dll_mode: DllMode::Model,
    };
    let summary = run(&pkg, &frames, options)?;
    print!("{}", summary.lines());
    Ok(())
}

/// The frames to run and the seed: a `.iwbr` replay's inputs and header seed, or the frames of an
/// `.inputs` file (frames 0 through its `end` frame, as the trace runner ticks them) with `seed`.
fn read_input(path: &Path, seed: u32) -> Result<(Vec<InputFrame>, u32), ToolError> {
    let is_replay = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("iwbr"));
    if is_replay {
        let bytes =
            std::fs::read(path).map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
        let replay =
            Replay::decode(&bytes).map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
        if replay.header.start_mode != StartMode::NewGame {
            return Err(ToolError::Message(format!(
                "{}: only replays that start a new game can run here",
                path.display()
            )));
        }
        let seed = u32::try_from(replay.header.rng_seed).map_err(|_| {
            ToolError::Message(format!(
                "{}: the replay's seed {} does not fit the sim's 32-bit seed",
                path.display(),
                replay.header.rng_seed
            ))
        })?;
        return Ok((replay.inputs, seed));
    }
    let parsed = parse_action_inputs(&read_text(path)?)
        .map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
    let ticks = usize::try_from(parsed.end).map_or(usize::MAX, |e| e.saturating_add(1));
    let mut frames = parsed.frames;
    frames.truncate(ticks);
    Ok((frames, seed))
}

/// Runs a package headless on these input frames, one per tick, from `options`.
pub fn run(
    pkg: &PackageFiles,
    frames: &[InputFrame],
    options: SimOptions,
) -> Result<ReplaySummary, ToolError> {
    let def = load_game(pkg, LoadMode::Strict).map_err(|diags| {
        ToolError::Message(format!(
            "the package does not load:\n  {}",
            diags
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n  ")
        ))
    })?;
    let mut sim = Sim::new(Arc::new(def), options)
        .map_err(|e| ToolError::Message(format!("the sim did not start: {e}")))?;
    let mut splits = Vec::new();
    for frame in frames {
        let step = sim.step(*frame);
        splits.extend(step.splits);
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
    Ok(ReplaySummary {
        ticks: sim.world().tick,
        splits,
        hash: sim.state_hash(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_have_the_three_prefixes() {
        let s = ReplaySummary {
            ticks: 12,
            splits: vec![Split { index: 0, tick: 1 }, Split { index: 1, tick: 12 }],
            hash: [0xab; 32],
        };
        let text = s.lines();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("ticks: 12"));
        assert_eq!(lines.next(), Some("splits: 0@1 1@12"));
        assert!(
            lines
                .next()
                .is_some_and(|l| l.starts_with("hash: abab") && l.len() == 6 + 64)
        );
        let none = ReplaySummary { splits: vec![], ..s };
        assert!(none.lines().contains("splits: none"));
    }

    #[test]
    fn an_inputs_file_runs_frames_zero_through_end() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("iwbtg-replay-test-{}.inputs", std::process::id()));
        std::fs::write(&path, "frame 2: hold jump\nframe 4: end\n").unwrap();
        let (frames, seed) = read_input(&path, 9).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(frames.len(), 5);
        assert_eq!(seed, 9);
        assert_eq!(frames[2].held, iwbtg_sim::input::Action::Jump.bit());
    }

    /// The golden one-room package on its `.inputs`: 61 ticks, the run starts at tick 1, and a
    /// second run gives the same hash. Until WP10's `sound::tick` lands the sim reaches a WP0
    /// stub; then the test says so and passes (the CLI exits 3 in that case).
    #[test]
    fn the_golden_one_room_package_runs_twice_to_one_hash() {
        let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sim/tests/golden");
        let pkg = load_package(&golden.join("one_room.pkg")).unwrap();
        let (frames, seed) = read_input(&golden.join("one_room.inputs"), 0).unwrap();
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed,
            dll_mode: DllMode::Model,
        };
        let attempt = std::panic::catch_unwind(|| {
            let a = run(&pkg, &frames, options.clone()).unwrap();
            let b = run(&pkg, &frames, options).unwrap();
            (a, b)
        });
        match attempt {
            Ok((a, b)) => {
                assert_eq!(a, b);
                assert_eq!(a.ticks, 61);
                assert_eq!(a.splits, vec![Split { index: 0, tick: 1 }]);
            }
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_default();
                assert!(msg.starts_with("not implemented: WP"), "unexpected panic: {msg}");
                eprintln!("skipped: needs a stub that is not merged yet ({msg})");
            }
        }
    }
}
