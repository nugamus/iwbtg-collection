//! Every run is recorded as a replay (`.iwbr`, docs/architecture/replay.md) and kept in the data
//! area of `storage.rs` (docs/architecture/client/replays.md; #476), for the replay viewer and
//! upload (M7) to use later.
//!
//! A run is one sim: the client's start, and each quick-retry or auto-retry after it. The
//! [`Recorder`] takes every tick's input frame and splits from `view::tick` and builds the header
//! from the sim's options (seed, start, play mode, map) and the package (id, `gameplay_hash`,
//! `engine_rev`). The file is written
//!
//! - when the run's timer ends (the clear). A ranked run's replay stops there; Play original goes
//!   on recording and rewrites the same file later, so one Play original session (until a retry)
//!   is one replay;
//! - when a retry starts a new run;
//! - when the window closes.
//!
//! Names: `replays/<game id>/<unix seconds>-<n>.iwbr`, listed oldest first in
//! `replays/index.txt` (the browser's storage cannot list its entries). On the web, where
//! `localStorage` holds a few MB, only the newest [`WEB_KEEP`] are kept, plus the replays the
//! stats boards point at (#607, [`to_remove`]).

use std::collections::BTreeSet;

use bevy::prelude::*;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::replay::{CHECKPOINT_INTERVAL, Header, MAGIC, REPLAY_FORMAT, Replay, StartMode, Trailer};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::sim::Sim;
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::timing::Split;

use crate::storage::{self, Store};
use crate::view::Game;

/// The list of replays in the data area, one name per line, oldest first.
pub const INDEX: &str = "replays/index.txt";

/// Replays kept in a browser's `localStorage` (a 60-minute run is a few KB, stored as hex),
/// more when the stats boards' PBs alone are more ([`to_remove`]). Natively every replay is kept.
pub const WEB_KEEP: usize = 50;

/// A new random RNG seed for a run (docs/product.md: ranked runs use a random seed per run, as
/// the originals do). Never used inside the sim; the replay header carries it.
pub fn fresh_seed() -> u32 {
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        (web_sys::js_sys::Math::random() * 4_294_967_296.0) as u32
    }
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        use std::hash::{BuildHasher, Hasher};
        // `RandomState` is keyed from the OS's random source once per process; the clock makes
        // two seeds of one process differ.
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        let x = h.finish();
        (x ^ (x >> 32)) as u32
    }
}

/// The run being recorded.
struct Recording {
    /// Where it goes in the data area.
    name: String,
    header: Header,
    inputs: Vec<InputFrame>,
    splits: Vec<(u16, u64)>,
    checkpoints: Vec<(u64, [u8; 16])>,
    /// Ticks recorded since the last write.
    unsaved: bool,
    /// Written at least once (so it is in the index).
    listed: bool,
    /// A ranked run's clear: nothing more is recorded.
    closed: bool,
    /// The timer has ended (the clear was written).
    ended: bool,
}

/// The replay recorder: the run being recorded and where replays go.
#[derive(Resource)]
pub struct Recorder {
    /// `None`: this session keeps no replays (a `--screenshot` check, or no data area).
    store: Option<Store>,
    gameplay_hash: [u8; 32],
    mods: ModSet,
    /// Replays started this session, for unique names.
    started: u32,
    current: Option<Recording>,
    /// The run cannot be recorded (it started in a room, not from a new game or a save).
    skip: bool,
    /// Whether old replays are pruned to [`WEB_KEEP`].
    prune: bool,
}

impl Recorder {
    /// A recorder writing to `store` (`None`: records nothing).
    pub fn new(store: Option<Store>, gameplay_hash: [u8; 32], mods: ModSet) -> Recorder {
        Recorder {
            store,
            gameplay_hash,
            mods,
            started: 0,
            current: None,
            skip: false,
            prune: cfg!(all(target_arch = "wasm32", target_os = "unknown")),
        }
    }

    /// The data-area name of the run's replay, once its first tick is recorded. A cleared run
    /// on a stats board points at it (`stats::Run::replay`).
    pub fn current_name(&self) -> Option<&str> {
        self.current.as_ref().map(|r| r.name.as_str())
    }

