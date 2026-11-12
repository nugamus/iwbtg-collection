//! Local stats (#40; docs/architecture/client/stats.md): PBs, your top runs, gold segments and
//! sum of best per map, mode and mod combination; deaths per segment for all time and for this
//! session; account totals for the player card. Kept in `stats.toml` in the data area of
//! `storage.rs`. Play original records here too, on its own `original/...` boards: personal
//! stats only, never uploaded (docs/product.md, Modes).
//!
//! The screens (results, live leaderboard, player card, game select) only read the [`Stats`]
//! resource; this module is the only writer. Nothing here touches the sim.

#[allow(dead_code, reason = "the screens read the book (#494, #496, #151)")]
mod book;
mod track;

use std::collections::BTreeSet;

use bevy::prelude::*;
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::stripped::PlayMode;

#[allow(unused_imports, reason = "the screens read these (#494, #496, #151)")]
pub use book::{Board, Book, MapStats, Recorded, Run, TOP_RUNS, Totals, board_key, map_key};
use track::{Event, Observation, Tracker};

use crate::storage::{self, Store};
use crate::view::{Game, SimTick};

/// The file in the data area.
pub(crate) const FILE: &str = "stats.toml";
/// Where an unreadable file is moved, so a fresh book never overwrites it.
const BROKEN: &str = "stats.broken.toml";
/// Unsaved deaths and play time are written at least this often, in seconds of real time.
const FLUSH_SECONDS: f32 = 30.0;

/// This session's deaths (since the client started), by map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    pub deaths: u64,
    /// Per [`map_key`], per segment.
    pub segment_deaths: std::collections::BTreeMap<String, Vec<u32>>,
}

/// The run being played and which board it counts for.
#[derive(Debug)]
struct Current {
    map: String,
    game: String,
    tick_rate: u32,
    board: String,
    /// Only a run started as a new game goes on a board; a clear from a save still counts as
    /// a clear.
    on_board: bool,
    tracker: Tracker,
}

/// The local stats. The screens read `book`, `session` and `last`.
#[derive(Resource)]
#[allow(
    dead_code,
    reason = "`session` and `last` are read by the screens (#494, #496)"
)]
pub struct Stats {
    pub book: Book,
    pub session: Session,
    /// What the last cleared run changed (the results screen), `None` before one.
    pub last: Option<Recorded>,
    store: Option<Store>,
    dirty: bool,
    since_flush: f32,
    current: Option<Current>,
    mods: ModSet,
}

impl Stats {
    /// The stats kept in `store` (`None`: this session keeps nothing).
    fn open(store: Option<Store>, mods: ModSet) -> Stats {
        let book = store.as_ref().map(load).unwrap_or_default();
        Stats {
            book,
            session: Session::default(),
            last: None,
            store,
            dirty: false,
            since_flush: 0.0,
            current: None,
            mods,
        }
    }

    /// Writes the book when it changed. A failed write is a warning; the next change tries
    /// again.
    fn flush(&mut self) {
        self.since_flush = 0.0;
        if !self.dirty {
            return;
        }
        let Some(store) = &self.store else {
            self.dirty = false;
            return;
        };
        match store.write_text(FILE, &self.book.to_text()) {
            Ok(()) => self.dirty = false,
            Err(e) => storage::warn(&format!("stats {e}")),
        }
    }

