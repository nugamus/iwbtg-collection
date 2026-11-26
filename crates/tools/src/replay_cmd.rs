//! `iwbtg replay run`: ticks, splits, ranked time, final hash (docs/architecture/cli.md).
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
//! `start: <room id>`, the room the run started in. `--room` starts an `.inputs` run in that room
//! instead of the new-game room or the map's ranked start; a `.iwbr` replay carries its own
//! start. The run stops early when the sim ends (`game_end`) and
//! fails with exit code 2 when it halts. With Sudden Death (`--mods SD`, or a replay header
//! carrying it) the run also stops on the tick of the first death, and a replay with inputs past
//! that tick is rejected (docs/architecture/replay.md, "Mods").

use std::path::{Path, PathBuf};

use iwbtg_format::{LoadMode, PackageFiles};
use iwbtg_sim::SimOptions;
use iwbtg_sim::def::{GameDef, RoomId};
use iwbtg_sim::input::{Action, InputFrame, parse_action_inputs};
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::replay::{Replay, StartMode};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::PlayMode;

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
    /// Start an `.inputs` run in this room; overrides the new-game room and the map's ranked
    /// start in either play mode. An `.iwbr` replay carries its own start.
    #[arg(long)]
    pub room: Option<String>,
}

pub use iwbtg_verify::RunInput;

pub use iwbtg_verify::ReplaySummary;

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
    if args.room.is_some() && is_replay(&args.input) {
        return Err(ToolError::Message(format!(
            "{}: an .iwbr replay carries its own start; --room needs an .inputs file",
            args.input.display()
        )));
    }
    let pkg = load_package(&args.pkg)?;
    let input = read_input(&args.input, args.seed, args.mods, args.mode, args.map.clone())?;
    let def = load_strict(&pkg, LoadMode::Strict)?;
    let start = match &args.room {
        Some(room) => room_start(&pkg, room, args.difficulty)?,
        None => input.start(&def, args.difficulty)?,
    };
    let options = SimOptions {
        start,
        seed: input.seed,
        dll_mode: DllMode::Model,
        mode: input.mode,
        map: input.map,
    };
    let summary = run(&pkg, &input.frames, options, input.mods)?;
    print!("{}", summary.lines());
    Ok(())
}

/// `--room`: a start in that room. [`StartPoint::Room`] already overrides the new-game room and
/// a stripped run's ranked start; cut rooms still redirect.
pub(crate) fn room_start(pkg: &PackageFiles, room: &str, difficulty: u8) -> Result<StartPoint, ToolError> {
    let def = load_strict(pkg, LoadMode::Strict)?;
    let id = def
        .rooms
        .iter()
        .enumerate()
        .find(|(_, r)| r.name == room)
        .and_then(|(index, _)| u32::try_from(index).ok())
        .map(RoomId)
        .ok_or_else(|| ToolError::Message(format!("no room `{room}` in package `{}`", def.id)))?;
    Ok(StartPoint::Room { room: id, difficulty })
}

/// Whether a replay argument is a recorded `.iwbr` file (as opposed to an `.inputs` file).
pub(crate) fn is_replay(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("iwbr"))
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
/// [`ReplaySummary`] lines, the replay's pauses and the run's time (ranked ticks plus the ticks
/// paused, #623), then `verified`, or one `mismatch: ...` line each and `Ok(false)`.
pub fn verify_main(args: &ReplayVerifyArgs) -> Result<bool, ToolError> {
    let files = load_package(&args.pkg)?;
    let bytes = std::fs::read(&args.replay)
        .map_err(|e| ToolError::Message(format!("{}: {e}", args.replay.display())))?;
    let label = args.replay.display().to_string();
    // The verification itself lives in `iwbtg-verify`, shared with the server (#1565); this
    // command only loads the package and prints the result.
    let verification = iwbtg_verify::verify(&files, &bytes, &label)?;
    if let Some(summary) = &verification.summary {
        print!(
            "{}{}",
            summary.lines(),
            pause_lines(&verification.pauses, verification.run_time)
        );
    }
    for mismatch in &verification.mismatches {
        println!("mismatch: {mismatch}");
    }
    if verification.verified {
        println!("verified");
    }
    Ok(verification.verified)
}

