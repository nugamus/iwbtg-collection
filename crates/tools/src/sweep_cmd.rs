//! `iwbtg trace sweep`: every trace case on every converter package, as one matrix (#785).
//!
//! CI cannot see the private research repo, so fidelity is checked by this command instead
//! (`cargo xtask fidelity`, docs/architecture/testing.md). It runs `trace check` for each
//! package folder under `<research>/build/packages` in one process, prints one row per game
//! (OK / OPEN / FAIL counts) and one line per case that is not OK, and compares every case
//! status with a committed baseline: names and statuses only, no private data. A case whose
//! status got worse (OK to OPEN or FAIL, OPEN to FAIL), a new case that fails, or a baseline case
//! that disappeared is a regression; `--bless` rewrites the baseline with the current statuses
//! and is the only way to acknowledge a removed case. A missing or unreadable packages/cases
//! tree, and an unreadable baseline, are errors rather than an empty comparison (#1016).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::baseline::{Status, Statuses, compare, format_baseline, one_line, parse_baseline, word};
use crate::trace_cmd::{ToolError, check_case, load_package, package_id, read_case};

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

/// The paths of every entry of `dir`; a read or entry failure names the folder.
fn read_dir_paths(dir: &Path) -> Result<Vec<PathBuf>, ToolError> {
    std::fs::read_dir(dir)
        .map_err(|e| ToolError::Message(format!("{}: {e}", dir.display())))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|e| ToolError::Message(format!("{}: {e}", dir.display())))
        })
        .collect()
}

/// The case files of one package id, sorted.
///
/// A missing `cases/<id>` folder is an empty list: the package has no cases yet. Any other read
/// failure is an error, not an empty list (#1016): a folder that exists but cannot be enumerated
/// would otherwise drop every case it holds and the baseline comparison would never see them.
/// Baseline cases of a genuinely removed folder show up as gone in `compare` and fail the gate
/// until `--bless` acknowledges the removal.
fn case_paths(cases: &Path, id: &str) -> Result<Vec<PathBuf>, ToolError> {
    let dir = cases.join(id);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(ToolError::Message(format!("{}: {e}", dir.display()))),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| ToolError::Message(format!("{}: {e}", dir.display())))?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "toml") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
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
fn sweep_package(dir: &Path, research: &Path, cases: &Path) -> Result<(String, GameRow), ToolError> {
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
            for path in case_paths(cases, &folder)? {
                row.cases.insert(stem(&path), fail(reason.clone()));
            }
            return Ok((folder, row));
        }
    };
    let mut row = GameRow::default();
    for path in case_paths(cases, &id)? {
        row.cases.insert(stem(&path), run_one(&files, &path, research));
    }
    Ok((id, row))
}

