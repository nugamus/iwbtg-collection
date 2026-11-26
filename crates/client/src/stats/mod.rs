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
use iwbtg_sim::sim::Sim;
use iwbtg_sim::stripped::PlayMode;

#[allow(unused_imports, reason = "the screens read these (#494, #496, #151)")]
pub use book::{
    Board, Book, DEATH_LOG_RUNS, DeathLog, DeathMark, Furthest, GameMode, MapStats, ModeStats, Recorded,
    Run, SessionSummary, TOP_RUNS, Totals, board_key, map_key, mode_key,
};
use track::{Event, Observation, Spot, Tracker};

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

/// One session: a run from the moment it opens until it is left or retried (#1623). The Classic
/// session summary (m5 A8, A9) and the session panel beside a Classic run (A7) read it, while it
/// is open ([`Stats::open_session`]) and after it ended ([`Stats::ended_session`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionLog {
    pub game: String,
    /// [`map_key`].
    pub map: String,
    /// Time Attack (`true`) or Classic.
    pub ranked: bool,
    pub tick_rate: u32,
    /// Ticks played in the session.
    pub ticks: u64,
    /// Seconds played (each tick at the rate it ran).
    pub seconds: f64,
    pub deaths: u64,
    /// Each death: the session tick and the room the kid was last seen in.
    pub death_log: Vec<SessionDeath>,
    /// Each room entered, in order, the start room first; `new` when it was never entered
    /// before on this map.
    pub rooms: Vec<RoomVisit>,
    /// Each save the game wrote (its save slot changed): when and in which room.
    pub saves: Vec<SaveTouch>,
    /// The run's end fired in this session (the game was finished).
    pub finished: bool,
    /// When it started, seconds since 1970.
    pub started_at: u64,
}

/// A death in a [`SessionLog`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionDeath {
    pub tick: u64,
    /// Package room id; `None` before a kid was seen.
    pub room: Option<String>,
}

/// A room entered in a [`SessionLog`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomVisit {
    pub tick: u64,
    pub room: String,
    pub new: bool,
}

/// A save in a [`SessionLog`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveTouch {
    pub tick: u64,
    pub room: String,
}

#[allow(dead_code, reason = "the Classic session summary and panel read it (#1624)")]
impl SessionLog {
    /// Deaths per room, most first (ties: the room died in first comes first).
    pub fn deaths_by_room(&self) -> Vec<(&str, u32)> {
        let mut rooms: Vec<(&str, u32)> = Vec::new();
        for d in &self.death_log {
            let Some(room) = d.room.as_deref() else {
                continue;
            };
            match rooms.iter_mut().find(|(r, _)| *r == room) {
                Some((_, n)) => *n += 1,
                None => rooms.push((room, 1)),
            }
        }
        rooms.sort_by_key(|a| std::cmp::Reverse(a.1));
        rooms
    }

    /// Rooms entered for the first time on this map.
    pub fn new_rooms(&self) -> usize {
        self.rooms.iter().filter(|r| r.new).count()
    }
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
    /// The package room ids by `RoomId` index, to name the rooms of the death log.
    rooms: Vec<String>,
    tracker: Tracker,
    /// Time Attack (`true`) or Classic: the mode its [`ModeStats`] count in.
    ranked: bool,
    /// This session so far (#1623).
    session: SessionLog,
    /// The room after the last tick, to see a change.
    room: Option<u32>,
    /// The save slot's spot after the last tick, to see a save.
    save: Option<(u32, i32, i32)>,
    /// The room the kid was last seen alive in.
    kid_room: Option<u32>,
}

/// The last cleared run on a board and where it went (the results screen, #41).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cleared {
    /// [`map_key`] and [`board_key`] of the board it went on.
    pub map: String,
    pub board: String,
    pub run: Run,
}

/// The local stats. The screens read `book`, `session`, `last` and `cleared`.
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
    /// The run `last` is about; set with it.
    pub cleared: Option<Cleared>,
    store: Option<Store>,
    dirty: bool,
    since_flush: f32,
    current: Option<Current>,
    /// The session that ended last (#1623), kept until the next one opens.
    ended: Option<SessionLog>,
    /// Text this build could not read from `stats.toml`, kept until it has been written to a
    /// free `stats.broken*` name (#1017). While it is set the original file is never replaced,
    /// so a failed backup (a full browser quota, a transient write error) cannot destroy it.
    recovery: Option<String>,
}

impl Stats {
    /// The stats kept in `store` (`None`: this session keeps nothing).
    pub(crate) fn open(store: Option<Store>) -> Stats {
        let (book, recovery) =
            store.as_ref().map_or_else(|| (Book::default(), None), load);
        let mut stats = Stats {
            book,
            session: Session::default(),
            last: None,
            cleared: None,
            store,
            dirty: false,
            since_flush: 0.0,
            current: None,
            ended: None,
            recovery,
        };
        // Try the backup at once; a success replaces the now-preserved file with the empty book.
        stats.flush();
        stats
    }

