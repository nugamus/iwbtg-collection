//! `iwbtg replay run`: ticks, splits, ranked time, final hash (docs/plans/sim-core.md 9.4, WP8b).
//!
//! Runs a package headless on a `.iwbr` replay or an action-named `.inputs` file and prints seven
//! lines:
//!
//! ```text
//! ticks: 9000
//! splits: 0@1 1@2210 2@9000
//! ranked: 8599 frozen: 400
//! hash: <64 hex digits>
//! mode: stripped
//! map: hard
//! start: stage1
//! ```
//!
//! A split is `<index>@<tick>` (`timing::Split`: 0 is the start, then the stages, then the end).
//! `splits: none` when no trigger fired. `ranked` is ranked time in ticks, the timer's ticks
//! outside every cutscene and dialogue region, and `frozen` the timer's ticks inside one
//! (`Sim::ranked_ticks`, #34); in Play original nothing is frozen. Three more lines follow: `mode: original` or
//! `mode: stripped` (Play original or ranked, #561; a `.iwbr` replay's header says which, else
//! `--mode`), `map: <id>`, the map whose timing chain (and in a stripped run, ranked start) the
//! run used (#505; the header's, else `--map`, else the package's first map), and
//! `start: <room id>`, the room the run started in. The run stops early when the sim ends (`game_end`) and
//! fails with exit code 2 when it halts. With Sudden Death (`--mods SD`, or a replay header
//! carrying it) the run also stops on the tick of the first death, and a replay with inputs past
//! that tick is rejected (docs/architecture/replay.md, "Mods").

use std::path::{Path, PathBuf};
use std::sync::Arc;

use iwbtg_format::{LoadMode, PackageFiles, load_game};
use iwbtg_sim::def::GameDef;
use iwbtg_sim::input::{InputFrame, parse_action_inputs};
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::replay::{CHECKPOINT_INTERVAL, Replay, StartMode};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::save::SaveSlot;
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
    /// Map id for an `.inputs` file (default: the package's first map, `main` without
    /// `[[maps]]`); a `.iwbr` replay carries its own, and a different `--map` is an error.
    #[arg(long)]
    pub map: Option<String>,
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
    /// The map id; `None` is the package's first map.
    pub map: Option<String>,
    /// The save slot a `FromSave` replay starts from (a client's Play original run that resumed
    /// the save file, #476); `None` is a new game.
    pub save: Option<SaveSlot>,
}

impl RunInput {
    /// Where the run starts: the replay's save slot, else a new game on `difficulty`.
    pub fn start(&self, difficulty: u8) -> StartPoint {
        match &self.save {
            Some(slot) => StartPoint::FromSave(slot.clone()),
            None => StartPoint::NewGame { difficulty },
        }
    }
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
    /// The map the run played (`Sim::map_id`).
    pub map: String,
    /// The room the run started in (after `Sim::new`), by id.
    pub start_room: String,
    /// `(tick, state hash truncated to 16 bytes)` after every tick that is a multiple of
    /// `replay::CHECKPOINT_INTERVAL`, as a replay's trailer keeps them.
    pub checkpoints: Vec<(u64, [u8; 16])>,
}

impl ReplaySummary {
    /// The seven output lines.
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
            "ticks: {}\nsplits: {splits}\nranked: {} frozen: {}\nhash: {hash}\nmode: {}\nmap: {}\nstart: {}\n",
            self.ticks,
            self.ranked_ticks,
            self.frozen_ticks,
            self.mode.name(),
            self.map,
            self.start_room
        )
    }
}

/// `iwbtg replay run`: reads the package and the input, runs [`run`] and prints `ticks: N`,
/// `splits: ...`, `ranked: N frozen: N`, `hash: <hex>`, `mode: ...`, `map: <id>` and
/// `start: <room>`.
pub fn main(args: &ReplayRunArgs) -> Result<(), ToolError> {
    if args.start != "new-game" {
        return Err(ToolError::Message(format!(
            "unknown --start `{}` (only `new-game`)",
            args.start
        )));
    }
    let pkg = load_package(&args.pkg)?;
    let input = read_input(&args.input, args.seed, args.mods, args.mode, args.map.clone())?;
    let options = SimOptions {
        start: input.start(args.difficulty),
        seed: input.seed,
        dll_mode: DllMode::Model,
        mode: input.mode,
        map: input.map,
    };
    let summary = run(&pkg, &input.frames, options, input.mods)?;
    print!("{}", summary.lines());
    Ok(())
}