/// Every package folder under `packages`, in parallel (one thread per package), plus a row for
/// each cases folder whose package is missing.
///
/// Both trees must be enumerable: a missing or unreadable `cases` root is an error, not an empty
/// sweep (#1016). Otherwise every baseline case would look gone (and, before [`Change`] treated
/// removals as regressions, the gate would pass with no case actually run).
pub fn sweep(packages: &Path, research: &Path, cases: &Path) -> Result<BTreeMap<String, GameRow>, ToolError> {
    let case_dirs: Vec<PathBuf> = read_dir_paths(cases)?;
    let mut dirs: Vec<PathBuf> = read_dir_paths(packages)?
        .into_iter()
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let mut rows: BTreeMap<String, GameRow> = std::thread::scope(|s| {
        let handles: Vec<_> = dirs
            .iter()
            .map(|d| s.spawn(move || sweep_package(d, research, cases)))
            .collect();
        let rows: Result<Vec<_>, ToolError> = handles
            .into_iter()
            .map(|h| h.join().expect("sweep thread"))
            .collect();
        rows.map(|pairs| pairs.into_iter().collect())
    })?;
    for p in case_dirs {
        let id = stem(&p);
        if p.is_dir() && !rows.contains_key(&id) {
            let mut row = GameRow {
                note: Some("no package with this id".into()),
                ..GameRow::default()
            };
            for path in case_paths(cases, &id)? {
                row.cases.insert(stem(&path), fail("no package with this id"));
            }
            rows.insert(id, row);
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
            // `--baseline` is explicit: a read failure must fail the sweep, not fall back to a
            // run without a baseline (#1016).
            let text = std::fs::read_to_string(path).map_err(|e| {
                let hint = if e.kind() == std::io::ErrorKind::NotFound {
                    " (write one with --bless)"
                } else {
                    ""
                };
                ToolError::Message(format!("{}: {e}{hint}", path.display()))
            })?;
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

    /// A scratch tree under the system temp dir, empty and removed first.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("iwbtg-sweep-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_cases_root_is_an_error() {
        let root = scratch("missing-cases");
        let packages = root.join("packages");
        std::fs::create_dir_all(&packages).unwrap();
        let err = sweep(&packages, &root, &root.join("cases")).unwrap_err();
        assert!(err.to_string().contains("cases"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unreadable_cases_root_is_an_error() {
        let root = scratch("cases-root-file");
        let packages = root.join("packages");
        std::fs::create_dir_all(&packages).unwrap();
        let cases = root.join("cases");
        std::fs::write(&cases, "not a folder").unwrap();
        let err = sweep(&packages, &root, &cases).unwrap_err();
        assert!(err.to_string().contains("cases"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unreadable_case_folder_is_an_error() {
        let root = scratch("case-folder-file");
        let packages = root.join("packages");
        std::fs::create_dir_all(packages.join("game_a")).unwrap();
        let cases = root.join("cases");
        std::fs::create_dir_all(&cases).unwrap();
        std::fs::write(cases.join("game_a"), "not a folder").unwrap();
        let err = sweep(&packages, &root, &cases).unwrap_err();
        assert!(err.to_string().contains("game_a"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_case_folder_fails_the_gate_until_blessed() {
        let root = scratch("missing-case-folder");
        let packages = root.join("packages");
        // An empty folder does not load as a package, but it still gets a row keyed `game_a`.
        std::fs::create_dir_all(packages.join("game_a")).unwrap();
        let cases = root.join("cases");
        std::fs::create_dir_all(&cases).unwrap();
        let baseline = root.join("baseline.txt");
        std::fs::write(&baseline, "game_a/case OK\n").unwrap();
        assert!(!main(&packages, &root, &cases, Some(&baseline), false).unwrap());
        // `--bless` is the deliberate acknowledgment: the case is retired, and the next run passes.
        assert!(main(&packages, &root, &cases, Some(&baseline), true).unwrap());
        assert!(
            !std::fs::read_to_string(&baseline)
                .unwrap()
                .contains("game_a/case")
        );
        assert!(main(&packages, &root, &cases, Some(&baseline), false).unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_case_file_fails_the_gate() {
        let root = scratch("missing-case-file");
        let packages = root.join("packages");
        std::fs::create_dir_all(packages.join("game_a")).unwrap();
        let cases = root.join("cases");
        // The folder enumerates, it just no longer holds the baseline's case file.
        std::fs::create_dir_all(cases.join("game_a")).unwrap();
        let baseline = root.join("baseline.txt");
        std::fs::write(&baseline, "game_a/case OK\n").unwrap();
        assert!(!main(&packages, &root, &cases, Some(&baseline), false).unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unreadable_baseline_is_an_error() {
        let root = scratch("baseline-dir");
        let packages = root.join("packages");
        std::fs::create_dir_all(&packages).unwrap();
        let cases = root.join("cases");
        std::fs::create_dir_all(&cases).unwrap();
        let baseline = root.join("baseline");
        std::fs::create_dir_all(&baseline).unwrap();
        let err = main(&packages, &root, &cases, Some(&baseline), false).unwrap_err();
        assert!(err.to_string().contains("baseline"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
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
