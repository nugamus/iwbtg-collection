//! `iwbtg trace sweep`: every trace case on every converter package, as one matrix (#785).
//!
//! CI cannot see the private research repo, so fidelity is checked by this command instead
//! (`cargo xtask fidelity`, docs/architecture/testing.md). It runs `trace check` for each
//! package folder under `<research>/build/packages` in one process, prints one row per game
//! (OK / OPEN / FAIL counts) and one line per case that is not OK, and compares every case
//! status with a committed baseline: names and statuses only, no private data. A case whose
//! status got worse (OK to OPEN or FAIL, OPEN to FAIL) or a new case that fails is a
//! regression; `--bless` rewrites the baseline with the current statuses.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::trace_cmd::{ToolError, check_case, load_package, package_id, read_case};

/// How one case ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// A difference, an error, a panic or a missing package.
    Fail,
    /// The case is marked `open`: it cannot run until its reason is resolved.
    Open,
    /// Bit for bit equal to the original trace.
    Ok,
}

impl Status {
    fn word(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Open => "OPEN",
            Status::Fail => "FAIL",
        }
    }

    fn parse(word: &str) -> Option<Status> {
        match word {
            "OK" => Some(Status::Ok),
            "OPEN" => Some(Status::Open),
            "FAIL" => Some(Status::Fail),
            _ => None,
        }
    }
}

/// One case's result: status and, when not OK, a one-line reason.
#[derive(Debug, Clone)]
pub struct CaseResult {
    pub status: Status,
    pub reason: String,
}

/// One game row: its package id (or folder name), an error that kept the package from
/// loading, and its cases by name.
#[derive(Debug, Default)]
pub struct GameRow {
    pub note: Option<String>,
    pub cases: BTreeMap<String, CaseResult>,
}

/// `<pkg id>/<case name>` to status.
pub type Statuses = BTreeMap<String, Status>;

/// The case files of one package id, sorted.
fn case_paths(cases: &Path, id: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(cases.join(id))
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "toml"))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The first line of a message, cut to `max` characters.
fn one_line(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    if line.chars().count() <= max {
        line.to_owned()
    } else {
        format!("{}...", line.chars().take(max).collect::<String>())
    }
}

fn fail(reason: impl Into<String>) -> CaseResult {
    CaseResult {
        status: Status::Fail,
        reason: reason.into(),
    }
}

/// Runs one case file; a panic in the sim is a FAIL, not the end of the sweep.
fn run_one(files: &iwbtg_format::PackageFiles, path: &Path, research: &Path) -> CaseResult {
    let case = match read_case(path) {
        Ok(c) => c,
        Err(e) => return fail(one_line(&e.to_string(), 100)),
    };
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check_case(files, &case, research)
    }));
    match ran {
        Ok(Ok(d)) if d.is_clean() => CaseResult {
            status: Status::Ok,
            reason: String::new(),
        },
        Ok(Ok(d)) => fail(format!("DIFF {}", one_line(&d.to_string(), 100))),
        Ok(Err(ToolError::Open(reason))) => CaseResult {
            status: Status::Open,
            reason: one_line(&reason, 100),
        },
        Ok(Err(e)) => fail(one_line(&e.to_string(), 100)),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("panic");
            fail(format!("panic: {}", one_line(msg, 100)))
        }
    }
}

/// One package folder: its row key (package id, else folder name) and row.
fn sweep_package(dir: &Path, research: &Path, cases: &Path) -> (String, GameRow) {
    let folder = dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let loaded = load_package(dir).and_then(|files| package_id(&files).map(|id| (files, id)));
    let (files, id) = match loaded {
        Ok(x) => x,
        Err(e) => {
            let reason = format!("package does not load: {}", one_line(&e.to_string(), 80));
            let mut row = GameRow {
                note: Some(reason.clone()),
                ..GameRow::default()
            };
            for path in case_paths(cases, &folder) {
                row.cases.insert(stem(&path), fail(reason.clone()));
            }
            return (folder, row);
        }
    };
    let mut row = GameRow::default();
    for path in case_paths(cases, &id) {
        row.cases.insert(stem(&path), run_one(&files, &path, research));
    }
    (id, row)
}