    /// The board key the current run counts on (`None` before its first tick).
    #[cfg(test)]
    pub(crate) fn board(&self) -> Option<&str> {
        self.current.as_ref().map(|c| c.board.as_str())
    }

    /// Whether the current run counts on its board (`None` before its first tick).
    #[cfg(test)]
    pub(crate) fn on_board(&self) -> Option<bool> {
        self.current.as_ref().map(|c| c.on_board)
    }

    /// Opens the current run's tracker from the state before its first tick (#1011), so that
    /// tick's play time, death, split or clear is counted. The first observation is only the
    /// baseline, so a run from a save keeps that save's counters and counts no historical death.
    /// Does nothing while a run's tracker is open; a retry or the end of the run clears it
    /// ([`reset`]).
    pub(crate) fn open_current(
        &mut self,
        sim: &Sim,
        mods: Option<&crate::mods::RunMods>,
        menu: Option<&crate::pause::Menu>,
        practice: bool,
    ) {
        if self.current.is_some() {
            return;
        }
        let def = sim.def();
        let opts = sim.options();
        let ranked = opts.mode == PlayMode::Stripped;
        // The selected map's chain, not the package `[timing]` (#1010): maps may have their own
        // stage lists.
        let mut tracker = Tracker::new(def.map_timing(sim.world().map).stages.len());
        let first = observation(sim, menu);
        tracker.observe(first);
        let map = map_key(&def.id, sim.map_id());
        let session = SessionLog {
            game: def.id.clone(),
            map: map.clone(),
            ranked,
            tick_rate: def.tick_rate,
            started_at: now_seconds(),
            ..SessionLog::default()
        };
        self.ended = None;
        let mut current = Current {
            map,
            game: def.id.clone(),
            tick_rate: def.tick_rate,
            // The board of the mods the run opened with (`mods::RunMods`).
            board: board_key(ranked, &mods.map_or(ModSet::NONE, |m| m.0).codes()),
            on_board: matches!(opts.start, StartPoint::NewGame { .. })
                && !practice
                && !mods.is_some_and(|m| m.0.contains(iwbtg_sim::mods::Mod::SaveStates)),
            rooms: def.rooms.iter().map(|r| r.name.clone()).collect(),
            tracker,
            ranked,
            session,
            room: None,
            save: first.save,
            kid_room: first.kid.map(|k| k.room),
        };
        self.enter(&mut current, first.room);
        self.current = Some(current);
    }

    /// The session being played, `None` between runs (#1623).
    #[allow(dead_code, reason = "the Classic session panel reads it (#1624)")]
    pub fn open_session(&self) -> Option<&SessionLog> {
        self.current.as_ref().map(|c| &c.session)
    }

    /// The session that ended last, until the next run opens (#1623).
    #[allow(dead_code, reason = "the Classic session summary reads it (#1624)")]
    pub fn ended_session(&self) -> Option<&SessionLog> {
        self.ended.as_ref()
    }

    /// Notes entering `room` (a package room index) in the session, the map's seen rooms and
    /// the mode's furthest room.
    fn enter(&mut self, cur: &mut Current, room: u32) {
        if cur.room == Some(room) {
            return;
        }
        cur.room = Some(room);
        let Some(name) = cur.rooms.get(room as usize).cloned() else {
            return;
        };
        let map = self.book.map_mut(&cur.map, &cur.game, cur.tick_rate);
        let new = map.rooms_seen.insert(name.clone());
        let mode = map.mode_mut(cur.ranked);
        if mode.furthest.as_ref().is_none_or(|f| room > f.index) {
            mode.furthest = Some(Furthest {
                index: room,
                room: name.clone(),
            });
        }
        self.dirty = true;
        cur.session.rooms.push(RoomVisit {
            tick: cur.session.ticks,
            room: name,
            new,
        });
    }

    /// Closes the open session: its time and deaths become the mode's last session (#1623).
    fn end_session(&mut self) {
        let Some(cur) = self.current.take() else {
            return;
        };
        let now = now_seconds();
        if cur.session.ticks > 0 {
            let mode = self
                .book
                .map_mut(&cur.map, &cur.game, cur.tick_rate)
                .mode_mut(cur.ranked);
            mode.last_played = Some(now);
            mode.last_session = Some(SessionSummary {
                seconds: cur.session.seconds,
                deaths: cur.session.deaths,
                ended_at: now,
            });
            self.dirty = true;
        }
        self.ended = Some(cur.session);
    }

