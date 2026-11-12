//! The stats book: everything `stats.toml` keeps, and the rules for adding a run to it
//! (docs/architecture/client/stats.md). Pure data: no Bevy, no storage, no clock.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The layout version this build writes. 2: runs keep their death log (#679).
pub const VERSION: u32 = 2;

/// Runs kept per board: the Local scope shows your top 100 (docs/product.md, Leaderboards).
pub const TOP_RUNS: usize = 100;

/// Runs per board that keep their [`DeathLog`]: the fastest ones. A run that falls below
/// drops its log, so the file stays small; the clear just played keeps its own in memory.
pub const DEATH_LOG_RUNS: usize = 10;

/// Each death of a run, in order (#679): the results' death timeline and per-stage heatmap.
/// Kept as parallel lists so `stats.toml` stays small: entry `i` of every list is death `i`.
/// The first `Run::segment_deaths[0]` deaths are in segment 0, the next in segment 1, and so on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathLog {
    /// The run's time at each death (the last tick the kid was seen alive), in the run's
    /// ticks like [`Run::splits`].
    pub ticks: Vec<u64>,
    /// The rooms in this log, by package room id (`room_1`), each once.
    pub room_names: Vec<String>,
    /// Per death: its room, an index into `room_names`; [`DeathLog::NO_SPOT`] when no kid was
    /// seen before it.
    pub rooms: Vec<u32>,
    /// Per death: the kid's position in whole pixels (0 without a spot).
    pub x: Vec<i32>,
    pub y: Vec<i32>,
}

/// One death of a run, for the results screen ([`Run::death_marks`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathMark<'a> {
    /// The run's time at the death; segment boundaries are [`Run::splits`].
    pub ticks: u64,
    /// The segment it counted in (0: start to the first stage).
    pub segment: usize,
    /// The room (package room id) and the kid's position in it, `None` when unknown.
    pub spot: Option<(&'a str, i32, i32)>,
}

impl DeathLog {
    /// The room index of a death without a known spot.
    pub const NO_SPOT: u32 = u32::MAX;

    /// No deaths logged.
    pub fn is_empty(&self) -> bool {
        self.ticks.is_empty()
    }

    /// Deaths logged.
    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    /// Adds a death at `ticks` in a room at a position, or at no known spot.
    pub fn push(&mut self, ticks: u64, spot: Option<(&str, i32, i32)>) {
        self.ticks.push(ticks);
        let (room, x, y) = match spot {
            Some((name, x, y)) => {
                let i = match self.room_names.iter().position(|r| r == name) {
                    Some(i) => i,
                    None => {
                        self.room_names.push(name.to_owned());
                        self.room_names.len() - 1
                    }
                };
                (u32::try_from(i).unwrap_or(Self::NO_SPOT), x, y)
            }
            None => (Self::NO_SPOT, 0, 0),
        };
        self.rooms.push(room);
        self.x.push(x);
        self.y.push(y);
    }

    /// Death `i`'s spot.
    fn spot(&self, i: usize) -> Option<(&str, i32, i32)> {
        let room = usize::try_from(*self.rooms.get(i)?).ok()?;
        Some((
            self.room_names.get(room)?.as_str(),
            *self.x.get(i)?,
            *self.y.get(i)?,
        ))
    }
}

/// One cleared run, in ranked ticks (`Sim::ranked_ticks`, #34).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// The final ranked time.
    pub ticks: u64,
    /// Deaths in this run.
    pub deaths: u32,
    /// Ranked time at the end of each segment, so the last is `ticks`. Segment `i` runs from
    /// split `i` to split `i + 1` of the game's `[timing]` (start, stages, end): a game with `n`
    /// stages has `n + 1` segments.
    pub splits: Vec<u64>,
    /// Deaths per segment, same order as `splits`.
    pub segment_deaths: Vec<u32>,
    /// When it finished, in seconds since 1970 (UTC). Display only; never ranks anything.
    pub finished_at: u64,
    /// Its replay in the data area (`replays/<game id>/<name>.iwbr`, `replays.rs`), for the
    /// replay viewer and upload. Absent for runs from before replays were kept (#476).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<String>,
    /// Each death, when and where (#679). Empty for runs from before it, and for runs outside
    /// the board's first [`DEATH_LOG_RUNS`].
    #[serde(default, skip_serializing_if = "DeathLog::is_empty")]
    pub death_log: DeathLog,
}