/// Every package folder under `packages`, in parallel (one thread per package), plus a row for
/// each cases folder whose package is missing.
pub fn sweep(packages: &Path, research: &Path, cases: &Path) -> Result<BTreeMap<String, GameRow>, ToolError> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(packages)
        .map_err(|e| ToolError::Message(format!("{}: {e}", packages.display())))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let mut rows: BTreeMap<String, GameRow> = std::thread::scope(|s| {
        let handles: Vec<_> = dirs
            .iter()
            .map(|d| s.spawn(move || sweep_package(d, research, cases)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("sweep thread"))
            .collect()
    });
    if let Ok(entries) = std::fs::read_dir(cases) {
        for e in entries.flatten() {
            let p = e.path();
            let id = stem(&p);
            if p.is_dir() && !rows.contains_key(&id) {
                let mut row = GameRow {
                    note: Some("no package with this id".into()),
                    ..GameRow::default()
                };
                for path in case_paths(cases, &id) {
                    row.cases.insert(stem(&path), fail("no package with this id"));
                }
                rows.insert(id, row);
            }
        }
    }
    Ok(rows)
}

/// The statuses of a sweep, keyed `<pkg id>/<case>`.
pub fn statuses(rows: &BTreeMap<String, GameRow>) -> Statuses {
    rows.iter()
        .flat_map(|(id, row)| {
            row.cases
                .iter()
                .map(move |(name, r)| (format!("{id}/{name}"), r.status))
        })
        .collect()
}

/// Reads a baseline file: `<pkg id>/<case> <STATUS>` lines; `#` starts a comment.
pub fn parse_baseline(text: &str) -> Result<Statuses, String> {
    let mut out = Statuses::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let (Some(key), Some(word), None) = (words.next(), words.next(), words.next()) else {
            return Err(format!("line {}: expected `<game>/<case> <STATUS>`", n + 1));
        };
        let status = Status::parse(word).ok_or_else(|| format!("line {}: unknown status {word:?}", n + 1))?;
        out.insert(key.to_owned(), status);
    }
    Ok(out)
}

/// Writes a baseline file.
pub fn format_baseline(statuses: &Statuses) -> String {
    let mut out = String::from(
        "# Fidelity baseline (#785): every trace case's status on the converter packages.\n\
         # Written by `cargo xtask fidelity --bless`; names and statuses only.\n",
    );
    for (key, status) in statuses {
        let _ = writeln!(out, "{key} {}", status.word());
    }
    out
}

/// One change against the baseline.
#[derive(Debug, PartialEq, Eq)]
pub struct Change {
    pub key: String,
    pub was: Option<Status>,
    pub now: Option<Status>,
}

impl Change {
    /// Worse than before, or new and failing.
    pub fn is_regression(&self) -> bool {
        match (self.was, self.now) {
            (Some(was), Some(now)) => now < was,
            (None, Some(now)) => now == Status::Fail,
            _ => false,
        }
    }
}

/// Every case whose status differs from the baseline, added or gone.
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

