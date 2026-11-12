//! Follows one run tick by tick and says what happened: ticks played, deaths in which segment,
//! the clear with its splits. Pure: the plugin (`stats/mod.rs`) reads the sim into an
//! [`Observation`] after every tick and writes the [`Event`]s into the book.

/// What the tracker reads from the sim after a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    /// `World::tick`.
    pub tick: u64,
    /// The death counter (`global.death`).
    pub deaths: u32,
    /// `TimingState::stages_done`.
    pub stages_done: u16,
    /// `TimingState::ended`.
    pub ended: bool,
    /// The run's time so far: `Sim::ranked_ticks` plus the ticks paused while the timer ran
    /// (`pause::Menu::paused_ticks`), as the HUD shows it.
    pub ranked_ticks: u64,
    /// Where the kid is after the tick, `None` when there is none (dead, or a room without one).
    pub kid: Option<Spot>,
}

/// A place in the game: a room (index into `GameDef::rooms`) and a kid position in whole pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spot {
    pub room: u32,
    pub x: i32,
    pub y: i32,
}

/// One death of the run: when and where (#679).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Death {
    /// The run's time (as [`Observation::ranked_ticks`]) on the last tick the kid was seen
    /// alive before the death counted: the death itself, not the restart that counts it.
    pub ticks: u64,
    /// Where the kid was then; `None` when no kid was seen this run before the death.
    pub spot: Option<Spot>,
}

/// What a tick did, for the stats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A tick ran.
    Ticked,
    /// The death counter went up by `n` in this segment.
    Died { segment: usize, n: u32 },
    /// The timer's `end` fired: the run is cleared.
    Cleared {
        ticks: u64,
        deaths: u32,
        splits: Vec<u64>,
        segment_deaths: Vec<u32>,
        /// Every death in order: the first `segment_deaths[0]` in segment 0, and so on.
        death_log: Vec<Death>,
    },
}

/// One run, from its first tick to its clear (or a retry, which starts a new tracker).
#[derive(Clone, Debug)]
pub struct Tracker {
    /// The game's stages + 1.
    segments: usize,
    last: Option<Observation>,
    splits: Vec<u64>,
    segment_deaths: Vec<u32>,
    death_log: Vec<Death>,
    /// The kid's last sighting alive and the run's time then.
    seen: Option<(u64, Spot)>,
    cleared: bool,
}

impl Tracker {
    /// A tracker for a game with `stages` stage splits.
    pub fn new(stages: usize) -> Tracker {
        Tracker {
            segments: stages + 1,
            last: None,
            splits: Vec::new(),
            segment_deaths: vec![0; stages + 1],
            death_log: Vec::new(),
            seen: None,
            cleared: false,
        }
    }

    /// The segment the run is in: deaths before the timer starts count in the first, and
    /// after the end in the last.
    fn segment(&self, stages_done: u16) -> usize {
        usize::from(stages_done).min(self.segments - 1)
    }

