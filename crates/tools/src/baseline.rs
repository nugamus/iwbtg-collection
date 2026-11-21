//! Status baselines shared by the trace sweep (`crates/tools/fidelity-baseline.txt`) and the feel
//! sweep (`crates/tools/feel-baseline.txt`): a `<area>/<name> <STATUS>` file, the comparison and
//! the regression rule. Extracted from `sweep_cmd.rs` (#1167).
//!
//! A status got worse (OK to OPEN or FAIL, OPEN to FAIL), a new entry fails, or a baseline entry
//! disappeared is a regression; `--bless` rewrites the statuses and is the only acknowledgement.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// How one contract or trace case ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// A difference, an error, a panic or a missing package.
    Fail,
    /// The case is marked `open`, or an F1 contract has no evidence yet.
    Open,
    /// The expected outcome holds, bit for bit where the case says exact.
    Ok,
}

impl Status {
    /// The baseline word.
    pub fn word(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Open => "OPEN",
            Status::Fail => "FAIL",
        }
    }

    /// The status a baseline word names.
    pub fn parse(word: &str) -> Option<Status> {
        match word {
            "OK" => Some(Status::Ok),
            "OPEN" => Some(Status::Open),
            "FAIL" => Some(Status::Fail),
            _ => None,
        }
    }
}

/// `<area>/<name>` or `<game>/<case>` to status.
pub type Statuses = BTreeMap<String, Status>;

/// Reads a baseline file: `<key> <STATUS>` lines; `#` starts a comment.
pub fn parse_baseline(text: &str) -> Result<Statuses, String> {
    let mut out = Statuses::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let (Some(key), Some(word), None) = (words.next(), words.next(), words.next()) else {
            return Err(format!("line {}: expected `<key> <STATUS>`", n + 1));
        };
        let status = Status::parse(word).ok_or_else(|| format!("line {}: unknown status {word:?}", n + 1))?;
        out.insert(key.to_owned(), status);
    }
    Ok(out)
}

/// The fidelity baseline's header.
pub const FIDELITY_HEADER: &str = "# Fidelity baseline (#785): every trace case's status on the converter packages.\n\
                                    # Written by `cargo xtask fidelity --bless`; names and statuses only.\n";

/// Writes the fidelity baseline file.
pub fn format_baseline(statuses: &Statuses) -> String {
    format_baseline_with(FIDELITY_HEADER, statuses)
}

/// Writes a baseline file under `header`.
pub fn format_baseline_with(header: &str, statuses: &Statuses) -> String {
    let mut out = String::from(header);
    for (key, status) in statuses {
        let _ = writeln!(out, "{key} {}", status.word());
    }
    out
}

/// One change against the baseline.
#[derive(Debug, PartialEq, Eq)]
pub struct Change {
    /// The baseline key.
    pub key: String,
    /// The baseline status, if it had one.
    pub was: Option<Status>,
    /// This run's status, if it has one.
    pub now: Option<Status>,
}

impl Change {
    /// Worse than before, new and failing, or gone from the run.
    ///
    /// A baseline entry that disappeared is a regression too (#1016): it can mean a removed
    /// contract/case file, or a tree that could not be enumerated, and either way the gate must
    /// not keep passing on the entries that are left. Retiring one is deliberate, so it also
    /// requires rewriting the baseline with `--bless`, never a silent drop from the sweep.
    pub fn is_regression(&self) -> bool {
        match (self.was, self.now) {
            (Some(was), Some(now)) => now < was,
            (None, Some(now)) => now == Status::Fail,
            (Some(_), None) => true,
            (None, None) => false,
        }
    }
}

/// Every entry whose status differs from the baseline, added or gone.
pub fn compare(baseline: &Statuses, now: &Statuses) -> Vec<Change> {
    let mut keys: Vec<&String> = baseline.keys().chain(now.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter_map(|k| {
            let (was, now) = (baseline.get(k).copied(), now.get(k).copied());
            (was != now).then(|| Change {
                key: k.clone(),
                was,
                now,
            })
        })
        .collect()
}

/// The status word, or `-` when the baseline or run has none.
pub fn word(status: Option<Status>) -> &'static str {
    status.map_or("-", Status::word)
}

/// The first line of a message, cut to `max` characters.
pub fn one_line(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    if line.chars().count() <= max {
        line.to_owned()
    } else {
        format!("{}...", line.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(pairs: &[(&str, Status)]) -> Statuses {
        pairs.iter().map(|(k, s)| ((*k).to_owned(), *s)).collect()
    }

    #[test]
    fn the_baseline_round_trips() {
        let s = st(&[("a/x", Status::Ok), ("a/y", Status::Open), ("b/z", Status::Fail)]);
        assert_eq!(parse_baseline(&format_baseline(&s)).unwrap(), s);
        assert!(parse_baseline("a/x MAYBE").is_err());
        assert!(parse_baseline("a/x").is_err());
    }

    #[test]
    fn worse_or_new_failing_entries_are_regressions() {
        let base = st(&[
            ("a/ok_to_open", Status::Ok),
            ("a/open_to_fail", Status::Open),
            ("a/fail_to_ok", Status::Fail),
            ("a/gone", Status::Ok),
        ]);
        let now = st(&[
            ("a/ok_to_open", Status::Open),
            ("a/open_to_fail", Status::Fail),
            ("a/fail_to_ok", Status::Ok),
            ("a/new_fail", Status::Fail),
            ("a/new_open", Status::Open),
        ]);
        let regressions: Vec<String> = compare(&base, &now)
            .into_iter()
            .filter(Change::is_regression)
            .map(|c| c.key)
            .collect();
        assert_eq!(
            regressions,
            ["a/gone", "a/new_fail", "a/ok_to_open", "a/open_to_fail"]
        );
        assert_eq!(compare(&base, &base), []);
    }

    #[test]
    fn a_baseline_entry_that_disappeared_is_a_regression() {
        for was in [Status::Ok, Status::Open, Status::Fail] {
            let gone = Change {
                key: "a/x".into(),
                was: Some(was),
                now: None,
            };
            assert!(gone.is_regression(), "{was:?} -> gone");
        }
        assert!(
            !Change {
                key: "a/x".into(),
                was: None,
                now: None
            }
            .is_regression()
        );
    }
}
