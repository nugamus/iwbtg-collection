//! Replay verification, shared by `iwbtg replay verify` and the server (#1565;
//! docs/architecture/replay.md, Verification).
//!
//! [`verify`] takes a loaded package and a replay's canonical bytes and returns a structured
//! [`Verification`]: whether the headless run reproduced every trailer claim, the mismatches, and
//! the run's summary when it happened. The CLI prints that result; the server (#163) trusts it.
//! One implementation is what keeps the offline tool and the server from drifting.
//!
//! The package is passed in rather than loaded here: the CLI loads a folder or an `.iwbz`, and
//! the server will load the catalog file; both then use `PackageFiles` and this crate does the
//! same work.

use std::sync::Arc;

use iwbtg_format::{LoadMode, PackageFiles, load_game};
use iwbtg_sim::def::{GameDef, RoomId};
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::replay::{CHECKPOINT_INTERVAL, Replay, SAVE_SLOT_FORMAT, StartMode};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::save::{Checkpoint, SaveSlot};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::timing::Split;
use iwbtg_sim::world::SimStatus;
use iwbtg_sim::{Sim, SimOptions};

/// Why a replay could not be set up or run.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    /// Anything, with a message for the user.
    #[error("{0}")]
    Message(String),
}

fn message(text: impl Into<String>) -> VerifyError {
    VerifyError::Message(text.into())
}

/// The result of one verification.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Verification {
    /// True when every trailer claim reproduced.
    pub verified: bool,
    /// One human-readable line per difference; empty when verified.
    pub mismatches: Vec<String>,
    /// The headless run's summary, or `None` when the replay was refused before running (a
    /// `sim_version` mismatch).
    pub summary: Option<ReplaySummary>,
    /// The replay's claimed pauses, for the run time the CLI prints.
    pub pauses: Vec<(u64, u64)>,
    /// The run's time: its ranked ticks plus the claimed paused ticks.
    pub run_time: u64,
}

/// Verifies `bytes` (a canonical replay) against `files`.
pub fn verify(files: &PackageFiles, bytes: &[u8], label: &str) -> Result<Verification, VerifyError> {
    let replay = Replay::decode(bytes).map_err(|e| message(format!("{label}: {e}")))?;
    let report = iwbtg_format::load::load_report(files, LoadMode::Strict);
    let (Some(def), Some(gameplay_hash)) = (report.def, report.gameplay_hash) else {
        return Err(load_strict(files, LoadMode::Strict)
            .err()
            .unwrap_or_else(|| message("the package does not load")));
    };
    let input = replay_input(bytes, label)?;
    let options = SimOptions {
        start: input.start(&def, 0)?,
        seed: input.seed,
        dll_mode: DllMode::Model,
        mode: input.mode,
        map: input.map.clone(),
    };
    let header = &replay.header;
    let mut mismatches = Vec::new();
    if header.package_id != def.id {
        mismatches.push(format!("package `{}`, not `{}`", header.package_id, def.id));
    }
    if header.gameplay_hash != gameplay_hash {
        mismatches.push(format!(
            "gameplay_hash {} is not this package's {}",
            hex(&header.gameplay_hash),
            hex(&gameplay_hash)
        ));
    }
    if let Some(line) = sim_version_mismatch(header.sim_version, iwbtg_sim::SIM_VERSION) {
        // The recorded hashes are a different build's; do not run or report a hash difference
        // that would hide the real cause.
        mismatches.push(line);
        return Ok(Verification {
            verified: false,
            mismatches,
            summary: None,
            pauses: Vec::new(),
            run_time: 0,
        });
    }

    let summary = run_def(def, &input.frames, options, input.mods)?;
    let trailer = &replay.trailer;
    if summary.ticks != trailer.total_ticks {
        mismatches.push(format!(
            "ran {} ticks, the replay claims {}",
            summary.ticks, trailer.total_ticks
        ));
    }
    let splits: Vec<(u16, u64)> = summary
        .splits
        .iter()
        .map(|split| (split.index, split.tick))
        .collect();
    if splits != trailer.splits {
        mismatches.push(format!(
            "splits {splits:?}, the replay claims {:?}",
            trailer.splits
        ));
    }
    if summary.ranked_ticks != trailer.ranked_ticks {
        mismatches.push(format!(
            "ranked time {}, the replay claims {}",
            summary.ranked_ticks, trailer.ranked_ticks
        ));
    }
    if let Some((ours, theirs)) = summary
        .checkpoints
        .iter()
        .zip(&trailer.checkpoints)
        .find(|(ours, theirs)| ours != theirs)
    {
        mismatches.push(format!(
            "checkpoint at tick {} differs (first desync before tick {})",
            theirs.0, ours.0
        ));
    } else if summary.checkpoints.len() != trailer.checkpoints.len() {
        mismatches.push(format!(
            "{} checkpoints, the replay claims {}",
            summary.checkpoints.len(),
            trailer.checkpoints.len()
        ));
    }
    if summary.hash != trailer.final_hash {
        mismatches.push(format!(
            "final hash differs: the replay claims {}",
            hex(&trailer.final_hash)
        ));
    }

    Ok(Verification {
        verified: mismatches.is_empty(),
        mismatches,
        run_time: summary.ranked_ticks.saturating_add(trailer.paused_ticks()),
        pauses: trailer.pauses.clone(),
        summary: Some(summary),
    })
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
    /// The save a `FromSave` replay starts from; `None` is a new game.
    pub save: Option<StartSave>,
}