    /// Reads the sim after a tick (or after a frame in which no tick ran: nothing then).
    pub fn observe(&mut self, now: Observation) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(before) = self.last.replace(now) else {
            // The first look has nothing to compare with: a run from a save starts with that
            // save's deaths.
            self.see(&now);
            return events;
        };
        if now.tick == before.tick {
            return events;
        }
        events.push(Event::Ticked);
        if now.deaths > before.deaths {
            let segment = self.segment(before.stages_done);
            let n = now.deaths - before.deaths;
            self.segment_deaths[segment] = self.segment_deaths[segment].saturating_add(n);
            // The sighting before this tick: a kid that died is gone by now, and a restart
            // while alive has already made the new kid at the save.
            let death = match self.seen {
                Some((ticks, spot)) => Death {
                    ticks,
                    spot: Some(spot),
                },
                None => Death {
                    ticks: before.ranked_ticks,
                    spot: None,
                },
            };
            for _ in 0..n {
                self.death_log.push(death);
            }
            events.push(Event::Died { segment, n });
        }
        self.see(&now);
        for _ in before.stages_done..now.stages_done {
            self.splits.push(now.ranked_ticks);
        }
        if now.ended && !before.ended && !self.cleared {
            self.cleared = true;
            // Stages that never fired (a package whose stage triggers were skipped) still give
            // every segment a split, so the board's segment count stays the game's.
            self.splits.resize(self.segments - 1, now.ranked_ticks);
            self.splits.push(now.ranked_ticks);
            events.push(Event::Cleared {
                ticks: now.ranked_ticks,
                deaths: self.segment_deaths.iter().sum(),
                splits: self.splits.clone(),
                segment_deaths: self.segment_deaths.clone(),
                death_log: self.death_log.clone(),
            });
        }
        events
    }

    /// Notes where the kid is, when there is one.
    fn see(&mut self, now: &Observation) {
        if let Some(spot) = now.kid {
            self.seen = Some((now.ranked_ticks, spot));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(tick: u64, deaths: u32, stages_done: u16, ended: bool, ranked: u64) -> Observation {
        Observation {
            tick,
            deaths,
            stages_done,
            ended,
            ranked_ticks: ranked,
            kid: None,
        }
    }

    fn spot(room: u32, x: i32, y: i32) -> Option<Spot> {
        Some(Spot { room, x, y })
    }

    #[test]
    fn deaths_land_in_their_segment_and_the_clear_carries_the_splits() {
        let mut t = Tracker::new(2);
        assert!(t.observe(at(0, 4, 0, false, 0)).is_empty(), "first look");
        assert_eq!(t.observe(at(1, 4, 0, false, 1)), vec![Event::Ticked]);
        assert_eq!(
            t.observe(at(2, 5, 0, false, 2)),
            vec![Event::Ticked, Event::Died { segment: 0, n: 1 }]
        );
        t.observe(at(3, 5, 1, false, 100));
        assert_eq!(
            t.observe(at(4, 7, 1, false, 101))[1],
            Event::Died { segment: 1, n: 2 }
        );
        t.observe(at(5, 7, 2, false, 250));
        let ev = t.observe(at(6, 7, 2, true, 400));
        assert_eq!(
            ev[1],
            Event::Cleared {
                ticks: 400,
                deaths: 3,
                splits: vec![100, 250, 400],
                segment_deaths: vec![1, 2, 0],
                // No kid seen: the time of the tick before the counter went up.
                death_log: [1, 100, 100].map(|ticks| Death { ticks, spot: None }).to_vec(),
            }
        );
        // The world stays ended; no second clear.
        assert_eq!(t.observe(at(7, 7, 2, true, 400)), vec![Event::Ticked]);
    }

    #[test]
    fn a_frame_without_a_tick_changes_nothing() {
        let mut t = Tracker::new(0);
        t.observe(at(10, 0, 0, false, 0));
        assert!(t.observe(at(10, 0, 0, false, 0)).is_empty());
    }

    #[test]
    fn missed_stages_still_give_every_segment_a_split() {
        let mut t = Tracker::new(2);
        t.observe(at(0, 0, 0, false, 0));
        let ev = t.observe(at(1, 0, 0, true, 50));
        assert!(matches!(&ev[1], Event::Cleared { splits, .. } if splits == &vec![50, 50, 50]));
    }

    /// A death is logged where and when the kid was last seen alive, once per counted death;
    /// a restart while alive logs the spot before the restart, not the new kid at the save.
    #[test]
    fn deaths_are_logged_at_the_last_sighting() {
        let mut t = Tracker::new(1);
        let kid = |o: Observation, k| Observation { kid: k, ..o };
        t.observe(kid(at(0, 0, 0, false, 0), spot(1, 10, 20)));
        t.observe(kid(at(1, 0, 0, false, 1), spot(1, 12, 20)));
        // Dies: the kid is gone; the death counts three ticks later (R).
        t.observe(at(2, 0, 0, false, 2));
        t.observe(at(3, 0, 0, false, 3));
        t.observe(kid(at(4, 1, 0, false, 4), spot(0, 50, 60)));
        // Split, then R while alive in room 2: the new kid stands at the save.
        t.observe(kid(at(5, 1, 1, false, 5), spot(2, 100, 200)));
        t.observe(kid(at(6, 3, 1, false, 6), spot(0, 50, 60)));
        let ev = t.observe(kid(at(7, 3, 1, true, 7), spot(0, 50, 60)));
        let Event::Cleared {
            death_log,
            segment_deaths,
            ..
        } = &ev[1]
        else {
            panic!("{ev:?}");
        };
        assert_eq!(segment_deaths, &vec![1, 2]);
        let d = |ticks, spot| Death { ticks, spot };
        assert_eq!(
            death_log,
            &vec![
                d(1, spot(1, 12, 20)),
                d(5, spot(2, 100, 200)),
                d(5, spot(2, 100, 200)),
            ]
        );
    }
}
