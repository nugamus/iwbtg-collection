//! The artboards' sample run (`Results169.dc.html`, its script) for `--results-preview`: the
//! Kaenbyou run of seven segments, a new PB (`pb`) or slower than it (`slower`). It goes through
//! the same path as a real clear: a stats board holding the leaderboard sample's runs
//! (`hud/leaderboard`, `SAMPLE_RUNS`) takes the run, and [`Summary::new`] reads what that changed.

use super::{About, Scenario, Summary};
use crate::card::RunChange;
use crate::hud::leaderboard::SAMPLE_RUNS;
use crate::stats::{Board, Run};

/// Kaenbyou runs at 50 ticks per second.
const RATE: u32 = 50;
/// The segments in seconds: the PB before the sample run, the new PB, the slower run and the best
/// segments before the run.
const OLD_PB: [f64; 7] = [212.40, 268.10, 301.56, 344.20, 415.88, 380.12, 457.86];
const NEW_PB: [f64; 7] = [205.84, 271.32, 288.10, 336.48, 392.14, 371.90, 426.68];
const SLOW: [f64; 7] = [210.06, 266.90, 318.44, 352.12, 430.76, 369.80, 450.32];
const BEST_BEFORE: [f64; 7] = [207.12, 262.40, 295.02, 330.10, 401.66, 368.44, 421.30];
const DEATHS_PB: [u32; 7] = [38, 61, 104, 152, 288, 197, 444];
const DEATHS_SLOW: [u32; 7] = [44, 70, 131, 160, 301, 180, 402];
/// The artboard names the last segment `Boss`.
const NAMES: [&str; 7] = [
    "Stage 1", "Stage 2", "Stage 3", "Stage 4", "Stage 5", "Stage 6", "Boss",
];
/// Midnight UTC starting 4 Nov 2026, and the sample runs' days before it (their dates in
/// `SAMPLE_RUNS`).
const NOV_4: u64 = 1_793_750_400;
const DAYS_BEFORE: [u64; 9] = [1, 6, 7, 14, 15, 21, 26, 33, 35];
/// `Cleared 3 Nov 2026, 21:14`.
const CLEARED_AT: u64 = NOV_4 - 86_400 + 21 * 3600 + 14 * 60;

fn ticks(seconds: f64) -> u64 {
    (seconds * f64::from(RATE)).round() as u64
}

/// The run's time at the end of each segment.
fn splits(segments: &[f64]) -> Vec<u64> {
    segments
        .iter()
        .scan(0, |at, &s| {
            *at += ticks(s);
            Some(*at)
        })
        .collect()
}

/// The board before the sample run: the leaderboard sample's runs (for `pb` without the first,
/// which is this run), the PB with its splits, and the best segments.
fn board(scenario: Scenario) -> Board {
    let pb = scenario == Scenario::Pb;
    let mut board = Board::default();
    for (i, &(seconds, _)) in SAMPLE_RUNS.iter().enumerate() {
        if pb && i == 0 {
            continue;
        }
        let splits = match i {
            0 => splits(&NEW_PB),
            1 => splits(&OLD_PB),
            _ => vec![ticks(seconds)],
        };
        board.runs.push(Run {
            ticks: *splits.last().unwrap_or(&0),
            deaths: 0,
            segment_deaths: Vec::new(),
            splits,
            finished_at: NOV_4 - DAYS_BEFORE[i] * 86_400 + 20 * 3600,
            replay: None,
            death_log: Default::default(),
        });
    }
    board.clears = board.runs.len() as u32;
    board.best_segments = BEST_BEFORE
        .iter()
        .zip(NEW_PB)
        .map(|(&best, new)| if pb { ticks(best) } else { ticks(best.min(new)) })
        .collect();
    board
}

/// The sample's results.
pub fn summary(scenario: Scenario) -> Summary {
    let pb = scenario == Scenario::Pb;
    let (segments, deaths) = if pb {
        (&NEW_PB, DEATHS_PB)
    } else {
        (&SLOW, DEATHS_SLOW)
    };
    let splits = splits(segments);
    let run = Run {
        ticks: *splits.last().unwrap_or(&0),
        deaths: deaths.iter().sum(),
        splits,
        segment_deaths: deaths.to_vec(),
        finished_at: CLEARED_AT,
        replay: Some("replays/kaenbyou/sample.iwbr".to_owned()),
        death_log: Default::default(),
    };
    let mut board = board(scenario);
    let rec = board.add(run.clone());
    let mut card = crate::card::preview::sample();
    if !pb {
        // `ProfileCard.dc.html`, scenario `slower`.
        card.deaths = 49_501;
        card.play_seconds = (212 * 3600 + 54 * 60) as f64;
    }
    card.run = Some(RunChange {
        deaths: u64::from(run.deaths),
        play_seconds: run.ticks as f64 / f64::from(RATE),
        ..card.run.unwrap_or(RunChange {
            deaths: 0,
            play_seconds: 0.0,
            game: "Kaenbyou".to_owned(),
            entries_cleared: 4,
            entries_total: 5,
        })
    });
    let about = About {
        title: "I wanna be the Kaenbyou".to_owned(),
        subtitle: "Very Hard · Any%".to_owned(),
        rate: RATE,
        ranked: true,
    };
    let mut s = Summary::new(about, &run, &rec, &board, card);
    for (stage, name) in s.stages.iter_mut().zip(NAMES) {
        stage.name = name.to_owned();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::leaderboard::date;
    use crate::stats::fmt_ticks;

    #[test]
    fn the_sample_dates_match_the_leaderboard_sample() {
        for (i, (_, label)) in SAMPLE_RUNS.iter().enumerate() {
            assert_eq!(date(NOV_4 - DAYS_BEFORE[i] * 86_400), *label);
        }
    }

    /// The artboard's numbers: the PB 39:40.12 → 38:12.46 (−1:27.66), gold on stages 1, 3 and 5,
    /// 1,284 deaths; slower: 39:58.40, +1:45.94 behind the PB that stands.
    #[test]
    fn the_sample_reads_as_the_artboard() {
        let pb = summary(Scenario::Pb);
        assert_eq!(pb.hero(), ("38:12".to_owned(), ".46".to_owned()));
        assert!(pb.is_pb);
        assert_eq!(pb.changes()[1].value, "39:40.12 → 38:12.46");
        assert_eq!(pb.changes()[1].note, "−1:27.66");
        let golds: Vec<bool> = pb.stages.iter().map(|s| s.gold).collect();
        assert_eq!(golds, [true, false, true, false, true, false, false]);
        assert_eq!(pb.deaths, 1284);
        assert_eq!(pb.stages[6].name, "Boss");
        assert_eq!(fmt_ticks(pb.stages[0].segment, RATE), "3:25.84");
        assert_eq!(pb.place, 1);

        let slow = summary(Scenario::Slower);
        assert!(!slow.is_pb);
        assert_eq!(fmt_ticks(slow.ticks, RATE), "39:58.40");
        assert_eq!(slow.changes()[1].value, "38:12.46 stands");
        assert_eq!(slow.changes()[1].note, "+1:45.94 this run");
        assert_eq!(slow.place, 3, "behind the new PB and the old one");
        assert_eq!(slow.deaths, 1288);
    }
}