/// The matrix: one row per game, then one line per case that is not OK.
pub fn render(rows: &BTreeMap<String, GameRow>) -> String {
    let width = rows.keys().map(String::len).max().unwrap_or(4).max(4);
    let mut out = format!("{:<width$}  {:>4} {:>4} {:>4}\n", "game", "OK", "OPEN", "FAIL");
    for (id, row) in rows {
        let count = |s: Status| row.cases.values().filter(|r| r.status == s).count();
        if row.cases.is_empty() {
            let note = row.note.as_deref().unwrap_or("no cases");
            let _ = writeln!(out, "{id:<width$}  {:>4} {:>4} {:>4}  {note}", "-", "-", "-");
        } else {
            let _ = write!(
                out,
                "{id:<width$}  {:>4} {:>4} {:>4}",
                count(Status::Ok),
                count(Status::Open),
                count(Status::Fail)
            );
            match &row.note {
                Some(note) => {
                    let _ = writeln!(out, "  {note}");
                }
                None => out.push('\n'),
            }
        }
    }
    let mut details = String::new();
    for (id, row) in rows {
        for (name, r) in &row.cases {
            if r.status != Status::Ok {
                let _ = writeln!(details, "  {id}/{name} {}: {}", r.status.word(), r.reason);
            }
        }
    }
    if !details.is_empty() {
        out.push_str("\nnot OK:\n");
        out.push_str(&details);
    }
    out
}

fn word(s: Option<Status>) -> &'static str {
    s.map_or("-", Status::word)
}

/// `iwbtg trace sweep`: `Ok(false)` on a regression against the baseline.
pub fn main(
    packages: &Path,
    research: &Path,
    cases: &Path,
    baseline: Option<&Path>,
    bless: bool,
) -> Result<bool, ToolError> {
    let rows = sweep(packages, research, cases)?;
    print!("{}", render(&rows));
    let now = statuses(&rows);
    let n = |s: Status| now.values().filter(|&&v| v == s).count();
    let with_cases = rows.values().filter(|r| !r.cases.is_empty()).count();
    let mut summary = format!(
        "fidelity: {} games ({with_cases} with cases), {} cases: {} OK, {} OPEN, {} FAIL",
        rows.len(),
        now.len(),
        n(Status::Ok),
        n(Status::Open),
        n(Status::Fail)
    );
    let mut ok = true;
    if let Some(path) = baseline {
        if bless {
            std::fs::write(path, format_baseline(&now))
                .map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
            summary.push_str("; baseline rewritten");
        } else {
            let Ok(text) = std::fs::read_to_string(path) else {
                println!(
                    "
{summary}; no baseline at {} (write one with --bless)",
                    path.display()
                );
                return Ok(n(Status::Fail) == 0);
            };
            let base =
                parse_baseline(&text).map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
            let changes = compare(&base, &now);
            if !changes.is_empty() {
                println!("\nchanges since the baseline:");
                for c in &changes {
                    let mark = if c.is_regression() { "  REGRESSION" } else { "" };
                    println!("  {} {} -> {}{mark}", c.key, word(c.was), word(c.now));
                }
            }
            let regressions = changes.iter().filter(|c| c.is_regression()).count();
            ok = regressions == 0;
            let _ = write!(
                summary,
                "; {} changed, {regressions} regression(s) against the baseline",
                changes.len()
            );
        }
    }
    println!("\n{summary}");
    Ok(ok)
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
    fn worse_or_new_failing_cases_are_regressions() {
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
        assert_eq!(regressions, ["a/new_fail", "a/ok_to_open", "a/open_to_fail"]);
        assert_eq!(compare(&base, &base), []);
    }

    #[test]
    fn the_matrix_has_one_row_per_game_and_lists_what_is_not_ok() {
        let mut rows = BTreeMap::new();
        let mut a = GameRow::default();
        a.cases.insert(
            "x".into(),
            CaseResult {
                status: Status::Ok,
                reason: String::new(),
            },
        );
        a.cases.insert(
            "y".into(),
            CaseResult {
                status: Status::Open,
                reason: "needs a probe".into(),
            },
        );
        rows.insert("game_a".to_owned(), a);
        rows.insert("game_b".to_owned(), GameRow::default());
        let text = render(&rows);
        assert!(text.contains("game_a     1    1    0"), "{text}");
        assert!(text.contains("game_b     -    -    -  no cases"), "{text}");
        assert!(text.contains("game_a/y OPEN: needs a probe"), "{text}");
        assert!(!text.contains("game_a/x"), "{text}");
    }
}