    /// Applies one tick's events.
    fn apply(&mut self, cur: &Current, events: Vec<Event>, tick_seconds: f64, replay: Option<&str>) {
        for event in events {
            self.dirty = true;
            let map = self.book.map_mut(&cur.map, &cur.game, cur.tick_rate);
            match event {
                Event::Ticked => {
                    map.played_ticks = map.played_ticks.saturating_add(1);
                    map.play_seconds += tick_seconds;
                }
                Event::Died { segment, n } => {
                    map.add_deaths(segment, n);
                    self.session.deaths = self.session.deaths.saturating_add(u64::from(n));
                    let s = self.session.segment_deaths.entry(cur.map.clone()).or_default();
                    if s.len() <= segment {
                        s.resize(segment + 1, 0);
                    }
                    s[segment] = s[segment].saturating_add(n);
                }
                Event::Cleared {
                    ticks,
                    deaths,
                    splits,
                    segment_deaths,
                } => {
                    map.clears = map.clears.saturating_add(1);
                    if cur.on_board {
                        let run = Run {
                            ticks,
                            deaths,
                            splits,
                            segment_deaths,
                            finished_at: now_seconds(),
                            replay: replay.map(str::to_owned),
                        };
                        let board = map.boards.entry(cur.board.clone()).or_default();
                        let rec = board.add(run);
                        eprintln!("{}", clear_line(ticks, deaths, cur.tick_rate, &rec));
                        self.last = Some(rec);
                    } else {
                        eprintln!(
                            "stats: cleared in {} (from a save: counted as a clear, not on a board)",
                            fmt_ticks(ticks, cur.tick_rate)
                        );
                    }
                    self.flush();
                }
            }
        }
    }
}

impl Drop for Stats {
    /// Closing the window keeps the last deaths and play time.
    fn drop(&mut self) {
        self.flush();
    }
}

/// Reads the file. A file this build cannot read is moved to `stats.broken.toml` with a
/// warning, and the session starts an empty book.
fn load(store: &Store) -> Book {
    match store.read_text(FILE) {
        Ok(None) => Book::default(),
        Ok(Some(text)) => Book::parse(&text).unwrap_or_else(|e| {
            storage::warn(&format!(
                "stats {}: {e}; kept as {}, starting empty",
                store.describe(FILE),
                store.describe(BROKEN)
            ));
            if let Err(e) = store.write_text(BROKEN, &text) {
                storage::warn(&format!("stats {e}"));
            }
            Book::default()
        }),
        Err(e) => {
            storage::warn(&format!("stats {e}"));
            Book::default()
        }
    }
}

/// The replays the boards in `store`'s `stats.toml` point at: the PBs' and the other top runs'
/// ([`Book::board_replays`]). `None` when the file is there but cannot be read, so the caller
/// removes nothing.
pub(crate) fn board_replays(store: &Store) -> Option<(BTreeSet<String>, BTreeSet<String>)> {
    let book = match store.read_text(FILE) {
        Ok(None) => Book::default(),
        Ok(Some(text)) => Book::parse(&text).ok()?,
        Err(_) => return None,
    };
    let (pbs, top) = book.board_replays();
    let owned = |s: BTreeSet<&str>| s.into_iter().map(str::to_owned).collect();
    Some((owned(pbs), owned(top)))
}

/// Seconds since 1970, for a run's date.
pub(crate) fn now_seconds() -> u64 {
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        (web_sys::js_sys::Date::now() / 1000.0) as u64
    }
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
}

/// `m:ss.cc` (or `h:mm:ss.cc`) from ticks at `rate` per second.
pub fn fmt_ticks(ticks: u64, rate: u32) -> String {
    let rate = u64::from(rate.max(1));
    let cs = ticks * 100 / rate;
    let (h, m, s, c) = (cs / 360_000, cs / 6000 % 60, cs / 100 % 60, cs % 100);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{c:02}")
    } else {
        format!("{m}:{s:02}.{c:02}")
    }
}

/// The terminal line for a clear on a board.
fn clear_line(ticks: u64, deaths: u32, rate: u32, rec: &Recorded) -> String {
    let t = |v| fmt_ticks(v, rate);
    let pb = match rec.pb_before {
        None => "first clear".to_owned(),
        Some(p) if ticks < p => format!("new PB, -{} (was {})", t(p - ticks), t(p)),
        Some(p) => format!("PB {} (+{})", t(p), t(ticks - p)),
    };
    let golds = rec.gold.iter().filter(|g| **g).count();
    let place = rec
        .place
        .map_or("outside the top 100".to_owned(), |p| format!("#{p} of your runs"));
    let sob = rec
        .sum_of_best
        .map_or(String::new(), |s| format!(", sum of best {}", t(s)));
    format!(
        "stats: cleared in {} ({deaths} deaths): {pb}, {place}, {golds} gold segment(s){sob}",
        t(ticks)
    )
}