impl Run {
    /// Each logged death in order, with the segment it counted in (from `segment_deaths`; a
    /// death past their sum counts in the last segment). Empty when the run has no log.
    pub fn death_marks(&self) -> Vec<DeathMark<'_>> {
        let last = self.segment_deaths.len().saturating_sub(1);
        let (mut segment, mut ends) = (0, 0u64);
        (0..self.death_log.len())
            .map(|i| {
                while segment < last && i as u64 >= ends + u64::from(self.segment_deaths[segment]) {
                    ends += u64::from(self.segment_deaths[segment]);
                    segment += 1;
                }
                DeathMark {
                    ticks: self.death_log.ticks[i],
                    segment,
                    spot: self.death_log.spot(i),
                }
            })
            .collect()
    }

    /// Each segment's own length.
    pub fn segments(&self) -> Vec<u64> {
        let mut before = 0;
        self.splits
            .iter()
            .map(|&s| {
                let seg = s.saturating_sub(before);
                before = s;
                seg
            })
            .collect()
    }
}

/// One leaderboard of your own: one map, one play mode, one mod combination.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    /// Every clear counted on this board, also those that fell out of `runs`.
    pub clears: u32,
    /// Your best runs, fastest first (ties: the earlier run first), at most [`TOP_RUNS`].
    /// The first is the PB.
    #[serde(default)]
    pub runs: Vec<Run>,
    /// The best time ever of each segment (gold), from any run on this board. Empty before
    /// the first clear; reset when the game's segment count changes.
    #[serde(default)]
    pub best_segments: Vec<u64>,
}

/// What adding a run changed, for the results screen and the terminal line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    /// The PB before this run, `None` for a first clear.
    pub pb_before: Option<u64>,
    /// The PB's splits before this run (the results' "vs PB" column), empty for a first clear.
    pub pb_splits_before: Vec<u64>,
    /// This run is the new PB.
    pub is_pb: bool,
    /// Per segment: this run set a new best (gold). All true on a first clear.
    pub gold: Vec<bool>,
    /// Place among your runs (1 is the PB), `None` when it is outside the top [`TOP_RUNS`].
    pub place: Option<usize>,
    /// Sum of best before and after this run.
    pub sum_of_best_before: Option<u64>,
    pub sum_of_best: Option<u64>,
}

impl Board {
    /// The fastest run.
    pub fn pb(&self) -> Option<&Run> {
        self.runs.first()
    }

    /// The sum of the best segments, `None` before the first clear.
    pub fn sum_of_best(&self) -> Option<u64> {
        (!self.best_segments.is_empty()).then(|| self.best_segments.iter().sum())
    }

    /// Adds a cleared run: the top runs, the PB and the gold segments.
    pub fn add(&mut self, run: Run) -> Recorded {
        let pb_before = self.pb().map(|r| r.ticks);
        let pb_splits_before = self.pb().map(|r| r.splits.clone()).unwrap_or_default();
        let sum_of_best_before = self.sum_of_best();
        let segs = run.segments();
        if self.best_segments.len() != segs.len() {
            self.best_segments = segs.clone();
            self.best_segments.fill(u64::MAX);
        }
        let gold: Vec<bool> = segs
            .iter()
            .zip(&mut self.best_segments)
            .map(|(&s, best)| {
                let new = s < *best;
                if new {
                    *best = s;
                }
                new
            })
            .collect();
        self.clears = self.clears.saturating_add(1);
        // After every run with the same time: ties keep the earlier run ahead.
        let at = self.runs.partition_point(|r| r.ticks <= run.ticks);
        let place = (at < TOP_RUNS).then_some(at + 1);
        if place.is_some() {
            self.runs.insert(at, run);
            self.runs.truncate(TOP_RUNS);
            if let Some(out) = self.runs.get_mut(DEATH_LOG_RUNS) {
                out.death_log = DeathLog::default();
            }
        }
        Recorded {
            pb_before,
            pb_splits_before,
            is_pb: place == Some(1),
            gold,
            place,
            sum_of_best_before,
            sum_of_best: self.sum_of_best(),
        }
    }
}

/// Which board a run counts for: `"<mode>/<mods>"`, mode `ranked` or `original`, mods as
/// `ModSet::codes` (`NM`, `SD+NB`).
pub fn board_key(ranked: bool, mods: &str) -> String {
    format!("{}/{mods}", if ranked { "ranked" } else { "original" })
}