    /// Writes the book when it changed. A failed write is a warning; the next change tries
    /// again. An unreadable file is backed up first: until that succeeds it is never replaced
    /// (#1017).
    fn flush(&mut self) {
        self.since_flush = 0.0;
        if self.preserve() {
            return;
        }
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

    /// Writes the text [`load`] could not read to a free `stats.broken*` name (#1017). Returns
    /// whether preservation is still pending: a failed backup is warned about here and retried
    /// on the next flush, and the original file is left untouched meanwhile. A success marks
    /// the empty book worth writing, so the preserved file is replaced on this same flush.
    fn preserve(&mut self) -> bool {
        let Some(text) = self.recovery.take() else {
            return false;
        };
        // `None`: this session keeps nothing, so there is nowhere to write it.
        let Some(store) = self.store.clone() else {
            return false;
        };
        match backup_name(&store).and_then(|name| store.write_text(&name, &text).map(|()| name)) {
            Ok(_) => {
                // The original is safe now; the empty book replaces it with this flush.
                self.dirty = true;
                false
            }
            Err(e) => {
                storage::warn(&format!(
                    "stats {}: {e}; it will not be replaced until a backup is written",
                    store.describe(FILE)
                ));
                self.recovery = Some(text);
                true
            }
        }
    }

    /// Applies one tick's events.
    fn apply(&mut self, cur: &mut Current, events: Vec<Event>, tick_seconds: f64, replay: Option<&str>) {
        for event in events {
            self.dirty = true;
            let map = self.book.map_mut(&cur.map, &cur.game, cur.tick_rate);
            match event {
                Event::Ticked => {
                    map.played_ticks = map.played_ticks.saturating_add(1);
                    map.play_seconds += tick_seconds;
                    map.mode_mut(cur.ranked).play_seconds += tick_seconds;
                    cur.session.ticks += 1;
                    cur.session.seconds += tick_seconds;
                }
                Event::Died { segment, n } => {
                    map.add_deaths(segment, n);
                    let mode = map.mode_mut(cur.ranked);
                    mode.deaths = mode.deaths.saturating_add(u64::from(n));
                    cur.session.deaths = cur.session.deaths.saturating_add(u64::from(n));
                    let room = cur
                        .kid_room
                        .and_then(|r| cur.rooms.get(r as usize))
                        .cloned();
                    for _ in 0..n {
                        cur.session.death_log.push(SessionDeath {
                            tick: cur.session.ticks,
                            room: room.clone(),
                        });
                    }
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
                    death_log,
                } => {
                    map.clears = map.clears.saturating_add(1);
                    let mode = map.mode_mut(cur.ranked);
                    mode.finishes = mode.finishes.saturating_add(1);
                    mode.finished_at = Some(now_seconds());
                    cur.session.finished = true;
                    if cur.on_board {
                        let mut log = DeathLog::default();
                        for d in death_log {
                            let spot = d.spot.and_then(|s| {
                                let room = cur.rooms.get(usize::try_from(s.room).ok()?)?;
                                Some((room.as_str(), s.x, s.y))
                            });
                            log.push(d.ticks, spot);
                        }
                        let run = Run {
                            ticks,
                            deaths,
                            splits,
                            segment_deaths,
                            finished_at: now_seconds(),
                            replay: replay.map(str::to_owned),
                            death_log: log,
                        };
                        let board = map.boards.entry(cur.board.clone()).or_default();
                        let rec = board.add(run.clone());
                        eprintln!("{}", clear_line(ticks, deaths, cur.tick_rate, &rec));
                        self.last = Some(rec);
                        self.cleared = Some(Cleared {
                            map: cur.map.clone(),
                            board: cur.board.clone(),
                            run,
                        });
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

    /// Reads one simulated tick into the open run: the tracker's events go into the book. The
    /// post-tick `watch` uses it, and so does `skip` for every tick it runs at once (#1018), so a
    /// skipped tick counts exactly like a watched one. Does nothing before a run's tracker is
    /// open (no run, or the tracker was reset).
    pub(crate) fn observe(
        &mut self,
        sim: &Sim,
        menu: Option<&crate::pause::Menu>,
        tick_seconds: f64,
        replay: Option<&str>,
    ) {
        let Some(mut cur) = self.current.take() else {
            return;
        };
        let now = observation(sim, menu);
        let events = cur.tracker.observe(now);
        self.apply(&mut cur, events, tick_seconds, replay);
        if let Some(kid) = now.kid {
            cur.kid_room = Some(kid.room);
        }
        if now.save != cur.save {
            cur.save = now.save;
            if let Some((room, _, _)) = now.save
                && let Some(name) = cur.rooms.get(room as usize)
            {
                cur.session.saves.push(SaveTouch {
                    tick: cur.session.ticks,
                    room: name.clone(),
                });
            }
        }
        self.enter(&mut cur, now.room);
        self.current = Some(cur);
    }
}

impl Drop for Stats {
    /// Closing the window keeps the last deaths and play time.
    fn drop(&mut self) {
        self.flush();
    }
}

/// Reads the file. A file this build cannot read comes back beside an empty book for
/// [`Stats::preserve`] to back up before the file may be replaced (#1017).
fn load(store: &Store) -> (Book, Option<String>) {
    match store.read_text(FILE) {
        Ok(None) => (Book::default(), None),
        Ok(Some(text)) => match Book::parse(&text) {
            Ok(book) => (book, None),
            Err(e) => {
                storage::warn(&format!(
                    "stats {}: {e}; backing it up before it is replaced, starting empty",
                    store.describe(FILE)
                ));
                (Book::default(), Some(text))
            }
        },
        Err(e) => {
            storage::warn(&format!("stats {e}"));
            (Book::default(), None)
        }
    }
}

/// The first free recovery name: `stats.broken.toml`, then `stats.broken-2.toml`, and so on
/// (#1017). An existing recovery copy is never replaced, and a name that cannot be checked is
/// treated as taken, so an unreadable store blocks the backup rather than clobbering it.
fn backup_name(store: &Store) -> Result<String, String> {
    const NAMES: u32 = 100;
    for i in 1..=NAMES {
        let name = if i == 1 {
            BROKEN.to_owned()
        } else {
            format!("stats.broken-{i}.toml")
        };
        match store.read_text(&name) {
            Ok(None) => return Ok(name),
            Ok(Some(_)) => {}
            Err(e) => return Err(format!("cannot check {}: {e}", store.describe(&name))),
        }
    }
    Err("cannot find a free backup name".to_owned())
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
}

/// The stats' place in the module list (main.rs): their plugin. A `--screenshot` check keeps none.
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = crate::registration::NoArgs;

    fn add(
        app: &mut bevy::prelude::App,
        _args: crate::registration::NoArgs,
        startup: &crate::registration::Startup,
    ) {
        app.add_plugins(StatsPlugin {
            keep: startup.keep(),
        });
    }
}

impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        let store = if self.keep { Store::data() } else { None };
        app.insert_resource(Stats::open(store))
            // The tracker opens on the pre-step state, so the first tick counts (#1011).
            .add_systems(FixedUpdate, seed.before(SimTick))
            .add_systems(FixedUpdate, watch.after(SimTick))
            .add_systems(Update, flush_now_and_then)
            // Leaving the run saves its deaths and play time; the next run starts a new tracker.
            .add_systems(
                OnExit(crate::shell::AppScreen::Run),
                reset.in_set(crate::shell::InRun),
            );
    }
}

/// Before the run's first tick: opens the tracker from the pre-step state, so that tick is
/// counted (#1011). A quick retry clears the tracker; the next seed opens the new run.
fn seed(
    game: NonSend<Game>,
    mut stats: ResMut<Stats>,
    menu: Option<Res<crate::pause::Menu>>,
    mods: Option<Res<crate::mods::RunMods>>,
    practice: Option<Res<crate::practice::Practice>>,
) {
    stats.open_current(
        &game.sim,
        mods.as_deref(),
        menu.as_deref(),
        practice.is_some_and(|p| p.0),
    );
}

/// What the sim looks like now, as the tracker reads it. Read only: the death log never touches
/// the sim's state.
fn observation(sim: &iwbtg_sim::sim::Sim, menu: Option<&crate::pause::Menu>) -> Observation {
    let world = sim.world();
    let kid = world
        .kid(&sim.rules().tick)
        .and_then(|id| world.get(id))
        .map(|k| Spot {
            room: world.room.0,
            x: k.m.x().get().round() as i32,
            y: k.m.y().get().round() as i32,
        });
    Observation {
        tick: world.tick,
        deaths: world.engine.run.deaths,
        stages_done: world.timing.stages_done,
        ended: world.timing.ended,
        ranked_ticks: sim
            .ranked_ticks()
            .saturating_add(menu.map_or(0, |m| m.paused_ticks())),
        kid,
        room: world.room.0,
        save: world.save.as_ref().map(|s| {
            (
                s.room.0,
                s.x.get().round() as i32,
                s.y.get().round() as i32,
            )
        }),
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
    mods: Option<Res<crate::mods::RunMods>>,
    practice: Option<Res<crate::practice::Practice>>,
) {
    // `seed` opened it before this tick; the fallback keeps a direct call (a test, or a tick
    // that ran before the fixed loop seeded) from losing the first observation.
    stats.open_current(
        &game.sim,
        mods.as_deref(),
        menu.as_deref(),
        practice.is_some_and(|p| p.0),
    );
    let replay = replays.as_deref().and_then(|r| r.current_name());
    stats.observe(
        &game.sim,
        menu.as_deref(),
        fixed.timestep().as_secs_f64(),
        replay,
    );
}

/// Writes unsaved deaths and play time every [`FLUSH_SECONDS`].
fn flush_now_and_then(time: Res<Time<Real>>, mut stats: ResMut<Stats>) {
    stats.since_flush += time.delta_secs();
    if stats.since_flush >= FLUSH_SECONDS {
        stats.flush();
    }
}

/// A run's boundary (a retry, or leaving the run for game select): the old run's deaths are
/// saved, and its run-scoped state goes: the tracker and the last clear with its record. Leaving
/// the clear behind is what stops a new run from opening the results page with the old time
/// (#1551). The stats book keeps what was written.
pub(crate) fn reset(world: &mut World) {
    if let Some(mut stats) = world.get_resource_mut::<Stats>() {
        stats.end_session();
        stats.last = None;
        stats.cleared = None;
        stats.flush();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bevy::ecs::system::RunSystemOnce;
    use iwbtg_sim::def::{GameDef, MapDef};
    use iwbtg_sim::input::{Action, InputFrame};
    use iwbtg_sim::sim::{Sim, SimOptions};
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::stripped::RankedStart;
    use iwbtg_sim::timing::{TimingDef, TimingTrigger};

    use super::*;

    fn temp(name: &str) -> Store {
        let d = std::env::temp_dir().join(format!("iwbtg-stats-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        Store::at(d)
    }

    /// One empty fixed step, standing in for `view::tick` (which needs the whole run's
    /// resources), so the schedule's ordering is exercised.
    fn step(mut game: NonSendMut<Game>) {
        game.sim.step(InputFrame::EMPTY);
    }

    /// The clear fixture with a second map `two` whose stage chain is `stages`; map 0 keeps the
    /// package `[timing]`.
    fn two_map_def(base: &GameDef, stages: Vec<TimingTrigger>) -> Arc<GameDef> {
        // A loaded def's chain lives in its map entry; `def.timing` is the map-less fallback.
        let chain = base.map_timing(0).clone();
        let mut def = base.clone();
        def.maps = vec![
            MapDef {
                id: "main".to_owned(),
                timing: chain.clone(),
                start: RankedStart::default(),
            },
            MapDef {
                id: "two".to_owned(),
                timing: TimingDef {
                    start: chain.start,
                    end: chain.end,
                    stages,
                },
                start: RankedStart::default(),
            },
        ];
        Arc::new(def)
    }

    /// A world playing `map` of `def` (the view's run state without a window).
    fn world_on(def: Arc<GameDef>, map: &str) -> World {
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed: 0,
            dll_mode: DllMode::Model,
            mode: PlayMode::Original,
            map: Some(map.to_owned()),
        };
        let prepared = Sim::prepare(&def).unwrap();
        let sim = Sim::new_prepared(def, options, &prepared).unwrap();
        let mut world = World::new();
        world.insert_non_send(Game { sim });
        world
    }

    /// #1010: a run on a map with two stages records both stage splits (the package `[timing]`
    /// has one) and a three-segment run.
    #[test]
    fn the_selected_maps_stages_shape_the_splits() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let base = (*pkg.def).clone();
        let goal = base.map_timing(0).stages[0];
        let def = two_map_def(&base, vec![TimingTrigger::RunStart, goal]);
        let mut world = world_on(Arc::clone(&def), "two");
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(None));
        world.run_system_once(seed).unwrap();
        let (_, run) = play_demo_clear(&mut world, [0, 0]);
        assert_eq!(run.segment_deaths.len(), 3, "two stages and the end");
        assert_eq!(run.splits.len(), 3, "{:?}", run.splits);
        assert!(run.splits[0] <= 1, "the RunStart stage is at the start: {:?}", run.splits);
        assert!(run.splits[1] > run.splits[0], "the goal split is later: {:?}", run.splits);
        assert_eq!(run.splits[2], run.ticks, "the clear ends the run");
        let id = map_key(&base.id, "two");
        let stats = world.resource::<Stats>();
        let cur = stats.current.as_ref().unwrap();
        assert_eq!(
            stats.book.map(&id).unwrap().boards[&cur.board].pb().unwrap().segment_deaths.len(),
            3
        );
    }

    /// #1010: a death after both earlier stages counts in the selected map's last segment (of
    /// three); the package chain's one stage would clamp it into the second.
    #[test]
    fn a_death_counts_in_the_selected_maps_segment() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let base = (*pkg.def).clone();
        let def = two_map_def(&base, vec![TimingTrigger::RunStart, TimingTrigger::RunStart]);
        let mut world = world_on(def, "two");
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(None));
        world.run_system_once(seed).unwrap();
        // The first tick fires both RunStart stages.
        world.non_send_mut::<Game>().sim.step(InputFrame::EMPTY);
        world.run_system_once(watch).unwrap();
        let suicide = InputFrame {
            held: Action::Suicide.bit(),
            pressed: Action::Suicide.bit(),
            released: 0,
        };
        let mut ticks = 0;
        while world.resource::<Stats>().session.deaths == 0 {
            assert!(ticks < 100, "no death in 100 ticks");
            world.non_send_mut::<Game>().sim.step(suicide);
            world.run_system_once(watch).unwrap();
            ticks += 1;
        }
        let id = map_key(&base.id, "two");
        assert_eq!(
            world.resource::<Stats>().session.segment_deaths[&id],
            vec![0, 0, 1],
            "the death is in the last of three segments"
        );
    }

    /// #1011: in the plugin's schedule (`seed` before the tick, `watch` after), the run's
    /// pre-step state is the baseline, so every stepped tick is counted, the first included.
    /// Before the fix `watch` opened the tracker after the first step and counted none of it.
    #[test]
    fn the_plugin_schedule_counts_the_first_tick() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(None));
        let mut schedule = Schedule::default();
        schedule.add_systems((
            seed.before(SimTick),
            step.in_set(SimTick),
            watch.after(SimTick),
        ));
        let id = map_key(&pkg.def.id, "main");
        schedule.run(&mut world);
        assert_eq!(world.non_send::<Game>().sim.world().tick, 1);
        assert_eq!(
            world.resource::<Stats>().book.map(&id).unwrap().played_ticks,
            1,
            "the first tick counts"
        );
        schedule.run(&mut world);
        assert_eq!(
            world.resource::<Stats>().book.map(&id).unwrap().played_ticks,
            2
        );
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
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let store = temp("deaths");
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(Some(store.clone())));
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
        let again = Stats::open(Some(store));
        assert_eq!(again.book.map(&id).unwrap().deaths, 1);
        assert_eq!(again.session, Session::default(), "a session starts at zero");
    }

    /// Plays the `tests/fixtures/clear.pkg` fixture once with scripted input: in each room
    /// `waits[i]` idle ticks, then right held and one jump (over the block in the first room,
    /// over the spike in the goal room). Returns the clear's record and its run.
    fn play_demo_clear(world: &mut World, waits: [u32; 2]) -> (Recorded, Run) {
        play_demo_clear_paused(world, waits, 0)
    }

    /// [`play_demo_clear`] with two pauses of `pause` ticks each (when not 0) in the pause menu:
    /// after tick 5 and after the stage split, as `pause::count` records them.
    fn play_demo_clear_paused(world: &mut World, waits: [u32; 2], pause: u64) -> (Recorded, Run) {
        let (right, jump) = (Action::Right.bit(), Action::Jump.bit());
        let mut rooms = Vec::new();
        let (mut in_room, mut jumped, mut jump_left) = (0u32, false, 0u32);
        let deaths_before = world.resource::<Stats>().session.deaths;
        for t in 0..2000u32 {
            let game = world.non_send::<Game>();
            let w = game.sim.world();
            if w.timing.ended {
                let stats = world.resource::<Stats>();
                let rec = stats.last.clone().expect("a clear is recorded");
                let cur = stats.current.as_ref().unwrap();
                let board = &stats.book.map(&cur.map).unwrap().boards[&cur.board];
                let run = board.runs[rec.place.unwrap() - 1].clone();
                let cleared = stats.cleared.as_ref().expect("the cleared run is kept");
                assert_eq!(
                    (&cleared.map, &cleared.board, &cleared.run),
                    (&cur.map, &cur.board, &run)
                );
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
                deaths_before,
                "the scripted kid died at tick {t}"
            );
        }
        panic!("no clear in 2000 ticks");
    }

    /// A practice run is never put on a board, even from a new game (#39).
    #[test]
    fn a_practice_run_is_never_on_a_board() {
        use iwbtg_sim::room::StartPoint;
        use iwbtg_sim::sim::{Sim, SimOptions};
        use iwbtg_sim::sound::DllMode;
        use iwbtg_sim::stripped::PlayMode;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let sim = Sim::new(
            pkg.def.clone(),
            SimOptions {
                start: StartPoint::NewGame { difficulty: 0 },
                seed: 1,
                dll_mode: DllMode::Model,
                mode: PlayMode::Original,
                map: None,
            },
        )
        .unwrap();
        let mut stats = Stats::open(None);
        stats.open_current(&sim, None, None, true);
        assert_eq!(stats.on_board(), Some(false), "practice is not on a board");

        let mut stats = Stats::open(None);
        stats.open_current(&sim, None, None, false);
        assert_eq!(stats.on_board(), Some(true), "a plain new game is");
    }

    /// #1619, #1623: a Classic run records its mode's time, deaths and finish, and the session
    /// (its rooms, the death's room, the finish) stays readable after the run is left.
    #[test]
    fn a_classic_session_is_kept_per_mode_and_after_it_ends() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = world_on(Arc::clone(&pkg.def), "main");
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(None));
        world.run_system_once(seed).unwrap();
        {
            let open = world.resource::<Stats>().open_session().expect("the run's session");
            assert!(!open.ranked);
            assert_eq!(open.rooms.len(), 1, "the start room: {:?}", open.rooms);
            assert!(open.rooms[0].new, "never entered before");
        }
        let suicide = InputFrame {
            held: Action::Suicide.bit(),
            pressed: Action::Suicide.bit(),
            released: 0,
        };
        for _ in 0..100 {
            if world.resource::<Stats>().session.deaths > 0 {
                break;
            }
            world.non_send_mut::<Game>().sim.step(suicide);
            world.run_system_once(watch).unwrap();
        }
        let start_room = world.resource::<Stats>().open_session().unwrap().rooms[0].room.clone();
        let restart = InputFrame {
            held: Action::Restart.bit(),
            pressed: Action::Restart.bit(),
            released: 0,
        };
        for frame in [restart, InputFrame::default()] {
            world.non_send_mut::<Game>().sim.step(frame);
            world.run_system_once(watch).unwrap();
        }
        play_demo_clear(&mut world, [0, 0]);
        reset(&mut world);
        let stats = world.resource::<Stats>();
        assert!(stats.open_session().is_none());
        let s = stats.ended_session().expect("kept after the run");
        // The suicide, and whatever the restart and the clear's route count.
        let n = stats.session.deaths;
        assert!(n >= 1);
        assert_eq!(s.deaths, n);
        assert_eq!(s.deaths_by_room()[0].0, start_room.as_str(), "the first death's room");
        assert_eq!(
            s.deaths_by_room().iter().map(|(_, d)| u64::from(*d)).sum::<u64>(),
            n
        );
        assert!(s.finished);
        assert!(s.rooms.len() >= 2, "{:?}", s.rooms);
        assert_eq!(s.new_rooms(), s.rooms.iter().map(|r| &r.room).collect::<BTreeSet<_>>().len());
        assert!(s.ticks > 0 && s.seconds > 0.0);
        let classic = stats.book.game_mode(&pkg.def.id, false);
        assert_eq!((classic.deaths, classic.finishes), (n, 1));
        assert!((classic.play_seconds - s.seconds).abs() < 1e-9);
        assert!(classic.furthest.is_some() && classic.finished_at.is_some());
        let last = classic.last_session.expect("the session that ended");
        assert_eq!(last.deaths, n);
        assert_eq!(stats.book.game_mode(&pkg.def.id, true), GameMode::default(), "no Time Attack");
    }

    /// Leaving a finished run forgets its clear and its record: a new run's first observation
    /// has no clear, so nothing can open the results page with the old time (#1551).
    #[test]
    fn a_new_run_after_a_clear_starts_with_no_clear() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(None));
        let (_, first) = play_demo_clear(&mut world, [0, 0]);
        assert!(first.ticks > 0);
        assert!(world.resource::<Stats>().cleared.is_some());

        // Leaving the run (OnExit(Run), the same call a retry makes).
        crate::view::restart_run(&mut world).unwrap();
        reset(&mut world);
        assert!(
            world.resource::<Stats>().cleared.is_none(),
            "the finished run is forgotten"
        );
        assert!(world.resource::<Stats>().last.is_none());

        // The new run's first observation: a fresh tracker, no clear, and the book keeps the
        // finished run's one clear.
        world.run_system_once(watch).unwrap();
        let stats = world.resource::<Stats>();
        assert!(stats.last.is_none() && stats.cleared.is_none(), "no clear on the first tick");
        assert_eq!(
            stats.book.map(&map_key(&pkg.def.id, "main")).unwrap().clears,
            1,
            "only the finished run is in the book"
        );
    }

    /// #679: a run with deaths logs them with their time, room and the kid's position; the clear
    /// writes it to the file and the next start reads it back.
    #[test]
    fn a_cleared_run_keeps_its_deaths_after_a_save_and_load() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let store = temp("death-log");
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(Some(store.clone())));
        world.run_system_once(watch).unwrap();
        let (start_room, kid_x, kid_y) = {
            let game = world.non_send::<Game>();
            let w = game.sim.world();
            let k = w.get(w.kid(&game.sim.rules().tick).unwrap()).unwrap();
            (
                pkg.def.rooms[w.room.index().unwrap()].name.clone(),
                k.m.x().get().round() as i32,
                k.m.y().get().round() as i32,
            )
        };
        let suicide = InputFrame {
            held: Action::Suicide.bit(),
            pressed: Action::Suicide.bit(),
            released: 0,
        };
        let mut ticks = 0;
        while world.resource::<Stats>().session.deaths == 0 {
            assert!(ticks < 100, "no death in 100 ticks");
            world.non_send_mut::<Game>().sim.step(suicide);
            world.run_system_once(watch).unwrap();
            ticks += 1;
        }
        // R brings the kid back at the start.
        let restart = InputFrame {
            held: Action::Restart.bit(),
            pressed: Action::Restart.bit(),
            released: 0,
        };
        for frame in [restart, InputFrame::default()] {
            world.non_send_mut::<Game>().sim.step(frame);
            world.run_system_once(watch).unwrap();
        }
        let (_, run) = play_demo_clear(&mut world, [0, 0]);
        assert_eq!(run.deaths, world.resource::<Stats>().session.deaths as u32);
        // Every counted death is logged (the demo also counts the R after the suicide), each
        // with a spot and in the segment its count went to.
        let marks = run.death_marks();
        assert!(run.deaths >= 1);
        assert_eq!(marks.len(), run.deaths as usize, "{run:?}");
        for (seg, &n) in run.segment_deaths.iter().enumerate() {
            let in_seg = marks.iter().filter(|m| m.segment == seg).count();
            assert_eq!(in_seg, n as usize, "{run:?}");
        }
        assert!(marks.iter().all(|m| m.spot.is_some()), "{run:?}");
        // The suicide: the kid stood still at its start until it died.
        let first = marks[0];
        assert_eq!((first.segment, first.ticks), (0, 0), "{run:?}");
        let (room, x, y) = first.spot.unwrap();
        assert_eq!(room, start_room);
        assert!((x - kid_x).abs() <= 1 && (y - kid_y).abs() <= 1, "{first:?}");
        let board = world.resource::<Stats>().current.as_ref().unwrap().board.clone();
        let again = Stats::open(Some(store));
        let id = map_key(&pkg.def.id, "main");
        let pb = again.book.map(&id).unwrap().boards[&board].pb().unwrap();
        assert_eq!(pb.death_log, run.death_log);
    }

    /// The clear demo (#589): two scripted clears give two clear lines, the first a first
    /// clear, the second with the PB comparison and sum of best. A run has two segments: start
    /// to the goal room (the stage split), then to the clear. The first run idles 10 ticks in
    /// the goal room, the second 20 in the first room: the second is 0.20 s slower but golds
    /// the goal room, so the sum of best drops below the PB.
    #[test]
    fn the_clear_demo_clears_twice_with_a_split() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        assert_eq!(pkg.def.map_timing(0).stages.len(), 1);
        let store = temp("clear");
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(Some(store.clone())));
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
        let again = Stats::open(Some(store));
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
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Time::<Fixed>::from_hz(50.0));
        world.insert_resource(Stats::open(None));
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
        let stats = Stats::open(Some(store.clone()));
        assert!(stats.book.maps.is_empty());
        assert_eq!(
            store.read_text(BROKEN).unwrap().as_deref(),
            Some("version = \"x\"")
        );
    }

    /// #1017: while the backup cannot be written, no flush (a normal write included) replaces
    /// the unreadable file; the backup is retried and only then is the empty book saved.
    #[test]
    fn a_failed_backup_never_replaces_the_unreadable_file() {
        let dir = std::env::temp_dir()
            .join(format!("iwbtg-stats-test-{}-broken-full", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::at(dir.clone());
        store.write_text(FILE, "version = \"x\"").unwrap();
        // A directory in the backup's place makes checking and writing it fail.
        std::fs::create_dir(dir.join(BROKEN)).unwrap();
        let mut stats = Stats::open(Some(store.clone()));
        assert!(stats.book.maps.is_empty());
        stats.dirty = true;
        stats.flush();
        assert_eq!(
            store.read_text(FILE).unwrap().as_deref(),
            Some("version = \"x\""),
            "the original survives a write while its backup is pending"
        );
        assert!(stats.recovery.is_some(), "the backup is still pending");
        // With the name free again the retry preserves the bytes, then saves the empty book.
        std::fs::remove_dir(dir.join(BROKEN)).unwrap();
        stats.flush();
        assert_eq!(
            store.read_text(BROKEN).unwrap().as_deref(),
            Some("version = \"x\"")
        );
        let written = store
            .read_text(FILE)
            .unwrap()
            .expect("the empty book is written");
        assert!(Book::parse(&written).unwrap().maps.is_empty());
    }

    /// #1017: a later unreadable file gets a new backup name; an earlier recovery copy is kept.
    #[test]
    fn an_existing_recovery_copy_is_kept() {
        let store = temp("broken-again");
        store.write_text(BROKEN, "older = \"copy\"").unwrap();
        store.write_text(FILE, "version = \"x\"").unwrap();
        let stats = Stats::open(Some(store.clone()));
        assert!(stats.book.maps.is_empty());
        assert_eq!(
            store.read_text(BROKEN).unwrap().as_deref(),
            Some("older = \"copy\"")
        );
        assert_eq!(
            store.read_text("stats.broken-2.toml").unwrap().as_deref(),
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
            death_log: DeathLog::default(),
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