/// Adds the local stats. `keep: false` (a `--screenshot` check) reads and writes nothing.
pub struct StatsPlugin {
    pub keep: bool,
    pub mods: ModSet,
}

impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        let store = if self.keep { Store::data() } else { None };
        app.insert_resource(Stats::open(store, self.mods))
            .add_systems(FixedUpdate, watch.after(SimTick))
            .add_systems(Update, flush_now_and_then);
    }
}

/// After every tick: deaths, play time and the clear go into the book. A run's times are the
/// HUD's: `Sim::ranked_ticks` plus the ticks the timer ran in the pause menu so far
/// (`pause::Menu::paused_ticks`, #623), so a split counts the pauses before it.
fn watch(
    game: NonSend<Game>,
    fixed: Res<Time<Fixed>>,
    mut stats: ResMut<Stats>,
    replays: Option<Res<crate::replays::Recorder>>,
    menu: Option<Res<crate::pause::Menu>>,
) {
    let sim = &game.sim;
    let def = sim.def();
    let mut cur = stats.current.take().unwrap_or_else(|| {
        let opts = sim.options();
        let ranked = opts.mode == PlayMode::Stripped;
        Current {
            map: map_key(&def.id, sim.map_id()),
            game: def.id.clone(),
            tick_rate: def.tick_rate,
            board: board_key(ranked, &stats.mods.codes()),
            on_board: matches!(opts.start, StartPoint::NewGame { .. }),
            tracker: Tracker::new(def.timing.stages.len()),
        }
    });
    let world = sim.world();
    let events = cur.tracker.observe(Observation {
        tick: world.tick,
        deaths: world.engine.run.deaths,
        stages_done: world.timing.stages_done,
        ended: world.timing.ended,
        ranked_ticks: sim
            .ranked_ticks()
            .saturating_add(menu.map_or(0, |m| m.paused_ticks())),
    });
    let replay = replays.as_deref().and_then(|r| r.current_name());
    stats.apply(&cur, events, fixed.timestep().as_secs_f64(), replay);
    stats.current = Some(cur);
}

/// Writes unsaved deaths and play time every [`FLUSH_SECONDS`].
fn flush_now_and_then(time: Res<Time<Real>>, mut stats: ResMut<Stats>) {
    stats.since_flush += time.delta_secs();
    if stats.since_flush >= FLUSH_SECONDS {
        stats.flush();
    }
}