/// Everything about one map.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MapStats {
    /// The game the map belongs to (counts "games cleared").
    pub game: String,
    /// The game's tick rate when last played, to show ticks as time.
    pub tick_rate: u32,
    /// Every clear of the map, also from a save or from a run that is not on a board.
    pub clears: u32,
    /// Every death on this map.
    pub deaths: u64,
    /// Deaths per segment over all runs, same segments as [`Run::splits`]. A death before the
    /// timer starts counts in segment 0.
    #[serde(default)]
    pub segment_deaths: Vec<u64>,
    /// Ticks played on this map, in any room, cleared or not.
    pub played_ticks: u64,
    /// Seconds played (each tick at the rate it ran), for the player card's play time.
    pub play_seconds: f64,
    /// Boards by [`board_key`].
    #[serde(default)]
    pub boards: BTreeMap<String, Board>,
}

/// The key of a map in the book: `<game id>/<map id>` (`Sim::map_id`; `main` for a package
/// without `[[maps]]`).
pub fn map_key(game: &str, map: &str) -> String {
    format!("{game}/{map}")
}

/// Account totals for the player card (#496).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Totals {
    /// Games with at least one cleared map.
    pub games_cleared: usize,
    pub deaths: u64,
    pub play_seconds: f64,
    pub clears: u64,
}

/// The whole file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Book {
    pub version: u32,
    /// By [`map_key`]: map ids are only unique within a game, so the key is `<game id>/<map id>`.
    #[serde(default)]
    pub maps: BTreeMap<String, MapStats>,
}

impl Default for Book {
    fn default() -> Book {
        Book {
            version: VERSION,
            maps: BTreeMap::new(),
        }
    }
}

impl Book {
    /// Reads `stats.toml` text.
    pub fn parse(text: &str) -> Result<Book, String> {
        let book: Book = toml::from_str(text).map_err(|e| e.to_string())?;
        if book.version > VERSION {
            return Err(format!(
                "version {}, this build reads version {VERSION}",
                book.version
            ));
        }
        Ok(book)
    }

    /// The text to store.
    pub fn to_text(&self) -> String {
        toml::to_string(self).expect("stats are plain data")
    }

    /// One map, `None` before it was first played.
    pub fn map(&self, map: &str) -> Option<&MapStats> {
        self.maps.get(map)
    }

    /// One board, `None` before its first clear.
    pub fn board(&self, map: &str, ranked: bool, mods: &str) -> Option<&Board> {
        self.map(map)?.boards.get(&board_key(ranked, mods))
    }

    /// One map, made when first played.
    pub fn map_mut(&mut self, map: &str, game: &str, tick_rate: u32) -> &mut MapStats {
        let m = self.maps.entry(map.to_owned()).or_default();
        m.game = game.to_owned();
        m.tick_rate = tick_rate;
        m
    }

    /// The replays the boards point at: each board's PB, and every other run in the top runs
    /// (a name is in only one of the two, PB first). Web pruning keeps these (`replays.rs`).
    pub fn board_replays(&self) -> (BTreeSet<&str>, BTreeSet<&str>) {
        let mut pbs = BTreeSet::new();
        let mut top = BTreeSet::new();
        for board in self.maps.values().flat_map(|m| m.boards.values()) {
            for (i, run) in board.runs.iter().enumerate() {
                if let Some(r) = run.replay.as_deref() {
                    if i == 0 {
                        pbs.insert(r)
                    } else {
                        top.insert(r)
                    };
                }
            }
        }
        top.retain(|r| !pbs.contains(r));
        (pbs, top)
    }

    /// Totals over every map.
    pub fn totals(&self) -> Totals {
        let mut cleared: Vec<&str> = self
            .maps
            .values()
            .filter(|m| m.clears > 0)
            .map(|m| m.game.as_str())
            .collect();
        cleared.sort_unstable();
        cleared.dedup();
        Totals {
            games_cleared: cleared.len(),
            deaths: self.maps.values().map(|m| m.deaths).sum(),
            play_seconds: self.maps.values().map(|m| m.play_seconds).sum(),
            clears: self.maps.values().map(|m| u64::from(m.clears)).sum(),
        }
    }
}

