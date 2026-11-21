//! `iwbtg feel`: run and sweep feel contracts (docs/architecture/feel-contracts.md, #1167).
//!
//! A contract is data: a scenario plus assertions over what a player can observe. The loader,
//! validator and evaluator live in the sim test module (`crates/sim/tests/feel/mod.rs`) so the sim
//! integration test and this CLI run the same code (the golden-corpus pattern); `crates/sim/src`
//! is owned by another seat while this lands (#1172).
//!
//! A contract names its package as a package id (a folder under `--packages`, the games repo) or
//! as `fixture:<name>`, one of the public fixture worlds built in code. A missing package, a
//! contract that fails to load and a status worse than the baseline are FAIL, never a skip.
//! `--bless` rewrites the baseline's statuses only.

#[path = "../../sim/tests/feel/mod.rs"]
mod contract;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use iwbtg_format::LoadMode;
use iwbtg_sim::def::GameDef;

use crate::baseline::{Status, Statuses, compare, format_baseline_with, one_line, parse_baseline, word};
use crate::trace_cmd::{ToolError, load_package};

/// The feel baseline's header (the file itself is written by `cargo xtask feel --bless`).
const FEEL_HEADER: &str = "# Feel baseline (#1167): every contract's status on its packages.\n\
                           # Written by `cargo xtask feel --bless`; names and statuses only.\n";

/// Resolves a contract's `package` to a game definition: a public fixture, or a package folder
/// under `packages` (the games repo).
fn resolve_package(package: &str, packages: Option<&Path>) -> Result<GameDef, ToolError> {
    if let Some(result) = contract::fixtures::from_package(package) {
        return result.map_err(ToolError::Message);
    }
    let Some(root) = packages else {
        return Err(ToolError::Message(format!(
            "contract package {package:?} needs --packages <dir> (the games repo, IWBTG_GAMES)"
        )));
    };
    let dir = root.join(package);
    if !dir.is_dir() {
        return Err(ToolError::Message(format!(
            "package {package:?} is not a folder under {}",
            root.display()
        )));
    }
    let files = load_package(&dir)?;
    iwbtg_format::load_game(&files, LoadMode::Strict).map_err(|diags| {
        ToolError::Message(format!(
            "package {package:?} does not load:\n  {}",
            diags
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n  ")
        ))
    })
}

/// Loads, validates and runs one contract. `expected_id` is the path below the contracts root;
/// `feel run` passes `None` and accepts the contract's own id.
fn run_one(
    path: &Path,
    packages: Option<&Path>,
    expected_id: Option<&str>,
) -> Result<(String, contract::Verdict), ToolError> {
    let text =
        std::fs::read_to_string(path).map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
    let raw = contract::parse(&text).map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
    let id = expected_id.unwrap_or(&raw.id).to_owned();
    let def = resolve_package(contract::package(&raw), packages)
        .map_err(|e| ToolError::Message(format!("{id}: {e}")))?;
    let loaded = contract::validate(raw, &def, &id)
        .map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
    Ok((id, contract::run(&loaded, def)))
}

/// `iwbtg feel run <contract>`: prints the verdict and every mismatch; `Ok(false)` on FAIL.
pub fn run_main(path: &Path, packages: Option<&Path>) -> Result<bool, ToolError> {
    let (id, verdict) = run_one(path, packages, None)?;
    match &verdict {
        contract::Verdict::Fail(mismatches) => {
            println!("{id} FAIL");
            for mismatch in mismatches {
                print!("{mismatch}");
            }
            Ok(false)
        }
        contract::Verdict::Open => {
            println!("{id} OPEN: an F1 contract with no evidence is OPEN until evidence lands");
            Ok(true)
        }
        contract::Verdict::Ok => {
            println!("{id} OK");
            Ok(true)
        }
    }
}

/// Every `*.toml` below `root`, sorted. A missing or unreadable root is an error, never an empty
/// sweep.
fn contract_paths(root: &Path) -> Result<Vec<PathBuf>, ToolError> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ToolError> {
        let entries =
            std::fs::read_dir(dir).map_err(|e| ToolError::Message(format!("{}: {e}", dir.display())))?;
        for entry in entries {
            let entry = entry.map_err(|e| ToolError::Message(format!("{}: {e}", dir.display())))?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out)?;
            } else if path.extension().is_some_and(|e| e == "toml") {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, &mut out)?;
    out.sort();
    Ok(out)
}

/// `a/b.toml` below `root` to `a/b`.
fn contract_id(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/")
}

/// One contract file to its status and, when not OK, a one-line reason.
fn sweep_one(root: &Path, path: &Path, packages: Option<&Path>) -> (String, Status, String) {
    let id = contract_id(root, path);
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_one(path, packages, Some(&id))
    }));
    match ran {
        Ok(Ok((_, verdict))) => {
            let status = match verdict {
                contract::Verdict::Ok => Status::Ok,
                contract::Verdict::Open => Status::Open,
                contract::Verdict::Fail(_) => Status::Fail,
            };
            let reason = match &verdict {
                contract::Verdict::Fail(mismatches) => mismatches
                    .first()
                    .map_or_else(String::new, |m| one_line(&m.to_string(), 100)),
                contract::Verdict::Open => "F1 contract with no evidence".to_owned(),
                contract::Verdict::Ok => String::new(),
            };
            (id, status, reason)
        }
        Ok(Err(e)) => (id, Status::Fail, one_line(&e.to_string(), 100)),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("panic");
            (id, Status::Fail, format!("panic: {}", one_line(msg, 100)))
        }
    }
}