/// The payload of a replay's `FromSave`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum StartSave {
    /// A versioned checkpoint; resolves its stable room name against the package.
    Checkpoint(Checkpoint),
    /// A pre-checkpoint replay's raw slot; its numeric room is bound to the replay's package by
    /// the header's `gameplay_hash`.
    LegacySlot(SaveSlot),
}

impl RunInput {
    /// Where the run starts: the replay's save, else a new game on `difficulty`.
    pub fn start(&self, def: &GameDef, difficulty: u8) -> Result<StartPoint, VerifyError> {
        match &self.save {
            Some(StartSave::Checkpoint(checkpoint)) => Ok(StartPoint::FromSave(checkpoint.clone())),
            Some(StartSave::LegacySlot(slot)) => {
                // A pre-checkpoint replay's slot stores a numeric room; the header's
                // `gameplay_hash` binds it to this package, so naming that room here is the
                // conversion, not a resolution against a live save.
                let checkpoint = Checkpoint::from_slot(def, slot)
                    .map_err(|e| message(format!("the replay's save slot does not resolve: {e}")))?;
                Ok(StartPoint::FromSave(checkpoint))
            }
            None => Ok(StartPoint::NewGame { difficulty }),
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
    /// The seven output lines shared by `replay run` and `replay verify`.
    pub fn lines(&self) -> String {
        let splits = if self.splits.is_empty() {
            "none".to_owned()
        } else {
            self.splits
                .iter()
                .map(|split| format!("{}@{}", split.index, split.tick))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let hash: String = self.hash.iter().map(|byte| format!("{byte:02x}")).collect();
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

/// The frames, seed, mods and play mode of a `.iwbr` replay's bytes; `label` names the replay in
/// errors.
pub fn replay_input(bytes: &[u8], label: &str) -> Result<RunInput, VerifyError> {
    let replay = Replay::decode(bytes).map_err(|e| message(format!("{label}: {e}")))?;
    let save = match &replay.header.start_mode {
        StartMode::NewGame => None,
        StartMode::FromSave(bytes) if replay.header.replay_format > SAVE_SLOT_FORMAT => {
            Some(StartSave::Checkpoint(Checkpoint::decode(bytes).map_err(|e| {
                message(format!("{label}: the replay's checkpoint does not decode: {e}"))
            })?))
        }
        StartMode::FromSave(bytes) => Some(StartSave::LegacySlot(
            // The shared exact decode: a legacy slot with bytes after it is refused (#1325).
            SaveSlot::decode(bytes)
                .map_err(|e| message(format!("{label}: the replay's save slot does not decode: {e}")))?,
        )),
    };
    let seed = u32::try_from(replay.header.rng_seed).map_err(|_| {
        message(format!(
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

/// Loads a package, formatting the diagnostics as an error.
pub fn load_strict(pkg: &PackageFiles, mode: LoadMode) -> Result<GameDef, VerifyError> {
    load_game(pkg, mode).map_err(|diags| {
        message(format!(
            "the package does not load:\n  {}",
            diags
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n  ")
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
) -> Result<ReplaySummary, VerifyError> {
    let mode = options.mode;
    let mut sim =
        Sim::new(Arc::new(def), options).map_err(|e| message(format!("the sim did not start: {e}")))?;
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
            let mut checkpoint = [0; 16];
            checkpoint.copy_from_slice(&sim.state_hash()[..16]);
            checkpoints.push((step.tick, checkpoint));
        }
        if step.status == SimStatus::Running && mods.ends_run(deaths, sim.world().engine.run.deaths) {
            let left = frames.len() - index - 1;
            if left > 0 {
                return Err(message(format!(
                    "Sudden Death ended the run at tick {}, but {left} more input ticks follow",
                    step.tick
                )));
            }
            break;
        }
        match step.status {
            SimStatus::Running => {}
            SimStatus::Ended => break,
            SimStatus::Halted(error) => {
                return Err(message(format!("the sim halted at tick {}: {error}", step.tick)));
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

/// The line that tells a verifier a replay came from another sim build, or `None` when the
/// versions match.
pub fn sim_version_mismatch(recorded: u32, current: u32) -> Option<String> {
    (recorded != current).then(|| {
        format!(
            "sim_version {recorded}, this build is {current}: a replay is verified by a build with its own SIM_VERSION"
        )
    })
}

/// A room's id from the package, or its number when the definition has no such room.
fn room_name(def: &GameDef, room: RoomId) -> String {
    def.room(room)
        .map_or_else(|| format!("#{}", room.0), |r| r.name.clone())
}

/// Lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_summary_renders_the_seven_lines() {
        let summary = ReplaySummary {
            ticks: 12,
            splits: Vec::new(),
            ranked_ticks: 10,
            frozen_ticks: 2,
            hash: [0xab; 32],
            mode: PlayMode::Stripped,
            map: "hard".to_owned(),
            start_room: "stage1".to_owned(),
            checkpoints: Vec::new(),
        };
        let text = summary.lines();
        assert!(text.starts_with("ticks: 12\nsplits: none\nranked: 10 frozen: 2\nhash: abab"));
        assert!(text.contains("mode: stripped\nmap: hard\nstart: stage1\n"));
    }

    #[test]
    fn a_sim_version_mismatch_is_named_not_a_hash_difference() {
        let line = sim_version_mismatch(3, 4).expect("a mismatch line");
        assert!(line.contains("sim_version 3"), "{line}");
        assert!(sim_version_mismatch(4, 4).is_none());
    }
}