impl MapStats {
    /// Counts deaths in a segment.
    pub fn add_deaths(&mut self, segment: usize, n: u32) {
        self.deaths = self.deaths.saturating_add(u64::from(n));
        if self.segment_deaths.len() <= segment {
            self.segment_deaths.resize(segment + 1, 0);
        }
        self.segment_deaths[segment] = self.segment_deaths[segment].saturating_add(u64::from(n));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(splits: &[u64], deaths: u32) -> Run {
        Run {
            ticks: *splits.last().unwrap(),
            deaths,
            splits: splits.to_vec(),
            segment_deaths: vec![0; splits.len()],
            finished_at: 0,
            replay: None,
            death_log: DeathLog::default(),
        }
    }

    /// The issue's user check: two runs, PB and sum of best update.
    #[test]
    fn two_runs_update_the_pb_and_the_sum_of_best() {
        let mut b = Board::default();
        let first = b.add(run(&[100, 250, 400], 3));
        assert_eq!(first.pb_before, None);
        assert!(first.is_pb);
        assert_eq!(first.gold, vec![true, true, true]);
        assert_eq!(first.sum_of_best, Some(400));
        // Faster overall, but the middle segment is slower: 90, 170, 120.
        let second = b.add(run(&[90, 260, 380], 1));
        assert_eq!(second.pb_before, Some(400));
        assert_eq!(second.pb_splits_before, vec![100, 250, 400]);
        assert!(second.is_pb);
        assert_eq!(second.place, Some(1));
        assert_eq!(second.gold, vec![true, false, true]);
        assert_eq!(second.sum_of_best_before, Some(400));
        assert_eq!(b.best_segments, vec![90, 150, 120]);
        assert_eq!(second.sum_of_best, Some(360));
        assert_eq!(b.pb().unwrap().ticks, 380);
        assert_eq!(b.clears, 2);
    }

    #[test]
    fn a_slower_run_is_not_a_pb_but_can_hold_a_gold() {
        let mut b = Board::default();
        b.add(run(&[100, 200], 0));
        let r = b.add(run(&[80, 230], 0));
        assert!(!r.is_pb);
        assert_eq!(r.place, Some(2));
        assert_eq!(r.gold, vec![true, false]);
        assert_eq!(b.sum_of_best(), Some(180));
    }

    #[test]
    fn ties_keep_the_earlier_run_ahead_and_the_top_stays_at_100() {
        let mut b = Board::default();
        for t in 0..TOP_RUNS as u64 {
            b.add(run(&[1000 + t], 0));
        }
        let tie = b.add(run(&[1000], 5));
        assert_eq!(tie.place, Some(2), "after the earlier run with the same time");
        assert_eq!(b.runs[0].deaths, 0);
        assert_eq!(b.runs.len(), TOP_RUNS);
        let slow = b.add(run(&[5000], 0));
        assert_eq!(slow.place, None);
        assert_eq!(b.runs.len(), TOP_RUNS);
        assert_eq!(b.clears, TOP_RUNS as u32 + 2);
    }

    #[test]
    fn a_changed_segment_count_restarts_the_golds() {
        let mut b = Board::default();
        b.add(run(&[100, 200], 0));
        let r = b.add(run(&[50, 150, 300], 0));
        assert_eq!(r.gold, vec![true, true, true]);
        assert_eq!(b.sum_of_best(), Some(300));
    }

    #[test]
    fn totals_count_games_not_maps() {
        let mut book = Book::default();
        let a = book.map_mut(&map_key("game-a", "easy"), "game-a", 50);
        a.clears = 1;
        a.add_deaths(0, 4);
        a.play_seconds = 10.0;
        let a2 = book.map_mut(&map_key("game-a", "hard"), "game-a", 50);
        a2.clears = 2;
        a2.add_deaths(2, 1);
        book.map_mut(&map_key("game-b", "main"), "game-b", 50)
            .add_deaths(1, 7);
        let t = book.totals();
        assert_eq!(t.games_cleared, 1);
        assert_eq!(t.deaths, 12);
        assert_eq!(t.clears, 3);
        assert_eq!(
            book.map(&map_key("game-a", "hard")).unwrap().segment_deaths,
            vec![0, 0, 1]
        );
    }

    #[test]
    fn the_file_round_trips_and_a_newer_one_is_refused() {
        let mut book = Book::default();
        let m = book.map_mut(&map_key("demo", "main"), "demo", 50);
        m.add_deaths(1, 2);
        m.boards
            .entry(board_key(false, "NM"))
            .or_default()
            .add(run(&[10, 20], 2));
        let text = book.to_text();
        assert_eq!(Book::parse(&text).unwrap(), book);
        assert!(
            text.contains("[maps.\"demo/main\".boards.\"original/NM\"]"),
            "{text}"
        );
        let newer = text.replace("version = 2", "version = 3");
        assert!(Book::parse(&newer).unwrap_err().contains("version 3"));
        assert!(Book::parse("version = 1\n").unwrap().maps.is_empty());
    }

    /// A run with `segment_deaths[i]` deaths in segment `i`, at ticks 1, 2, ...: segment 0 in
    /// `room_1`, later ones in `room_2`; the second death of each segment has no spot.
    fn logged(splits: &[u64], segment_deaths: &[u32]) -> Run {
        let mut r = run(splits, segment_deaths.iter().sum());
        r.segment_deaths = segment_deaths.to_vec();
        let mut t = 0;
        for (seg, &n) in segment_deaths.iter().enumerate() {
            for k in 0..n {
                t += 1;
                let room = if seg == 0 { "room_1" } else { "room_2" };
                let spot = (k != 1).then_some((room, 10 * t as i32, 300));
                r.death_log.push(t, spot);
            }
        }
        r
    }

    /// #679: a run's deaths survive a save and load, with their segments and spots, and
    /// rooms are written once per run.
    #[test]
    fn the_death_log_round_trips_with_segments_and_spots() {
        let mut book = Book::default();
        let m = book.map_mut(&map_key("demo", "main"), "demo", 50);
        m.boards
            .entry(board_key(true, "NM"))
            .or_default()
            .add(logged(&[100, 200], &[3, 0]));
        let text = book.to_text();
        assert!(text.contains("room_names = [\"room_1\"]"), "{text}");
        let back = Book::parse(&text).unwrap();
        assert_eq!(back, book);
        let run = back.board("demo/main", true, "NM").unwrap().pb().unwrap();
        let marks = run.death_marks();
        assert_eq!(marks.len(), 3);
        assert_eq!(
            marks[0],
            DeathMark {
                ticks: 1,
                segment: 0,
                spot: Some(("room_1", 10, 300))
            }
        );
        assert_eq!(marks[1].spot, None, "a death without a seen kid");
        assert_eq!(marks[2].ticks, 3);

        let two = logged(&[100, 200, 300], &[1, 0, 2]);
        let segs: Vec<usize> = two.death_marks().iter().map(|d| d.segment).collect();
        assert_eq!(segs, vec![0, 2, 2], "an empty segment is skipped");
        assert_eq!(two.death_marks()[1].spot.unwrap().0, "room_2");
    }

    /// #679: a version 1 file (runs without a death log) still loads; its runs have no marks.
    #[test]
    fn a_version_1_file_loads_without_death_logs() {
        let old = r#"version = 1

[maps."demo/main"]
game = "demo"
tick_rate = 50
clears = 1
deaths = 2
segment_deaths = [2]
played_ticks = 300
play_seconds = 6.0

[maps."demo/main".boards."ranked/NM"]
clears = 1
best_segments = [300]

[[maps."demo/main".boards."ranked/NM".runs]]
ticks = 300
deaths = 2
splits = [300]
segment_deaths = [2]
finished_at = 0
"#;
        let book = Book::parse(old).unwrap();
        let run = book.board("demo/main", true, "NM").unwrap().pb().unwrap();
        assert_eq!(run.ticks, 300);
        assert!(run.death_log.is_empty());
        assert!(run.death_marks().is_empty());
    }

    /// Only the fastest [`DEATH_LOG_RUNS`] runs keep their log.
    #[test]
    fn runs_below_the_first_ten_drop_their_death_log() {
        let mut b = Board::default();
        for t in 0..=DEATH_LOG_RUNS as u64 {
            b.add(logged(&[1000 - t], &[1]));
        }
        assert_eq!(b.runs.len(), DEATH_LOG_RUNS + 1);
        assert!(b.runs[..DEATH_LOG_RUNS].iter().all(|r| r.death_log.len() == 1));
        assert!(b.runs[DEATH_LOG_RUNS].death_log.is_empty());
    }
}