    /// One tick, called by `view::tick` right after `Sim::step` of a running sim: the frame
    /// it was given and the splits it returned.
    pub fn record(&mut self, sim: &Sim, frame: InputFrame, splits: &[Split]) {
        if self.store.is_none() || self.skip {
            return;
        }
        if self.current.is_none() {
            self.current = self.begin(sim);
            if self.current.is_none() {
                self.skip = true;
                return;
            }
        }
        let Some(rec) = self.current.as_mut() else { return };
        if rec.closed {
            return;
        }
        rec.inputs.push(frame);
        rec.splits.extend(splits.iter().map(|s| (s.index, s.tick)));
        rec.unsaved = true;
        let world = sim.world();
        // After every tick that reaches a multiple of the interval (a halted step adds no tick).
        if world.tick > 0
            && world.tick.is_multiple_of(CHECKPOINT_INTERVAL)
            && rec.checkpoints.last().map(|c| c.0) != Some(world.tick)
        {
            rec.checkpoints.push((world.tick, checkpoint(sim)));
        }
        if world.timing.ended && !rec.ended {
            rec.ended = true;
            rec.closed = rec.header.play_mode == PlayMode::Stripped;
            self.write(sim);
        }
    }

    /// Writes the run's replay if it has ticks not yet written, and starts afresh: the next
    /// recorded tick begins a new replay. Called on a retry (before the new sim) and at exit.
    pub fn finish(&mut self, sim: &Sim) {
        self.write(sim);
        self.current = None;
        self.skip = false;
    }

    /// The header and name of a new replay of `sim`'s run, or `None` when its start cannot be
    /// replayed.
    fn begin(&mut self, sim: &Sim) -> Option<Recording> {
        let options = sim.options();
        let start_mode = match &options.start {
            StartPoint::NewGame { .. } => StartMode::NewGame,
            StartPoint::FromSave(slot) => StartMode::FromSave(postcard::to_stdvec(slot).ok()?),
            _ => return None,
        };
        let def = sim.def();
        let header = Header {
            magic: MAGIC,
            replay_format: REPLAY_FORMAT,
            sim_version: iwbtg_sim::SIM_VERSION,
            package_id: def.id.clone(),
            gameplay_hash: self.gameplay_hash,
            engine_rev: def.engine_rev,
            rng_seed: u64::from(options.seed),
            start_mode,
            play_mode: options.mode,
            map: sim.map_id().to_owned(),
            mods: self.mods,
            client_build_id: format!("iwbtg-client {}", env!("CARGO_PKG_VERSION")),
        };
        let name = self.new_name(&def.id);
        Some(Recording {
            name,
            header,
            inputs: Vec::new(),
            splits: Vec::new(),
            checkpoints: Vec::new(),
            unsaved: false,
            listed: false,
            closed: false,
            ended: false,
        })
    }

    /// `replays/<game id>/<unix seconds>-<n>.iwbr`, not yet in the store.
    fn new_name(&mut self, game: &str) -> String {
        let secs = crate::stats::now_seconds();
        loop {
            self.started += 1;
            let name = format!("replays/{game}/{secs}-{}.iwbr", self.started);
            let taken = self
                .store
                .as_ref()
                .is_some_and(|s| matches!(s.read(&name), Ok(Some(_))));
            if !taken {
                return name;
            }
        }
    }

    /// Writes the replay when it has unsaved ticks, lists it in the index the first time and
    /// prints one line with the final hash (`iwbtg replay verify` prints the same).
    fn write(&mut self, sim: &Sim) {
        let (Some(store), Some(rec)) = (&self.store, self.current.as_mut()) else {
            return;
        };
        if !rec.unsaved || rec.inputs.is_empty() {
            return;
        }
        let final_hash = sim.state_hash();
        let replay = Replay {
            header: rec.header.clone(),
            inputs: rec.inputs.clone(),
            trailer: Trailer {
                total_ticks: rec.inputs.len() as u64,
                splits: rec.splits.clone(),
                ranked_ticks: sim.ranked_ticks(),
                checkpoints: rec.checkpoints.clone(),
                final_hash,
            },
        };
        let bytes = match replay.encode() {
            Ok(b) => b,
            Err(e) => {
                storage::warn(&format!("replay not saved: {e}"));
                return;
            }
        };
        if let Err(e) = store.write(&rec.name, &bytes) {
            storage::warn(&format!("replay not saved: {e}"));
            return;
        }
        rec.unsaved = false;
        eprintln!(
            "replay: {} ({} ticks, {} bytes) hash {}",
            store.describe(&rec.name),
            rec.inputs.len(),
            bytes.len(),
            hex(&final_hash)
        );
        if !rec.listed {
            rec.listed = true;
            if let Err(e) = list(store, &rec.name, self.prune.then_some(WEB_KEEP)) {
                storage::warn(&format!("replay index {e}"));
            }
        }
    }
}

