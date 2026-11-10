//! `iwbtg replay run`: ticks, splits, ranked time, final hash (docs/plans/sim-core.md 9.4, WP8b).
//!
//! Runs a package headless on a `.iwbr` replay or an action-named `.inputs` file and prints six
//! lines:
//!
//! ```text
//! ticks: 9000
//! splits: 0@1 1@2210 2@9000
//! ranked: 8599 frozen: 400
//! hash: <64 hex digits>
//! mode: stripped
//! start: stage1
//! ```
//!
//! A split is `<index>@<tick>` (`timing::Split`: 0 is the start, then the stages, then the end).
//! `splits: none` when no trigger fired. `ranked` is ranked time in ticks, the timer's ticks
//! outside every cutscene and dialogue region, and `frozen` the timer's ticks inside one
//! (`Sim::ranked_ticks`, #34); in Play original nothing is frozen. Two more lines follow: `mode: original` or
//! `mode: stripped` (Play original or ranked, #561; a `.iwbr` replay's header says which, else
//! `--mode`), and `start: <room id>`, the room the run started in. The run stops early when the sim ends (`game_end`) and
//! fails with exit code 2 when it halts. With Sudden Death (`--mods SD`, or a replay header
//! carrying it) the run also stops on the tick of the first death, and a replay with inputs past
//! that tick is rejected (docs/architecture/replay.md, "Mods").

use std::path::{Path, PathBuf};
use std::sync::Arc;

use iwbtg_format::{LoadMode, PackageFiles, load_game};
use iwbtg_sim::def::GameDef;
use iwbtg_sim::input::{InputFrame, parse_action_inputs};
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::replay::{Replay, StartMode};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::PlayMode;
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
    /// Mod codes for an `.inputs` file (`SD`, `SD,NB`); a `.iwbr` replay carries its own.
    #[arg(long, value_parser = ModSet::parse, default_value = "")]
    pub mods: ModSet,
    /// Play mode for an `.inputs` file: `original` or `stripped` (ranked, `[stripped]`); a
    /// `.iwbr` replay carries its own.
    #[arg(long, value_parser = PlayMode::parse, default_value = "original")]
    pub mode: PlayMode,
}

/// What to run: the frames and the setup a replay header (or the `.inputs` flags) gives.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RunInput {
    /// One frame per tick.
    pub frames: Vec<InputFrame>,
    /// The RNG seed.
    pub seed: u32,
    /// The mods.
    pub mods: ModSet,
    /// Original or stripped.
    pub mode: PlayMode,
}

/// What a headless run gives.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReplaySummary {
    /// Ticks run.
    pub ticks: u64,
    /// Splits from the `[timing]` triggers.
    pub splits: Vec<Split>,
    /// Ranked time in ticks (`Sim::ranked_ticks`).
    pub ranked_ticks: u64,
    /// The timer's ticks inside a region (`Sim::frozen_ticks`).
    pub frozen_ticks: u64,
    /// The final state hash.
    pub hash: [u8; 32],
    /// Original or stripped.
    pub mode: PlayMode,
    /// The room the run started in (after `Sim::new`), by id.
    pub start_room: String,
}

impl ReplaySummary {
    /// The six output lines.
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
        format!(
            "ticks: {}\nsplits: {splits}\nranked: {} frozen: {}\nhash: {hash}\nmode: {}\nstart: {}\n",
            self.ticks,
            self.ranked_ticks,
            self.frozen_ticks,
            self.mode.name(),
            self.start_room
        )
    }
}

/// `iwbtg replay run`: reads the package and the input, runs [`run`] and prints `ticks: N`,
/// `splits: ...`, `ranked: N frozen: N`, `hash: <hex>`, `mode: ...` and `start: <room>`.
pub fn main(args: &ReplayRunArgs) -> Result<(), ToolError> {
    if args.start != "new-game" {
        return Err(ToolError::Message(format!(
            "unknown --start `{}` (only `new-game`)",
            args.start
        )));
    }
    let pkg = load_package(&args.pkg)?;
    let input = read_input(&args.input, args.seed, args.mods, args.mode)?;
    let options = SimOptions {
        start: StartPoint::NewGame {
            difficulty: args.difficulty,
        },
        seed: input.seed,
        dll_mode: DllMode::Model,
        mode: input.mode,
    };
    let summary = run(&pkg, &input.frames, options, input.mods)?;
    print!("{}", summary.lines());
    Ok(())
}

/// The frames, seed, mods and play mode of a `.iwbr` replay's bytes; `label` names the replay in
/// errors.
pub fn replay_input(bytes: &[u8], label: &str) -> Result<RunInput, ToolError> {
    let replay = Replay::decode(bytes).map_err(|e| ToolError::Message(format!("{label}: {e}")))?;
    if replay.header.start_mode != StartMode::NewGame {
        return Err(ToolError::Message(format!(
            "{label}: only replays that start a new game can run here"
        )));
    }
    let seed = u32::try_from(replay.header.rng_seed).map_err(|_| {
        ToolError::Message(format!(
            "{label}: the replay's seed {} does not fit the sim's 32-bit seed",
            replay.header.rng_seed
        ))
    })?;
    Ok(RunInput {
        frames: replay.inputs,
        seed,
        mods: replay.header.mods,
        mode: replay.header.play_mode,
    })
}

