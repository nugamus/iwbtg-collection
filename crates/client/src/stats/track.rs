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
    /// `Sim::ranked_ticks`.
    pub ranked_ticks: u64,
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
            events.push(Event::Died { segment, n });
        }
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
            });
        }
        events
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
        }
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
}