/// A retry (`retry.rs`) starts a new run: the old one's deaths are saved, the next tick starts
/// a new tracker.
pub(crate) fn reset(world: &mut World) {
    if let Some(mut stats) = world.get_resource_mut::<Stats>() {
        stats.current = None;
        stats.flush();
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use iwbtg_sim::input::{Action, InputFrame};

    use super::*;

    fn temp(name: &str) -> Store {
        let d = std::env::temp_dir().join(format!("iwbtg-stats-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        Store::at(d)
    }

    #[test]
    fn times_read_as_minutes_seconds_and_hundredths() {
        assert_eq!(fmt_ticks(0, 50), "0:00.00");
        assert_eq!(fmt_ticks(51, 50), "0:01.02");
        assert_eq!(fmt_ticks(50 * 3723, 50), "1:02:03.00");
    }

    /// The demo kid presses suicide: the death lands in the book and the session, a retry
    /// writes the file, and the next start reads it back.
    #[test]
    fn deaths_are_counted_saved_and_read_back() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let store = temp("deaths");
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(Some(store.clone()), ModSet::NONE));
        let suicide = InputFrame {
            held: Action::Suicide.bit(),
            pressed: Action::Suicide.bit(),
            released: 0,
        };
        world.run_system_once(watch).unwrap();
        let mut ticks = 0;
        while world.resource::<Stats>().session.deaths == 0 {
            assert!(ticks < 100, "no death in 100 ticks");
            world.non_send_mut::<Game>().sim.step(suicide);
            world.run_system_once(watch).unwrap();
            ticks += 1;
        }
        let id = map_key(&pkg.def.id, "main");
        let stats = world.resource::<Stats>();
        let map = stats.book.map(&id).unwrap();
        assert_eq!(map.deaths, 1);
        assert_eq!(map.played_ticks, ticks);
        assert!((map.play_seconds - ticks as f64 / 50.0).abs() < 1e-9);
        assert_eq!(stats.session.segment_deaths[&id], vec![1]);
        assert_eq!(store.read_text(FILE).unwrap(), None, "not written before a flush");
        reset(&mut world);
        let again = Stats::open(Some(store), ModSet::NONE);
        assert_eq!(again.book.map(&id).unwrap().deaths, 1);
        assert_eq!(again.session, Session::default(), "a session starts at zero");
    }

    /// Plays `demo-clear.pkg` once with scripted input: in each room `waits[i]` idle ticks, then
    /// right held and one jump (over the block in the first room, over the spike in the goal
    /// room). Returns the clear's record and its run.
    fn play_demo_clear(world: &mut World, waits: [u32; 2]) -> (Recorded, Run) {
        play_demo_clear_paused(world, waits, 0)
    }

    /// [`play_demo_clear`] with two pauses of `pause` ticks each (when not 0) in the pause menu:
    /// after tick 5 and after the stage split, as `pause::count` records them.
    fn play_demo_clear_paused(world: &mut World, waits: [u32; 2], pause: u64) -> (Recorded, Run) {
        let (right, jump) = (Action::Right.bit(), Action::Jump.bit());
        let mut rooms = Vec::new();
        let (mut in_room, mut jumped, mut jump_left) = (0u32, false, 0u32);
        for t in 0..2000u32 {
            let game = world.non_send::<Game>();
            let w = game.sim.world();
            if w.timing.ended {
                let stats = world.resource::<Stats>();
                let rec = stats.last.clone().expect("a clear is recorded");
                let cur = stats.current.as_ref().unwrap();
                let board = &stats.book.map(&cur.map).unwrap().boards[&cur.board];
                let run = board.runs[rec.place.unwrap() - 1].clone();
                return (rec, run);
            }
            if rooms.last() != Some(&w.room) {
                rooms.push(w.room);
                (in_room, jumped) = (0, false);
            }
            let kid_x = w
                .kid(&game.sim.rules().tick)
                .and_then(|id| w.get(id))
                .map(|k| k.m.x().get());
            let wait = waits[rooms.len().min(2) - 1];
            let mut frame = InputFrame::default();
            if in_room >= wait {
                frame.held |= right;
                if in_room == wait {
                    frame.pressed |= right;
                }
                // The block's left edge is at x 320 in the first room, the spike's at 352 in
                // the goal room: jump a little before either.
                if !jumped && kid_x.is_some_and(|x| x >= 300.0) {
                    jumped = true;
                    jump_left = 20;
                    frame.pressed |= jump;
                }
            } else if in_room > 0 {
                frame.released |= right;
            }
            if jump_left > 0 {
                frame.held |= jump;
                jump_left -= 1;
                if jump_left == 0 {
                    frame.released |= jump;
                }
            }
            in_room += 1;
            let (tick, stages) = (w.tick, w.timing.stages_done);
            if pause > 0 {
                let mut menu = world.resource_mut::<crate::pause::Menu>();
                let n = menu.pauses.len();
                if (n == 0 && tick == 5) || (n == 1 && stages == 1) {
                    menu.pauses.push((tick, pause));
                }
            }
            world.non_send_mut::<Game>().sim.step(frame);
            world.run_system_once(watch).unwrap();
            assert_eq!(
                world.resource::<Stats>().session.deaths,
                0,
                "the scripted kid died at tick {t}"
            );
        }
        panic!("no clear in 2000 ticks");
    }

    /// The clear demo (#589): two scripted clears give two clear lines, the first a first
    /// clear, the second with the PB comparison and sum of best. A run has two segments: start
    /// to the goal room (the stage split), then to the clear. The first run idles 10 ticks in
    /// the goal room, the second 20 in the first room: the second is 0.20 s slower but golds
    /// the goal room, so the sum of best drops below the PB.
    #[test]
    fn the_clear_demo_clears_twice_with_a_split() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo-clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        assert_eq!(pkg.def.timing.stages.len(), 1);
        let store = temp("clear");
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(Some(store.clone()), ModSet::NONE));
        let mut runs = Vec::new();
        for waits in [[0, 10], [20, 0]] {
            let (rec, run) = play_demo_clear(&mut world, waits);
            assert_eq!(
                run.splits.len(),
                2,
                "one stage split and the end: {:?}",
                run.splits
            );
            assert!(run.splits[0] < run.splits[1]);
            runs.push((clear_line(run.ticks, run.deaths, 50, &rec), run));
            crate::view::restart_run(&mut world).unwrap();
            reset(&mut world);
        }
        let (first, second) = (&runs[0], &runs[1]);
        assert_eq!(second.1.ticks, first.1.ticks + 10);
        assert!(
            first
                .0
                .contains("first clear, #1 of your runs, 2 gold segment(s)"),
            "{}",
            first.0
        );
        assert!(
            second
                .0
                .contains("(+0:00.20), #2 of your runs, 1 gold segment(s), sum of best"),
            "{}",
            second.0
        );
        let id = map_key(&pkg.def.id, "main");
        let again = Stats::open(Some(store), ModSet::NONE);
        assert_eq!(
            again.book.map(&id).unwrap().clears,
            2,
            "both clears are in the file"
        );
    }

    /// #623: a run paused twice (before and after the stage split) records the HUD's times:
    /// the clear and each split carry the paused ticks before them.
    #[test]
    fn paused_ticks_count_in_the_clear_and_the_splits() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo-clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(None, ModSet::NONE));
        world.insert_resource(crate::pause::Menu::default());
        let (_, plain) = play_demo_clear(&mut world, [0, 0]);
        crate::view::restart_run(&mut world).unwrap();
        reset(&mut world);
        crate::pause::reset(&mut world);
        let (_, paused) = play_demo_clear_paused(&mut world, [0, 0], 25);
        assert_eq!(world.resource::<crate::pause::Menu>().paused_ticks(), 50);
        assert_eq!(paused.ticks, plain.ticks + 50);
        assert_eq!(
            paused.splits,
            vec![plain.splits[0] + 25, plain.splits[1] + 50],
            "the first split has only the pause before it"
        );
    }

    #[test]
    fn an_unreadable_file_is_kept_aside_not_overwritten() {
        let store = temp("broken");
        store.write_text(FILE, "version = \"x\"").unwrap();
        let stats = Stats::open(Some(store.clone()), ModSet::NONE);
        assert!(stats.book.maps.is_empty());
        assert_eq!(
            store.read_text(BROKEN).unwrap().as_deref(),
            Some("version = \"x\"")
        );
    }

    #[test]
    fn the_clear_line_says_pb_and_sum_of_best() {
        let mut b = Board::default();
        let mk = |s: Vec<u64>| Run {
            ticks: *s.last().unwrap(),
            deaths: 0,
            segment_deaths: vec![0; s.len()],
            splits: s,
            finished_at: 0,
            replay: None,
        };
        let first = b.add(mk(vec![100, 200]));
        assert_eq!(
            clear_line(200, 2, 50, &first),
            "stats: cleared in 0:04.00 (2 deaths): first clear, #1 of your runs, 2 gold segment(s), sum of best 0:04.00"
        );
        let second = b.add(mk(vec![90, 210]));
        assert_eq!(
            clear_line(210, 0, 50, &second),
            "stats: cleared in 0:04.20 (0 deaths): PB 0:04.00 (+0:00.20), #2 of your runs, 1 gold segment(s), sum of best 0:03.80"
        );
    }
}