/// The state hash truncated to the checkpoint's 16 bytes.
fn checkpoint(sim: &Sim) -> [u8; 16] {
    let h = sim.state_hash();
    let mut out = [0; 16];
    out.copy_from_slice(&h[..16]);
    out
}

/// Adds `name` to the index; with `keep`, removes replays past that many ([`to_remove`]), never
/// one a stats board points at unless the boards alone hold more than `keep`. When `stats.toml`
/// cannot be read nothing is removed this time.
fn list(store: &Store, name: &str, keep: Option<usize>) -> Result<(), String> {
    let text = store.read_text(INDEX)?.unwrap_or_default();
    let mut names: Vec<&str> = text.lines().filter(|l| !l.is_empty() && *l != name).collect();
    names.push(name);
    if let Some(keep) = keep
        && names.len() > keep
        && let Some((pbs, top)) = crate::stats::board_replays(store)
    {
        let gone = to_remove(&names, keep, &pbs, &top);
        for g in &gone {
            store.remove(g)?;
        }
        names.retain(|n| !gone.contains(n));
    }
    let mut out = names.join("\n");
    out.push('\n');
    store.write_text(INDEX, &out)
}

/// Which of `names` (the index, oldest first; the last was just written) to remove so that at
/// most `keep` remain: first the oldest replays no board points at, then the oldest of the
/// boards' top runs that are not a PB (`pbs`, `top`: `stats::board_replays`). A PB's replay and
/// the one just written are never removed, so more than `keep` remain when the PBs alone are
/// that many.
fn to_remove<'a>(
    names: &[&'a str],
    keep: usize,
    pbs: &BTreeSet<String>,
    top: &BTreeSet<String>,
) -> Vec<&'a str> {
    let excess = names.len().saturating_sub(keep);
    let Some((_, older)) = names.split_last() else {
        return Vec::new();
    };
    let free = older.iter().filter(|n| !pbs.contains(**n) && !top.contains(**n));
    let ranked = older.iter().filter(|n| top.contains(**n) && !pbs.contains(**n));
    free.chain(ranked).take(excess).copied().collect()
}

/// Lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Records every run. `keep: false` (a `--screenshot` check) records nothing.
pub struct ReplaysPlugin {
    pub keep: bool,
    pub gameplay_hash: [u8; 32],
    pub mods: ModSet,
}

impl Plugin for ReplaysPlugin {
    fn build(&self, app: &mut App) {
        let store = if self.keep { Store::data() } else { None };
        app.insert_resource(Recorder::new(store, self.gameplay_hash, self.mods))
            // Bevy's own exit on the last window closing runs in `Last` too; the app stops after
            // that frame, so this must run after it in the same frame.
            .add_systems(Last, finish_on_exit.after(bevy::window::ExitSystems));
    }
}

/// A retry (`retry.rs`) ends the run: its replay is written before the new sim replaces it.
pub(crate) fn reset(world: &mut World) {
    let Some(mut recorder) = world.remove_resource::<Recorder>() else {
        return;
    };
    recorder.finish(&world.non_send::<Game>().sim);
    world.insert_resource(recorder);
}

