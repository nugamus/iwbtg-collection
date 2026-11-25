//! The stress packages load and replay (#1068).
//!
//! `crates/sim/tests/stress/` holds the synthetic packages the script layer is measured on (#990,
//! #991). Nothing else reads them, so a loader, script-API or package change could break them
//! unnoticed. This module loads every `*.pkg` with [`LoadMode::Strict`] and fails on any
//! diagnostic, then runs each `.inputs` replay twice from the smallest room of its variant. The
//! 5,000-bullet rooms are for `iwbtg profile` (#1002), not for every check; the same replays run
//! in them at the cost of seconds per run.
//!
//! A new package is picked up automatically. A new replay needs its room in [`ROOMS`] and in the
//! package named by its prefix (`avoidance_native.inputs` runs `avoidance.pkg`); the test fails
//! with the missing name rather than skipping it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use iwbtg_format::load::load_report;
use iwbtg_format::{LoadMode, PackageFiles};
use iwbtg_sim::def::{GameDef, RoomId};
use iwbtg_sim::input::parse_action_inputs;
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::world::SimStatus;
use iwbtg_sim::{Sim, SimOptions};

/// The room each replay starts in. The smallest room of its variant keeps the check cheap.
const ROOMS: &[(&str, &str)] = &[
    ("avoidance_native", "native_500"),
    ("avoidance_scripted", "scripted_500"),
    ("boss_clear", "arena"),
];

/// The folder the packages and replays live in.
#[allow(
    clippy::disallowed_methods,
    reason = "the test reads the stress folder, not sim code"
)]
fn stress_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stress")
}

/// Every package folder under `tests/stress`, sorted by name.
#[allow(
    clippy::disallowed_methods,
    reason = "the test reads the stress folder, not sim code"
)]
fn packages() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(stress_dir())
        .expect("the stress folder reads")
        .map(|entry| entry.expect("a stress entry").path())
        .filter(|path| path.is_dir() && path.extension().is_some_and(|e| e == "pkg"))
        .collect();
    out.sort();
    out
}

/// Every replay file under `tests/stress`, sorted by name.
#[allow(
    clippy::disallowed_methods,
    reason = "the test reads the stress folder, not sim code"
)]
fn replays() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(stress_dir())
        .expect("the stress folder reads")
        .map(|entry| entry.expect("a stress entry").path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|e| e == "inputs"))
        .collect();
    out.sort();
    out
}

/// Loads `dir` in strict mode. A diagnostic, warning or error, is a failure.
#[allow(
    clippy::disallowed_methods,
    reason = "the test reads the stress folder, not sim code"
)]
fn load(dir: &Path) -> GameDef {
    let files = PackageFiles::load_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let report = load_report(&files, LoadMode::Strict);
    let diagnostics: Vec<String> = report.diagnostics.iter().map(ToString::to_string).collect();
    assert!(
        diagnostics.is_empty(),
        "{}: {} diagnostic(s):\n  {}",
        dir.display(),
        diagnostics.len(),
        diagnostics.join("\n  ")
    );
    report
        .def
        .unwrap_or_else(|| panic!("{} loaded without a definition", dir.display()))
}

/// The package a replay belongs to: the one whose stem is the replay's stem before its first
/// `_` (`avoidance_native` runs `avoidance.pkg`).
fn package_for(replay: &str, packages: &[PathBuf]) -> PathBuf {
    let mut matches: Vec<PathBuf> = packages
        .iter()
        .filter(|pkg| {
            let stem = pkg.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            replay
                .strip_prefix(stem)
                .is_some_and(|rest| rest.starts_with('_'))
        })
        .cloned()
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "{replay}: expected one package, found {matches:?}"
    );
    matches.pop().expect("checked above")
}

/// A finished run's tick count, room name and final hash.
type Summary = (u64, String, [u8; 32]);

/// Runs `frames` in `room` and returns what it ended on. A halt (a script error included) fails
/// with the tick and the error.
fn run(def: &GameDef, room: &str, frames: &[iwbtg_sim::input::InputFrame]) -> Summary {
    let id = def
        .rooms
        .iter()
        .enumerate()
        .find(|(_, r)| r.name == room)
        .and_then(|(index, _)| u32::try_from(index).ok())
        .map(RoomId)
        .unwrap_or_else(|| panic!("no room `{room}` in package `{}`", def.id));
    let options = SimOptions {
        start: StartPoint::Room {
            room: id,
            difficulty: 0,
        },
        seed: 0,
        dll_mode: DllMode::Model,
        mode: PlayMode::Original,
        map: None,
    };
    let mut sim = Sim::new(Arc::new(def.clone()), options)
        .unwrap_or_else(|e| panic!("{room}: the sim did not start: {e}"));
    let start_room = sim
        .def()
        .room(sim.world().room)
        .map_or_else(|| format!("#{}", sim.world().room.0), |r| r.name.clone());
    assert_eq!(start_room, room, "the run did not start in its room");
    for frame in frames {
        let step = sim.step(*frame);
        if let SimStatus::Halted(e) = step.status {
            panic!("{room}: the sim halted at tick {}: {e}", step.tick);
        }
        if step.status == SimStatus::Ended {
            break;
        }
    }
    assert!(sim.world().tick > 0, "{room}: no tick ran");
    (sim.world().tick, start_room, sim.state_hash())
}

/// Every package loads in strict mode with no diagnostics, warning or error.
#[test]
fn every_stress_package_loads_in_strict_mode() {
    let packages = packages();
    assert!(!packages.is_empty(), "no *.pkg under tests/stress");
    for dir in packages {
        let def = load(&dir);
        assert!(
            !def.rooms.is_empty(),
            "{}: the package has no rooms",
            dir.display()
        );
    }
}

/// Every replay finishes without a halt, starts in the room mapped to it, and the same run twice
/// produces the same final hash, so the packages cannot drift silently.
#[test]
fn every_stress_replay_runs_and_is_deterministic() {
    let packages = packages();
    let replays = replays();
    assert!(!replays.is_empty(), "no *.inputs under tests/stress");
    for replay in replays {
        let stem = replay
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| panic!("{}: no UTF-8 stem", replay.display()));
        let room = ROOMS
            .iter()
            .find(|(name, _)| *name == stem)
            .map(|(_, room)| *room)
            .unwrap_or_else(|| panic!("{stem}: add the replay to ROOMS"));
        let def = load(&package_for(stem, &packages));
        #[allow(
            clippy::disallowed_methods,
            reason = "the test reads the replay, not sim code"
        )]
        let text = std::fs::read_to_string(&replay).unwrap_or_else(|e| panic!("{}: {e}", replay.display()));
        let inputs = parse_action_inputs(&text).unwrap_or_else(|e| panic!("{}: {e}", replay.display()));
        let first = run(&def, room, &inputs.frames);
        let second = run(&def, room, &inputs.frames);
        assert_eq!(first, second, "{stem}: two runs differ");
        assert_eq!(first.1, room, "{stem}: started in the wrong room");
    }
}