/// Prints one line per contract that is not OK, then the summary.
fn render(results: &BTreeMap<String, (Status, String)>) -> String {
    let mut out = String::new();
    for (id, (status, reason)) in results {
        if *status == Status::Ok {
            continue;
        }
        if reason.is_empty() {
            let _ = writeln!(out, "  {id} {}", status.word());
        } else {
            let _ = writeln!(out, "  {id} {}: {reason}", status.word());
        }
    }
    out
}

/// `iwbtg feel sweep --contracts <dir> [--packages <dir>] [--baseline <file>] [--bless]`:
/// `Ok(false)` on a regression against the baseline.
pub fn sweep_main(
    contracts: &Path,
    packages: Option<&Path>,
    baseline: Option<&Path>,
    bless: bool,
) -> Result<bool, ToolError> {
    let paths = contract_paths(contracts)?;
    let mut results: BTreeMap<String, (Status, String)> = BTreeMap::new();
    for path in &paths {
        let (id, status, reason) = sweep_one(contracts, path, packages);
        results.insert(id, (status, reason));
    }
    let now: Statuses = results
        .iter()
        .map(|(id, (status, _))| (id.clone(), *status))
        .collect();
    print!("{}", render(&results));
    let n = |s: Status| now.values().filter(|&&v| v == s).count();
    let mut summary = format!(
        "feel: {} contracts: {} OK, {} OPEN, {} FAIL",
        now.len(),
        n(Status::Ok),
        n(Status::Open),
        n(Status::Fail)
    );
    // Without a baseline the sweep gates on the statuses themselves; with one, a status worse
    // than the baseline (or a new failure or a removed contract) is the regression. This mirrors
    // `trace sweep`, so `--bless` can acknowledge a known FAIL without hiding a new one.
    let mut ok = if baseline.is_some() {
        true
    } else {
        !now.values().any(|s| *s == Status::Fail)
    };
    if let Some(path) = baseline {
        if bless {
            std::fs::write(path, format_baseline_with(FEEL_HEADER, &now))
                .map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
            summary.push_str("; baseline rewritten");
        } else {
            // `--baseline` is explicit: a read failure must fail the sweep, not fall back to a
            // run without a baseline.
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

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("iwbtg-feel-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A contract that names a fixture but no evidence is OPEN; an unknown fixture is FAIL.
    #[test]
    fn a_missing_package_is_fail_not_skip() {
        let root = scratch("missing");
        let path = root.join("x.toml");
        std::fs::write(
            &path,
            r#"
id = "x"
impact = "F2"
package = "fixture:does-not-exist"
start = { room = "room_0" }
inputs = "frame 0: end"
[[assert]]
tick = 0
kid_count = 1
"#,
        )
        .unwrap();
        let (id, status, reason) = sweep_one(&root, &path, None);
        assert_eq!(id, "x");
        assert_eq!(status, Status::Fail, "{reason}");
        assert!(reason.contains("no public fixture"), "{reason}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_public_set_runs_from_the_checkout() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sim/tests/feel");
        let paths = contract_paths(&root).unwrap();
        assert_eq!(paths.len(), 4, "{paths:?}");
        for path in &paths {
            let id = contract_id(&root, path);
            let (got, status, reason) = sweep_one(&root, path, None);
            assert_eq!(got, id);
            assert_eq!(status, Status::Ok, "{id}: {reason}");
        }
    }

    #[test]
    fn a_baseline_regression_is_reported_and_bless_clears_it() {
        let root = scratch("baseline");
        let contracts = root.join("contracts");
        std::fs::create_dir_all(contracts.join("lifecycle")).unwrap();
        std::fs::write(
            contracts.join("lifecycle/pass.toml"),
            r#"
id = "lifecycle/pass"
impact = "F2"
package = "fixture:jump"
start = { room = "room_0" }
inputs = "frame 0: end"
[[assert]]
tick = 0
kid_count = 1
"#,
        )
        .unwrap();
        let baseline = root.join("feel-baseline.txt");
        assert!(sweep_main(&contracts, None, Some(&baseline), true).unwrap());
        assert!(
            std::fs::read_to_string(&baseline)
                .unwrap()
                .contains("lifecycle/pass OK")
        );
        // A baseline entry that disappeared is a regression until `--bless`.
        std::fs::write(&baseline, "lifecycle/gone OK\nlifecycle/pass OK\n").unwrap();
        assert!(!sweep_main(&contracts, None, Some(&baseline), false).unwrap());
        assert!(sweep_main(&contracts, None, Some(&baseline), true).unwrap());
        assert!(
            !std::fs::read_to_string(&baseline)
                .unwrap()
                .contains("lifecycle/gone")
        );
        // A contract that no longer loads is FAIL, and `--bless` acknowledges it.
        std::fs::write(contracts.join("lifecycle/pass.toml"), "not toml").unwrap();
        assert!(!sweep_main(&contracts, None, Some(&baseline), false).unwrap());
        assert!(sweep_main(&contracts, None, Some(&baseline), true).unwrap());
        let text = std::fs::read_to_string(&baseline).unwrap();
        assert!(text.contains("lifecycle/pass FAIL"), "{text}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