/// Closing the window ends the run: its replay is written while the sim is still there.
fn finish_on_exit(mut exit: MessageReader<AppExit>, game: NonSend<Game>, mut recorder: ResMut<Recorder>) {
    if exit.read().next().is_some() {
        recorder.finish(&game.sim);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iwbtg_sim::input::Action;
    use iwbtg_sim::sim::SimOptions;
    use iwbtg_sim::sound::DllMode;

    use super::*;

    fn temp(name: &str) -> Store {
        let d = std::env::temp_dir().join(format!("iwbtg-replays-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        Store::at(d)
    }

    fn demo() -> crate::package::Package {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo.pkg");
        crate::package::open(&dir, false).unwrap()
    }

    /// Runs `ticks` ticks of `sim` through the recorder, holding right and tapping jump.
    fn play(sim: &mut Sim, rec: &mut Recorder, ticks: u64) {
        for t in 0..ticks {
            let jump = t % 40 == 5;
            let frame = InputFrame {
                held: Action::Right.bit() | if jump { Action::Jump.bit() } else { 0 },
                pressed: if jump { Action::Jump.bit() } else { 0 },
                released: if t % 40 == 6 { Action::Jump.bit() } else { 0 },
            };
            let step = sim.step(frame);
            rec.record(sim, frame, &step.splits);
        }
    }

    #[test]
    fn seeds_differ() {
        let seeds: std::collections::BTreeSet<u32> = (0..8).map(|_| fresh_seed()).collect();
        assert!(seeds.len() > 1, "{seeds:?}");
    }

    /// A ranked demo run with a random seed: the file decodes, carries the run's setup, and a
    /// fresh sim on its header and inputs reaches the same final hash and checkpoints.
    #[test]
    fn a_recorded_run_replays_to_the_same_hash() {
        let pkg = demo();
        let store = temp("same");
        let seed = fresh_seed();
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed,
            dll_mode: DllMode::Model,
            mode: PlayMode::Stripped,
            map: None,
        };
        let mut sim = Sim::new(Arc::clone(&pkg.def), options.clone()).unwrap();
        let mut rec = Recorder::new(Some(store.clone()), pkg.gameplay_hash, ModSet::NONE);
        play(&mut sim, &mut rec, 520);
        let name = rec.current_name().unwrap().to_owned();
        assert!(
            name.starts_with("replays/demo/") && name.ends_with("-1.iwbr"),
            "{name}"
        );
        rec.finish(&sim);
        assert!(rec.current_name().is_none());

        let replay = Replay::decode(&store.read(&name).unwrap().unwrap()).unwrap();
        let h = &replay.header;
        assert_eq!(h.rng_seed, u64::from(seed));
        assert_eq!((h.play_mode, h.map.as_str()), (PlayMode::Stripped, "main"));
        assert_eq!(h.gameplay_hash, pkg.gameplay_hash);
        assert_eq!(h.start_mode, StartMode::NewGame);
        assert_eq!(replay.inputs.len(), 520);
        assert_eq!(replay.trailer.final_hash, sim.state_hash());
        let ticks: Vec<u64> = replay.trailer.checkpoints.iter().map(|c| c.0).collect();
        assert_eq!(ticks, vec![500]);
        assert_eq!(store.read_text(INDEX).unwrap().unwrap(), format!("{name}\n"));

        let mut again = Sim::new(Arc::clone(&pkg.def), options).unwrap();
        for (i, f) in replay.inputs.iter().enumerate() {
            again.step(*f);
            if i == 499 {
                assert_eq!(again.state_hash()[..16], replay.trailer.checkpoints[0].1);
            }
        }
        assert_eq!(again.state_hash(), replay.trailer.final_hash);
    }

    /// Nothing recorded since the last write writes nothing; the next run is a new file.
    #[test]
    fn each_run_is_its_own_file_and_the_web_keeps_the_newest() {
        let pkg = demo();
        let store = temp("runs");
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed: 1,
            dll_mode: DllMode::Model,
            mode: PlayMode::Original,
            map: None,
        };
        let mut rec = Recorder::new(Some(store.clone()), pkg.gameplay_hash, ModSet::NONE);
        rec.prune = true;
        let mut names = Vec::new();
        for _ in 0..WEB_KEEP + 2 {
            let mut sim = Sim::new(Arc::clone(&pkg.def), options.clone()).unwrap();
            play(&mut sim, &mut rec, 3);
            names.push(rec.current_name().unwrap().to_owned());
            rec.finish(&sim);
            rec.finish(&sim);
        }
        let index = store.read_text(INDEX).unwrap().unwrap();
        let listed: Vec<&str> = index.lines().collect();
        assert_eq!(listed, names[2..].iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(store.read(&names[0]).unwrap(), None, "the oldest is removed");
        assert!(store.read(&names[2]).unwrap().is_some());
    }

    /// The issue's check (#607): 60 runs on the web, the first is the PB and the second the
    /// board's #2: both replays stay, the oldest of the others go.
    #[test]
    fn pruning_keeps_the_replays_a_board_points_at() {
        use crate::stats::{Book, Run, board_key, map_key};
        let pkg = demo();
        let store = temp("board");
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed: 1,
            dll_mode: DllMode::Model,
            mode: PlayMode::Stripped,
            map: None,
        };
        let mut rec = Recorder::new(Some(store.clone()), pkg.gameplay_hash, ModSet::NONE);
        rec.prune = true;
        let mut names = Vec::new();
        for i in 0..60 {
            let mut sim = Sim::new(Arc::clone(&pkg.def), options.clone()).unwrap();
            play(&mut sim, &mut rec, 3);
            names.push(rec.current_name().unwrap().to_owned());
            rec.finish(&sim);
            if i == 1 {
                let mut book = Book::default();
                let board = book
                    .map_mut(&map_key("demo", "main"), "demo", 50)
                    .boards
                    .entry(board_key(true, "NM"))
                    .or_default();
                for (ticks, replay) in [(100, &names[0]), (200, &names[1])] {
                    board.add(Run {
                        ticks,
                        deaths: 0,
                        splits: vec![ticks],
                        segment_deaths: vec![0],
                        finished_at: 0,
                        replay: Some(replay.clone()),
                    });
                }
                store.write_text(crate::stats::FILE, &book.to_text()).unwrap();
            }
        }
        let index = store.read_text(INDEX).unwrap().unwrap();
        let listed: Vec<&str> = index.lines().collect();
        let mut want = vec![names[0].as_str(), names[1].as_str()];
        want.extend(names[12..].iter().map(String::as_str));
        assert_eq!(listed, want);
        assert!(store.read(&names[0]).unwrap().is_some(), "the PB's replay stays");
        assert!(store.read(&names[1]).unwrap().is_some(), "the #2's replay stays");
        assert_eq!(store.read(&names[2]).unwrap(), None);
        assert_eq!(store.read(&names[11]).unwrap(), None);

        // An unreadable stats file removes nothing.
        store.write_text(crate::stats::FILE, "not = [toml").unwrap();
        let mut sim = Sim::new(Arc::clone(&pkg.def), options).unwrap();
        play(&mut sim, &mut rec, 3);
        rec.finish(&sim);
        assert_eq!(
            store.read_text(INDEX).unwrap().unwrap().lines().count(),
            WEB_KEEP + 1
        );
    }

    /// When the boards alone hold more than the cap: free replays go first, then the oldest
    /// top runs; PBs and the replay just written never go.
    #[test]
    fn past_the_cap_top_runs_go_oldest_first_and_pbs_stay() {
        let set = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<BTreeSet<String>>();
        let names = ["a", "b", "c", "d", "e", "f"];
        let pbs = set(&["a"]);
        let top = set(&["b", "c", "d", "e"]);
        assert_eq!(to_remove(&names, 3, &pbs, &top), vec!["b", "c", "d"]);
        assert_eq!(to_remove(&names, 5, &pbs, &top), vec!["b"]);
        let names = ["a", "x", "b", "y"];
        assert_eq!(to_remove(&names, 2, &pbs, &top), vec!["x", "b"]);
        assert_eq!(
            to_remove(&names, 0, &set(&["a", "b", "x"]), &top),
            Vec::<&str>::new()
        );
        assert_eq!(to_remove(&names, 4, &pbs, &top), Vec::<&str>::new());
    }

    /// A run that resumes the save file records the slot, so it can be replayed.
    #[test]
    fn a_run_from_a_save_keeps_the_slot() {
        let pkg = demo();
        let store = temp("save");
        let slot = iwbtg_sim::save::SaveSlot {
            room: pkg.def.start_room,
            x: iwbtg_sim::num::Real::new(96.0),
            y: iwbtg_sim::num::Real::new(500.0),
            flipped: false,
            facing: 1,
            difficulty: 0,
            deaths: 7,
            time: 1234,
            bosses: 0,
            items: 0,
            clear: false,
        };
        let options = SimOptions {
            start: StartPoint::FromSave(slot.clone()),
            seed: 0,
            dll_mode: DllMode::Model,
            mode: PlayMode::Original,
            map: None,
        };
        let mut sim = Sim::new(Arc::clone(&pkg.def), options).unwrap();
        let mut rec = Recorder::new(Some(store.clone()), pkg.gameplay_hash, ModSet::NONE);
        play(&mut sim, &mut rec, 2);
        let name = rec.current_name().unwrap().to_owned();
        rec.finish(&sim);
        let replay = Replay::decode(&store.read(&name).unwrap().unwrap()).unwrap();
        assert_eq!(
            replay.header.start_mode,
            StartMode::FromSave(postcard::to_stdvec(&slot).unwrap())
        );
    }
}