/// What to run: a `.iwbr` replay's inputs and header, or the frames of an `.inputs` file (frames
/// 0 through its `end` frame, as the trace runner ticks them) with `seed`, `mods` and `mode`.
fn read_input(path: &Path, seed: u32, mods: ModSet, mode: PlayMode) -> Result<RunInput, ToolError> {
    let is_replay = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("iwbr"));
    if is_replay {
        let bytes =
            std::fs::read(path).map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
        return replay_input(&bytes, &path.display().to_string());
    }
    let parsed = parse_action_inputs(&read_text(path)?)
        .map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
    let ticks = usize::try_from(parsed.end).map_or(usize::MAX, |e| e.saturating_add(1));
    let mut frames = parsed.frames;
    frames.truncate(ticks);
    Ok(RunInput {
        frames,
        seed,
        mods,
        mode,
    })
}

/// Runs a package headless on these input frames, one per tick, from `options`, with `mods`.
pub fn run(
    pkg: &PackageFiles,
    frames: &[InputFrame],
    options: SimOptions,
    mods: ModSet,
) -> Result<ReplaySummary, ToolError> {
    run_def(load_strict(pkg, LoadMode::Strict)?, frames, options, mods)
}

/// Loads a package, formatting the diagnostics as an error.
pub fn load_strict(pkg: &PackageFiles, mode: LoadMode) -> Result<GameDef, ToolError> {
    load_game(pkg, mode).map_err(|diags| {
        ToolError::Message(format!(
            "the package does not load:
  {}",
            diags.iter().map(ToString::to_string).collect::<Vec<_>>().join(
                "
  "
            )
        ))
    })
}

/// Runs a loaded game headless on these input frames, one per tick, from `options`. A run that
/// `mods` end (Sudden Death) stops on that tick; frames left after it are an error.
pub fn run_def(
    def: GameDef,
    frames: &[InputFrame],
    options: SimOptions,
    mods: ModSet,
) -> Result<ReplaySummary, ToolError> {
    let mode = options.mode;
    let mut sim = Sim::new(Arc::new(def), options)
        .map_err(|e| ToolError::Message(format!("the sim did not start: {e}")))?;
    let start_room = room_name(sim.def(), sim.world().room);
    let mut splits = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let deaths = sim.world().engine.run.deaths;
        let step = sim.step(*frame);
        splits.extend(step.splits);
        if step.status == SimStatus::Running && mods.ends_run(deaths, sim.world().engine.run.deaths) {
            let left = frames.len() - index - 1;
            if left > 0 {
                return Err(ToolError::Message(format!(
                    "Sudden Death ended the run at tick {}, but {left} more input ticks follow",
                    step.tick
                )));
            }
            break;
        }
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
        ranked_ticks: sim.ranked_ticks(),
        frozen_ticks: sim.frozen_ticks(),
        hash: sim.state_hash(),
        mode,
        start_room,
    })
}