/// Arguments of `iwbtg replay verify <pkg> <replay>`.
#[derive(Clone, PartialEq, Eq, Debug, clap::Args)]
pub struct ReplayVerifyArgs {
    /// Package folder.
    pub pkg: PathBuf,
    /// A `.iwbr` replay.
    pub replay: PathBuf,
}

/// `iwbtg replay verify`: what the server checks of a replay (docs/architecture/replay.md,
/// Verification), offline. The replay must be for this package (id and `gameplay_hash`) and this
/// sim (`SIM_VERSION`); a headless run on its header and inputs must reproduce every claim of its
/// trailer: tick count, splits, ranked time, checkpoints and final hash. Prints the
/// [`ReplaySummary`] lines, then `verified`, or one `mismatch: ...` line each and `Ok(false)`.
pub fn verify_main(args: &ReplayVerifyArgs) -> Result<bool, ToolError> {
    let files = load_package(&args.pkg)?;
    let bytes = std::fs::read(&args.replay)
        .map_err(|e| ToolError::Message(format!("{}: {e}", args.replay.display())))?;
    let label = args.replay.display().to_string();
    let replay = Replay::decode(&bytes).map_err(|e| ToolError::Message(format!("{label}: {e}")))?;
    let report = iwbtg_format::load::load_report(&files, LoadMode::Strict);
    let (Some(def), Some(gameplay_hash)) = (report.def, report.gameplay_hash) else {
        return Err(load_strict(&files, LoadMode::Strict)
            .err()
            .unwrap_or_else(|| ToolError::Message("the package does not load".to_owned())));
    };
    let input = replay_input(&bytes, &label)?;
    let options = SimOptions {
        start: input.start(0),
        seed: input.seed,
        dll_mode: DllMode::Model,
        mode: input.mode,
        map: input.map.clone(),
    };
    let h = &replay.header;
    let mut bad = Vec::new();
    if h.package_id != def.id {
        bad.push(format!("package `{}`, not `{}`", h.package_id, def.id));
    }
    if h.gameplay_hash != gameplay_hash {
        bad.push(format!(
            "gameplay_hash {} is not this package's {}",
            hex(&h.gameplay_hash),
            hex(&gameplay_hash)
        ));
    }
    if h.sim_version != iwbtg_sim::SIM_VERSION {
        bad.push(format!(
            "recorded on sim version {}, this build is {}",
            h.sim_version,
            iwbtg_sim::SIM_VERSION
        ));
    }
    let s = run_def(def, &input.frames, options, input.mods)?;
    print!("{}", s.lines());
    let t = &replay.trailer;
    if s.ticks != t.total_ticks {
        bad.push(format!(
            "ran {} ticks, the replay claims {}",
            s.ticks, t.total_ticks
        ));
    }
    let splits: Vec<(u16, u64)> = s.splits.iter().map(|x| (x.index, x.tick)).collect();
    if splits != t.splits {
        bad.push(format!("splits {splits:?}, the replay claims {:?}", t.splits));
    }
    if s.ranked_ticks != t.ranked_ticks {
        bad.push(format!(
            "ranked time {}, the replay claims {}",
            s.ranked_ticks, t.ranked_ticks
        ));
    }
    if let Some((ours, theirs)) = s.checkpoints.iter().zip(&t.checkpoints).find(|(a, b)| a != b) {
        bad.push(format!(
            "checkpoint at tick {} differs (first desync before tick {})",
            theirs.0, ours.0
        ));
    } else if s.checkpoints.len() != t.checkpoints.len() {
        bad.push(format!(
            "{} checkpoints, the replay claims {}",
            s.checkpoints.len(),
            t.checkpoints.len()
        ));
    }
    if s.hash != t.final_hash {
        bad.push(format!(
            "final hash differs: the replay claims {}",
            hex(&t.final_hash)
        ));
    }
    for b in &bad {
        println!("mismatch: {b}");
    }
    if bad.is_empty() {
        println!("verified");
    }
    Ok(bad.is_empty())
}

/// Lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The frames, seed, mods and play mode of a `.iwbr` replay's bytes; `label` names the replay in
/// errors.
pub fn replay_input(bytes: &[u8], label: &str) -> Result<RunInput, ToolError> {
    let replay = Replay::decode(bytes).map_err(|e| ToolError::Message(format!("{label}: {e}")))?;
    let save = match &replay.header.start_mode {
        StartMode::NewGame => None,
        StartMode::FromSave(bytes) => Some(postcard::from_bytes::<SaveSlot>(bytes).map_err(|e| {
            ToolError::Message(format!("{label}: the replay's save slot does not decode: {e}"))
        })?),
    };
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
        map: Some(replay.header.map),
        save,
    })
}

/// What to run: a `.iwbr` replay's inputs and header, or the frames of an `.inputs` file (frames
/// 0 through its `end` frame, as the trace runner ticks them) with `seed`, `mods`, `mode` and
/// `map`. A `map` that differs from a replay header's is an error.
fn read_input(
    path: &Path,
    seed: u32,
    mods: ModSet,
    mode: PlayMode,
    map: Option<String>,
) -> Result<RunInput, ToolError> {
    let is_replay = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("iwbr"));
    if is_replay {
        let bytes =
            std::fs::read(path).map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
        let input = replay_input(&bytes, &path.display().to_string())?;
        if let (Some(want), Some(have)) = (&map, &input.map)
            && want != have
        {
            return Err(ToolError::Message(format!(
                "{}: the replay played map `{have}`, not `{want}`",
                path.display()
            )));
        }
        return Ok(input);
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
        map,
        save: None,
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
    let map = sim.map_id().to_owned();
    let mut splits = Vec::new();
    let mut checkpoints = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let deaths = sim.world().engine.run.deaths;
        let step = sim.step(*frame);
        splits.extend(step.splits);
        // After every tick that reaches a multiple of the interval (a halted step adds no tick).
        if step.tick > 0
            && step.tick.is_multiple_of(CHECKPOINT_INTERVAL)
            && checkpoints.last().map(|c: &(u64, [u8; 16])| c.0) != Some(step.tick)
        {
            let mut c = [0; 16];
            c.copy_from_slice(&sim.state_hash()[..16]);
            checkpoints.push((step.tick, c));
        }
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
        map,
        start_room,
        checkpoints,
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
            map: "hard".into(),
            start_room: "stage1".into(),
            checkpoints: Vec::new(),
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
        assert_eq!(lines.next(), Some("map: hard"));
        assert_eq!(lines.next(), Some("start: stage1"));
        let none = ReplaySummary { splits: vec![], ..s };
        assert!(none.lines().contains("splits: none"));
    }

    #[test]
    fn an_inputs_file_runs_frames_zero_through_end() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("iwbtg-replay-test-{}.inputs", std::process::id()));
        std::fs::write(&path, "frame 2: hold jump\nframe 4: end\n").unwrap();
        let input = read_input(&path, 9, ModSet::NONE, PlayMode::Stripped, None).unwrap();
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
            map: None,
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

    /// A replay of the `[stripped]` example on `map`: 60 empty ticks, header only (#505).
    fn stripped_replay(map: &str) -> Vec<u8> {
        use iwbtg_sim::replay::{Header, MAGIC, REPLAY_FORMAT, Trailer};
        Replay {
            header: Header {
                magic: MAGIC,
                replay_format: REPLAY_FORMAT,
                sim_version: iwbtg_sim::SIM_VERSION,
                package_id: "stripped_example".into(),
                gameplay_hash: [0; 32],
                engine_rev: 0,
                rng_seed: 0,
                start_mode: StartMode::NewGame,
                play_mode: PlayMode::Stripped,
                map: map.into(),
                mods: ModSet::NONE,
                client_build_id: String::new(),
            },
            inputs: vec![InputFrame::default(); 60],
            trailer: Trailer {
                total_ticks: 60,
                ..Trailer::default()
            },
        }
        .encode()
        .unwrap()
    }

    /// The `[stripped]` example has two maps: `medium` times `[timing]` from `[stripped].start`
    /// (`stage1`), `hard` ends on entering `title` and starts there. One replay per map runs that
    /// map's chain from that map's start, and prints its map.
    #[test]
    fn each_maps_replay_runs_its_own_chain_and_start() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../format/tests/valid/stripped");
        let pkg = load_package(&dir).unwrap();
        let mut seen = Vec::new();
        for map in ["medium", "hard"] {
            let input = replay_input(&stripped_replay(map), map).unwrap();
            assert_eq!(input.map.as_deref(), Some(map), "the header's map");
            let options = SimOptions {
                start: StartPoint::NewGame { difficulty: 0 },
                seed: input.seed,
                dll_mode: DllMode::Model,
                mode: input.mode,
                map: input.map,
            };
            let s = run(&pkg, &input.frames, options, input.mods).unwrap();
            assert_eq!(s.map, map);
            assert!(s.lines().contains(&format!("map: {map}\n")));
            seen.push((s.start_room, s.splits, s.hash));
        }
        assert_eq!(seen[0].0, "stage1");
        assert_eq!(
            seen[0].1,
            vec![Split { index: 0, tick: 1 }],
            "medium has not ended"
        );
        assert_eq!(seen[1].0, "title", "hard's stripped_start room");
        assert_eq!(
            seen[1].1,
            vec![Split { index: 0, tick: 1 }, Split { index: 1, tick: 1 }],
            "hard's end fires in title"
        );
        assert_ne!(seen[0].2, seen[1].2);
    }

    /// A replay whose trailer is what a run of it gives verifies; a changed final hash, or a
    /// replay of another package version, does not (#476).
    #[test]
    fn verify_accepts_a_true_trailer_and_rejects_a_changed_one() {
        use iwbtg_sim::replay::Trailer;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../format/tests/valid/stripped");
        let files = load_package(&dir).unwrap();
        let hash = iwbtg_format::load::load_report(&files, LoadMode::Strict)
            .gameplay_hash
            .unwrap();
        let mut replay = Replay::decode(&stripped_replay("hard")).unwrap();
        replay.header.gameplay_hash = hash;
        let input = replay_input(&replay.encode().unwrap(), "x").unwrap();
        let options = SimOptions {
            start: input.start(0),
            seed: input.seed,
            dll_mode: DllMode::Model,
            mode: input.mode,
            map: input.map.clone(),
        };
        let s = run(&files, &input.frames, options, input.mods).unwrap();
        replay.trailer = Trailer {
            total_ticks: s.ticks,
            splits: s.splits.iter().map(|x| (x.index, x.tick)).collect(),
            ranked_ticks: s.ranked_ticks,
            checkpoints: s.checkpoints.clone(),
            final_hash: s.hash,
        };
        let path = std::env::temp_dir().join(format!("iwbtg-verify-test-{}.iwbr", std::process::id()));
        let verify = |r: &Replay| {
            std::fs::write(&path, r.encode().unwrap()).unwrap();
            verify_main(&ReplayVerifyArgs {
                pkg: dir.clone(),
                replay: path.clone(),
            })
            .unwrap()
        };
        assert!(verify(&replay));
        let mut tampered = replay.clone();
        tampered.trailer.final_hash[0] ^= 1;
        assert!(!verify(&tampered));
        let mut other = replay.clone();
        other.header.gameplay_hash = [7; 32];
        assert!(!verify(&other));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_map_flag_must_match_the_replays_map() {
        let path = std::env::temp_dir().join(format!("iwbtg-map-test-{}.iwbr", std::process::id()));
        std::fs::write(&path, stripped_replay("hard")).unwrap();
        let same = read_input(&path, 0, ModSet::NONE, PlayMode::Original, Some("hard".into()));
        let other = read_input(&path, 0, ModSet::NONE, PlayMode::Original, Some("medium".into()));
        std::fs::remove_file(&path).ok();
        assert_eq!(same.unwrap().map.as_deref(), Some("hard"));
        let err = other.unwrap_err().to_string();
        assert!(err.contains("played map `hard`, not `medium`"), "{err}");
    }

    #[test]
    fn an_unknown_map_does_not_start() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../format/tests/valid/stripped");
        let pkg = load_package(&dir).unwrap();
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed: 0,
            dll_mode: DllMode::Model,
            mode: PlayMode::Stripped,
            map: Some("main".into()),
        };
        let err = run(&pkg, &[], options, ModSet::NONE).unwrap_err().to_string();
        assert!(err.contains("map `main` is not in package"), "{err}");
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
            map: None,
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
            None,
        )
        .unwrap();
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed,
            dll_mode: DllMode::Model,
            mode: iwbtg_sim::stripped::PlayMode::Original,
            map: None,
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