/// Arguments of `iwbtg replay dump <replay>`.
#[derive(Clone, PartialEq, Eq, Debug, clap::Args)]
pub struct ReplayDumpArgs {
    /// A `.iwbr` replay.
    pub replay: PathBuf,
}

/// `iwbtg replay dump`: prints a replay as text ([`dump_text`]). Needs no package.
pub fn dump_main(args: &ReplayDumpArgs) -> Result<(), ToolError> {
    let bytes = std::fs::read(&args.replay)
        .map_err(|e| ToolError::Message(format!("{}: {e}", args.replay.display())))?;
    let label = args.replay.display().to_string();
    let replay = Replay::decode(&bytes).map_err(|e| ToolError::Message(format!("{label}: {e}")))?;
    print!("{}", dump_text(&replay));
    Ok(())
}

/// A replay as text: header fields (`name: value`), the trailer (ticks, ranked, pauses as
/// `<tick>+<ticks paused>` or `none`, time as ranked plus paused, splits, checkpoints, final
/// hash), then `inputs:` and one line per run of equal frames,
/// `<first tick> x<length> held=<actions> pressed=<actions> released=<actions>`, where an empty
/// set is `-` and actions are joined by `+`.
pub fn dump_text(replay: &Replay) -> String {
    use std::fmt::Write;
    let h = &replay.header;
    let t = &replay.trailer;
    let start = match &h.start_mode {
        StartMode::NewGame => "new game".to_owned(),
        StartMode::FromSave(b) => format!("save slot ({} bytes)", b.len()),
    };
    let mut out = String::new();
    let _ = writeln!(out, "format: {}", h.replay_format);
    let _ = writeln!(out, "sim_version: {}", h.sim_version);
    let _ = writeln!(out, "engine_rev: {}", h.engine_rev);
    let _ = writeln!(out, "package: {}", h.package_id);
    let _ = writeln!(out, "gameplay_hash: {}", hex(&h.gameplay_hash));
    let _ = writeln!(out, "seed: {}", h.rng_seed);
    let _ = writeln!(out, "start: {start}");
    let _ = writeln!(out, "mode: {}", h.play_mode.name());
    let _ = writeln!(out, "map: {}", h.map);
    let _ = writeln!(out, "mods: {}", h.mods.codes());
    let _ = writeln!(out, "client: {}", h.client_build_id);
    let _ = writeln!(out, "ticks: {}", t.total_ticks);
    let _ = writeln!(out, "ranked: {}", t.ranked_ticks);
    let _ = writeln!(out, "pauses: {}", pauses_text(&t.pauses));
    let _ = writeln!(out, "time: {}", t.ranked_time());
    let splits = if t.splits.is_empty() {
        "none".to_owned()
    } else {
        t.splits
            .iter()
            .map(|(i, tick)| format!("{i}@{tick}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let _ = writeln!(out, "splits: {splits}");
    let _ = writeln!(out, "checkpoints: {}", t.checkpoints.len());
    for (tick, hash) in &t.checkpoints {
        let _ = writeln!(out, "  {tick} {}", hex(hash));
    }
    let _ = writeln!(out, "final_hash: {}", hex(&t.final_hash));
    let set = |bits: u32| {
        let names: Vec<&str> = Action::ALL
            .into_iter()
            .filter(|a| bits & *a as u32 != 0)
            .map(|a| a.name())
            .collect();
        if names.is_empty() {
            "-".to_owned()
        } else {
            names.join("+")
        }
    };
    let _ = writeln!(out, "inputs:");
    let mut tick = 0u64;
    let mut frames = replay.inputs.iter().peekable();
    while let Some(f) = frames.next() {
        let mut len = 1u64;
        while frames.next_if(|n| *n == f).is_some() {
            len += 1;
        }
        let _ = writeln!(
            out,
            "  {tick} x{len} held={} pressed={} released={}",
            set(f.held),
            set(f.pressed),
            set(f.released)
        );
        tick += len;
    }
    out
}

/// A trailer's pauses as `<tick>+<ticks paused>` joined by spaces, or `none`.
fn pauses_text(pauses: &[(u64, u64)]) -> String {
    if pauses.is_empty() {
        return "none".to_owned();
    }
    pauses
        .iter()
        .map(|(tick, len)| format!("{tick}+{len}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The two lines `replay verify` prints after the run's: the replay's pauses and the run's
/// time (`Trailer::ranked_time`; the sim never sees a pause, so the pauses are the client's
/// claim).
fn pause_lines(pauses: &[(u64, u64)], run_time: u64) -> String {
    format!("pauses: {}\ntime: {run_time}\n", pauses_text(pauses))
}

/// Lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The frames, seed, mods and play mode of a `.iwbr` replay's bytes; `label` names the replay in
/// errors.
pub fn replay_input(bytes: &[u8], label: &str) -> Result<RunInput, ToolError> {
    Ok(iwbtg_verify::replay_input(bytes, label)?)
}

/// What to run: a `.iwbr` replay's inputs and header, or the frames of an `.inputs` file (frames
/// 0 through its `end` frame, as the trace runner ticks them) with `seed`, `mods`, `mode` and
/// `map`. A `map` that differs from a replay header's is an error.
pub(crate) fn read_input(
    path: &Path,
    seed: u32,
    mods: ModSet,
    mode: PlayMode,
    map: Option<String>,
) -> Result<RunInput, ToolError> {
    if is_replay(path) {
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
    Ok(iwbtg_verify::load_strict(pkg, mode)?)
}

/// Runs a loaded game headless on these input frames, one per tick, from `options`. A run that
/// `mods` end (Sudden Death) stops on that tick; frames left after it are an error.
pub fn run_def(
    def: GameDef,
    frames: &[InputFrame],
    options: SimOptions,
    mods: ModSet,
) -> Result<ReplaySummary, ToolError> {
    Ok(iwbtg_verify::run_def(def, frames, options, mods)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iwbtg_sim::save::Checkpoint;
    use iwbtg_sim::timing::Split;
    use iwbtg_verify::{StartSave, sim_version_mismatch};

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

    /// The `stripped_example` fixture starts original runs in `intro` (`new_game_room`) and
    /// stripped runs at `stage1` (`[stripped].start`); `--room title` beats both.
    #[test]
    fn a_room_start_overrides_the_new_game_and_ranked_starts() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../format/tests/valid/stripped");
        let pkg = load_package(&dir).unwrap();
        let frames = vec![InputFrame::default(); 3];
        for mode in [PlayMode::Original, PlayMode::Stripped] {
            let options = SimOptions {
                start: room_start(&pkg, "title", 1).unwrap(),
                seed: 0,
                dll_mode: DllMode::Model,
                mode,
                map: None,
            };
            let s = run(&pkg, &frames, options, ModSet::NONE).unwrap();
            assert_eq!(s.start_room, "title", "{mode:?}");
        }
        for (mode, want) in [(PlayMode::Original, "intro"), (PlayMode::Stripped, "stage1")] {
            let options = SimOptions {
                start: StartPoint::NewGame { difficulty: 0 },
                seed: 0,
                dll_mode: DllMode::Model,
                mode,
                map: None,
            };
            let s = run(&pkg, &frames, options, ModSet::NONE).unwrap();
            assert_eq!(s.start_room, want, "without --room, {mode:?}");
        }
    }

    #[test]
    fn an_unknown_room_names_the_room_and_package() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../format/tests/valid/stripped");
        let pkg = load_package(&dir).unwrap();
        let err = room_start(&pkg, "no_such_room", 0).unwrap_err().to_string();
        assert!(err.contains("no_such_room"), "{err}");
        assert!(err.contains("stripped_example"), "{err}");
    }

    #[test]
    fn a_room_option_needs_an_inputs_file() {
        let path = std::env::temp_dir().join(format!("iwbtg-room-test-{}.iwbr", std::process::id()));
        std::fs::write(&path, stripped_replay("medium")).unwrap();
        let args = ReplayRunArgs {
            pkg: Path::new(env!("CARGO_MANIFEST_DIR")).join("../format/tests/valid/stripped"),
            input: path.clone(),
            start: "new-game".into(),
            difficulty: 0,
            seed: 0,
            mods: ModSet::NONE,
            mode: PlayMode::Original,
            map: None,
            room: Some("title".into()),
        };
        let err = main(&args).unwrap_err().to_string();
        std::fs::remove_file(&path).ok();
        assert!(err.contains("--room"), "{err}");
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

    /// A format-7 replay's `FromSave` carries a checkpoint; a format-6 replay's carries a raw
    /// slot. Both decode, and each starts from its save. The two layouts are identical apart
    /// from the opaque payload, so a format-6 file is the format-7 bytes with the version byte.
    #[test]
    fn a_from_save_replay_reads_both_payload_versions() {
        use iwbtg_sim::replay::{Header, MAGIC, REPLAY_FORMAT, SAVE_SLOT_FORMAT, Trailer};

        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../format/tests/valid/stripped");
        let files = load_package(&dir).unwrap();
        let def = load_strict(&files, LoadMode::Strict).unwrap();
        let slot = iwbtg_sim::save::SaveSlot {
            room: def.start_room,
            x: iwbtg_sim::num::Real::new(96.0),
            y: iwbtg_sim::num::Real::new(500.0),
            precision: Default::default(),
            point: None,
            flipped: false,
            facing: 1,
            difficulty: 0,
            deaths: 7,
            time: 1234,
            bosses: 0,
            items: 0,
            clear: false,
        };
        let checkpoint = Checkpoint::from_slot(&def, &slot).unwrap();
        let replay = |start_mode| Replay {
            header: Header {
                magic: MAGIC,
                replay_format: REPLAY_FORMAT,
                sim_version: iwbtg_sim::SIM_VERSION,
                package_id: def.id.clone(),
                gameplay_hash: [0; 32],
                engine_rev: 0,
                rng_seed: 0,
                start_mode,
                play_mode: PlayMode::Original,
                map: "medium".into(),
                mods: ModSet::NONE,
                client_build_id: String::new(),
            },
            inputs: vec![InputFrame::default(); 4],
            trailer: Trailer {
                total_ticks: 4,
                ..Trailer::default()
            },
        };

        let bytes = replay(StartMode::FromSave(checkpoint.encode().unwrap()))
            .encode()
            .unwrap();
        let input = replay_input(&bytes, "checkpoint").unwrap();
        assert_eq!(input.save, Some(StartSave::Checkpoint(checkpoint.clone())));
        assert_eq!(
            input.start(&def, 0).unwrap(),
            StartPoint::FromSave(checkpoint.clone()),
            "the checkpoint reaches the sim unresolved (#1238)"
        );

        let mut bytes = replay(StartMode::FromSave(postcard::to_stdvec(&slot).unwrap()))
            .encode()
            .unwrap();
        bytes[4] = u8::try_from(SAVE_SLOT_FORMAT).unwrap();
        let input = replay_input(&bytes, "legacy slot").unwrap();
        assert_eq!(input.save, Some(StartSave::LegacySlot(slot.clone())));
        assert_eq!(
            input.start(&def, 0).unwrap(),
            StartPoint::FromSave(checkpoint.clone()),
            "a legacy slot converts to the same checkpoint"
        );

        // #1325: the checkpoint payload is decoded exactly, so a byte appended after it is
        // refused instead of ignored.
        let mut trailing = checkpoint.encode().unwrap();
        trailing.push(0);
        let bytes = replay(StartMode::FromSave(trailing)).encode().unwrap();
        let error = replay_input(&bytes, "trailing").unwrap_err();
        assert!(error.to_string().contains("trailing bytes"), "{error}");
    }

    /// #1325: the replay's `FromSave` runs the shared `Checkpoint::validate`; a foreign
    /// checkpoint format is refused exactly as the client's envelope path refuses it.
    #[test]
    fn a_from_save_checkpoint_with_a_foreign_format_is_refused() {
        use iwbtg_sim::replay::{Header, MAGIC, REPLAY_FORMAT, Trailer};

        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../format/tests/valid/stripped");
        let files = load_package(&dir).unwrap();
        let def = load_strict(&files, LoadMode::Strict).unwrap();
        let slot = iwbtg_sim::save::SaveSlot {
            room: def.start_room,
            x: iwbtg_sim::num::Real::new(96.0),
            y: iwbtg_sim::num::Real::new(500.0),
            precision: Default::default(),
            point: None,
            flipped: false,
            facing: 1,
            difficulty: 0,
            deaths: 7,
            time: 1234,
            bosses: 0,
            items: 0,
            clear: false,
        };
        let mut checkpoint = Checkpoint::from_slot(&def, &slot).unwrap();
        checkpoint.format = Checkpoint::FORMAT + 1;
        let bytes = Replay {
            header: Header {
                magic: MAGIC,
                replay_format: REPLAY_FORMAT,
                sim_version: iwbtg_sim::SIM_VERSION,
                package_id: def.id.clone(),
                gameplay_hash: [0; 32],
                engine_rev: 0,
                rng_seed: 0,
                start_mode: StartMode::FromSave(checkpoint.encode().unwrap()),
                play_mode: PlayMode::Original,
                map: "medium".into(),
                mods: ModSet::NONE,
                client_build_id: String::new(),
            },
            inputs: vec![InputFrame::default(); 4],
            trailer: Trailer {
                total_ticks: 4,
                ..Trailer::default()
            },
        }
        .encode()
        .unwrap();
        let error = replay_input(&bytes, "foreign format").unwrap_err();
        assert!(error.to_string().contains("checkpoint format 2"), "{error}");
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
    fn dump_prints_header_trailer_and_runs() {
        let mut replay = Replay::decode(&stripped_replay("hard")).unwrap();
        let jump = Action::Jump as u32;
        replay.inputs[10] = InputFrame {
            held: jump,
            pressed: jump,
            released: 0,
        };
        replay.inputs[11] = InputFrame {
            held: jump,
            pressed: 0,
            released: 0,
        };
        replay.inputs[12] = InputFrame {
            held: jump,
            pressed: 0,
            released: 0,
        };
        replay.trailer.splits = vec![(1, 5), (2, 40)];
        replay.trailer.ranked_ticks = 30;
        replay.trailer.pauses = vec![(12, 250), (30, 4)];
        let text = dump_text(&Replay::decode(&replay.encode().unwrap()).unwrap());
        for line in [
            "package: stripped_example",
            "mode: stripped",
            "map: hard",
            "mods: NM",
            "ticks: 60",
            "ranked: 30",
            "pauses: 12+250 30+4",
            "time: 284",
            "splits: 1@5 2@40",
            "checkpoints: 0",
            "  0 x10 held=- pressed=- released=-",
            "  10 x1 held=jump pressed=jump released=-",
            "  11 x2 held=jump pressed=- released=-",
            "  13 x47 held=- pressed=- released=-",
        ] {
            assert!(
                text.lines().any(|l| l == line),
                "missing `{line}` in:
{text}"
            );
        }
    }

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
        let def = load_strict(&files, LoadMode::Strict).unwrap();
        let options = SimOptions {
            start: input.start(&def, 0).unwrap(),
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
            pauses: vec![(5, 100), (40, 25)],
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
        let mut old = replay.clone();
        old.header.sim_version = iwbtg_sim::SIM_VERSION - 1;
        assert!(!verify(&old), "another build's replay does not verify");
        assert_eq!(
            pause_lines(
                &replay.trailer.pauses,
                s.ranked_ticks + replay.trailer.paused_ticks()
            ),
            format!(
                "pauses: 5+100 40+25
time: {}
",
                s.ranked_ticks + 125
            )
        );
        let mut tampered = replay.clone();
        tampered.trailer.final_hash[0] ^= 1;
        assert!(!verify(&tampered));
        let mut other = replay.clone();
        other.header.gameplay_hash = [7; 32];
        assert!(!verify(&other));
        std::fs::remove_file(&path).ok();
    }

    /// A replay from another build is reported as a sim-version mismatch, not as a hash
    /// mismatch: the other build's hashes mean nothing here (#1303).
    #[test]
    fn a_sim_version_mismatch_names_the_versions_and_not_a_hash() {
        assert_eq!(sim_version_mismatch(2, 2), None);
        let line = sim_version_mismatch(1, 2).expect("a mismatch");
        assert!(line.contains("sim_version 1, this build is 2"), "{line}");
        assert!(!line.contains("hash"), "{line}");
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
        // A ranked run's timer starts on the first input (#43): tick 0 carries one, so tick 1
        // is its first timed tick.
        let mut frames = vec![InputFrame::default(); 60];
        frames[0] = InputFrame {
            held: iwbtg_sim::input::Action::Jump.bit(),
            ..InputFrame::default()
        };
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
    /// second run gives the same hash. A stub panic aborts the CLI; the test says so if one is
    /// reached.
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