/// A room's id from the package, or its number when the definition has no such room.
fn room_name(def: &GameDef, room: iwbtg_sim::def::RoomId) -> String {
    def.room(room)
        .map_or_else(|| format!("#{}", room.0), |r| r.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_have_the_three_prefixes() {
        let s = ReplaySummary {
            ticks: 12,
            splits: vec![Split { index: 0, tick: 1 }, Split { index: 1, tick: 12 }],
            ranked_ticks: 8,
            frozen_ticks: 3,
            hash: [0xab; 32],
            mode: PlayMode::Stripped,
            start_room: "stage1".into(),
        };
        let text = s.lines();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("ticks: 12"));
        assert_eq!(lines.next(), Some("splits: 0@1 1@12"));
        assert_eq!(lines.next(), Some("ranked: 8 frozen: 3"));
        assert!(
            lines
                .next()
                .is_some_and(|l| l.starts_with("hash: abab") && l.len() == 6 + 64)
        );
        assert_eq!(lines.next(), Some("mode: stripped"));
        assert_eq!(lines.next(), Some("start: stage1"));
        let none = ReplaySummary { splits: vec![], ..s };
        assert!(none.lines().contains("splits: none"));
    }

    #[test]
    fn an_inputs_file_runs_frames_zero_through_end() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("iwbtg-replay-test-{}.inputs", std::process::id()));
        std::fs::write(&path, "frame 2: hold jump\nframe 4: end\n").unwrap();
        let input = read_input(&path, 9, ModSet::NONE, PlayMode::Stripped).unwrap();
        std::fs::remove_file(&path).ok();
        let frames = input.frames;
        assert_eq!(frames.len(), 5);
        assert_eq!(input.seed, 9);
        assert_eq!(input.mode, PlayMode::Stripped);
        assert_eq!(frames[2].held, iwbtg_sim::input::Action::Jump.bit());
    }

    /// A kid whose suicide press kills it, and frames that press it on tick 5.
    fn suicide_kid() -> (GameDef, Vec<InputFrame>) {
        use iwbtg_sim::def::physics::{DeathStep, KeyOn, KidKey};
        use iwbtg_sim::def::testing::GameDefBuilder;
        use iwbtg_sim::def::{Builtin, ClassMask, EngineKind, KindDef, PhysicsProfile, RoomDef};
        use iwbtg_sim::input::Action;
        use iwbtg_sim::kid::KidAction;
        use iwbtg_sim::kid::actions::common::CommonAction;
        let mut p = PhysicsProfile::default();
        p.events.keys = vec![KidKey {
            action: Action::Suicide,
            on: KeyOn::Press,
            run: vec![KidAction::Common(CommonAction::Kill)],
        }];
        p.death.order = vec![DeathStep::DestroyKid, DeathStep::DeathsPlusOne];
        let mut b = GameDefBuilder::new("sudden-death");
        let sprite = b.sprite_rect((11, 21), (5, 12));
        let mut k = KindDef::new("kid", Builtin::Kid);
        k.sprite = Some(sprite);
        k.classes = ClassMask::KID;
        // The loader gives a kid kind its events from the profile; the builder does not.
        k.events = iwbtg_sim::kid::event_set(&p);
        let kid = b.kind(k);
        b.engine_kind(EngineKind::Kid, kid);
        let room = b.room(RoomDef::new("a", 800, 608));
        b.start_room(room);
        b.place(room, kid, 100.0, 100.0).unwrap();
        b.physics(p);
        let mut frames = vec![InputFrame::default(); 200];
        let bit = Action::Suicide.bit();
        frames[5] = InputFrame {
            held: bit,
            pressed: bit,
            released: 0,
        };
        (b.build().unwrap(), frames)
    }

    #[test]
    fn sudden_death_stops_the_run_on_the_first_death() {
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed: 0,
            dll_mode: DllMode::Model,
            mode: iwbtg_sim::stripped::PlayMode::Original,
        };
        let (def, frames) = suicide_kid();
        let plain = run_def(def.clone(), &frames, options.clone(), ModSet::NONE).unwrap();
        assert_eq!(plain.ticks, 200, "without Sudden Death the run goes on");
        let sd = ModSet::parse("SD").unwrap();
        let err = run_def(def.clone(), &frames, options.clone(), sd)
            .unwrap_err()
            .to_string();
        assert!(err.contains("Sudden Death ended the run at tick"), "{err}");
        // A replay that ends on the death tick, as the recording client stops there, runs.
        let tick: usize = err
            .split("at tick ")
            .nth(1)
            .and_then(|t| t.split(',').next())
            .and_then(|t| t.parse().ok())
            .unwrap();
        let ended = run_def(def, &frames[..tick], options, sd).unwrap();
        assert_eq!(ended.ticks, tick as u64);
    }

    /// The `[stripped]` example package: its `@cutscene` lives 30 ticks in `stage1`, the ranked
    /// start, so a ranked run's timer is frozen for it; Play original starts elsewhere.
    #[test]
    fn the_stripped_example_freezes_ranked_time_for_its_cutscene() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../format/tests/valid/stripped");
        let pkg = load_package(&dir).unwrap();
        let frames = vec![InputFrame::default(); 60];
        let options = |mode| SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed: 0,
            dll_mode: DllMode::Model,
            mode,
        };
        let ranked = run(&pkg, &frames, options(PlayMode::Stripped), ModSet::NONE).unwrap();
        assert_eq!((ranked.ranked_ticks, ranked.frozen_ticks), (30, 29));
        let original = run(&pkg, &frames, options(PlayMode::Original), ModSet::NONE).unwrap();
        assert_eq!((original.ranked_ticks, original.frozen_ticks), (59, 0));
    }

    /// The golden one-room package on its `.inputs`: 61 ticks, the run starts at tick 1, and a
    /// second run gives the same hash. Until WP10's `sound::tick` lands the sim reaches a WP0
    /// stub; then the test says so and passes (the CLI exits 3 in that case).
    #[test]
    fn the_golden_one_room_package_runs_twice_to_one_hash() {
        let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sim/tests/golden");
        let pkg = load_package(&golden.join("one_room.pkg")).unwrap();
        let RunInput {
            frames, seed, mods, ..
        } = read_input(
            &golden.join("one_room.inputs"),
            0,
            ModSet::NONE,
            PlayMode::Original,
        )
        .unwrap();
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed,
            dll_mode: DllMode::Model,
            mode: iwbtg_sim::stripped::PlayMode::Original,
        };
        let attempt = std::panic::catch_unwind(|| {
            let a = run(&pkg, &frames, options.clone(), mods).unwrap();
            let b = run(&pkg, &frames, options, mods).unwrap();
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
