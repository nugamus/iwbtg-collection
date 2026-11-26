//! `iwbtg feel`: run and sweep feel contracts (docs/architecture/feel-contracts.md, #1167).
//!
//! A contract is data: a scenario plus assertions over what a player can observe. The loader,
//! validator and evaluator live in the shared test module (`crates/sim/tests/feel/mod.rs`),
//! included here with `#[path]` so the sim integration test and this CLI run the same code (the
//! golden-corpus pattern). That is the design, not a staging step: the TOML parser stays out of
//! `crates/sim/src`, so `toml` remains a sim dev-dependency and there is no sim↔tools dependency
//! knot (#1172, closed as not planned).
//!
//! A contract names its package as a package id or as `fixture:<name>`, one of the public fixture
//! worlds built in code. A package id is looked up in order: the games repo (`--packages`, the
//! hand-worked port, strict load), then the converter output (`--converter-packages`, research
//! `build/packages`, loaded like the trace sweep with unmapped kinds inert). Every result line says
//! which source the contract ran against (#1168). A package in neither source is SKIP, named with
//! the folders searched, and is neither a result nor a regression; a package that is found but
//! does not load, a contract that fails to load and a status worse than the baseline are FAIL.
//! `--bless` rewrites the baseline's statuses only and keeps a skipped contract's old entry.

#[path = "../../sim/tests/feel/mod.rs"]
mod contract;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use iwbtg_format::LoadMode;
use iwbtg_sim::def::GameDef;

use crate::baseline::{Status, Statuses, compare, format_baseline_with, one_line, parse_baseline, word};
use crate::trace_cmd::{ToolError, find_case_by_source, load_package};

/// The feel baseline's header (the file itself is written by `cargo xtask feel --bless`).
const FEEL_HEADER: &str = "# Feel baseline (#1167, #1168): every port contract's status on the package source it ran against.\n\
                           # Written by `cargo xtask feel --bless`; names and statuses only. A skipped contract keeps its last entry.\n";

/// Where a contract's package id is looked up, in this order.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sources<'a> {
    /// The games repo (`IWBTG_GAMES`): hand-worked ports, loaded strictly.
    pub games: Option<&'a Path>,
    /// The converter output (`$IWBTG_RESEARCH/build/packages`), loaded like the trace sweep.
    pub converter: Option<&'a Path>,
}

/// Why a contract did not get a game definition.
enum Unresolved {
    /// The package id is in no source: SKIP, with the folders searched.
    Missing(String),
    /// The package was found but does not load, or the fixture is unknown: FAIL.
    Fail(ToolError),
}

/// Resolves a contract's `package` to a game definition and the source it came from: a public
/// fixture, the games repo, or the converter output (in that order).
fn resolve_package(package: &str, sources: Sources<'_>) -> Result<(GameDef, String), Unresolved> {
    if let Some(result) = contract::fixtures::from_package(package) {
        return result
            .map(|def| (def, "fixture".to_owned()))
            .map_err(|e| Unresolved::Fail(ToolError::Message(e)));
    }
    let order = [
        (sources.games, "games", LoadMode::Strict),
        (
            sources.converter,
            "converter",
            LoadMode::Trace {
                unmapped_kinds_inert: true,
            },
        ),
    ];
    let mut searched = Vec::new();
    for (root, label, mode) in order {
        let Some(root) = root else { continue };
        let dir = root.join(package);
        if !dir.is_dir() {
            searched.push(format!("{label} {}", root.display()));
            continue;
        }
        // A package that is there but broken is FAIL; it never falls through to the next source.
        let files = load_package(&dir).map_err(|e| {
            Unresolved::Fail(ToolError::Message(format!(
                "package {package:?} ({label} {}) does not read: {e}",
                dir.display()
            )))
        })?;
        let def = iwbtg_format::load_game(&files, mode).map_err(|diags| {
            Unresolved::Fail(ToolError::Message(format!(
                "package {package:?} ({label} {}) does not load:\n  {}",
                dir.display(),
                diags
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n  ")
            )))
        })?;
        return Ok((def, format!("{label}: {package}")));
    }
    Err(Unresolved::Missing(if searched.is_empty() {
        format!(
            "package {package:?}: no package source (--packages / --converter-packages; IWBTG_GAMES / IWBTG_RESEARCH)"
        )
    } else {
        format!("package {package:?} is not in {}", searched.join(" or "))
    }))
}

/// One contract run: its verdict and the package source it ran against.
struct Ran {
    id: String,
    verdict: contract::Verdict,
    source: String,
}

/// Loads, validates and runs one contract. `expected_id` is the path below the contracts root;
/// `feel run` passes `None` and accepts the contract's own id. `cases` overrides the trace-case
/// root for the F1 link check; the CLI resolves the checkout's `crates/tools/cases` when needed.
fn run_one(
    path: &Path,
    sources: Sources<'_>,
    expected_id: Option<&str>,
    cases: Option<&Path>,
) -> Result<Ran, Unresolved> {
    let fail = |msg: String| Unresolved::Fail(ToolError::Message(msg));
    let text = std::fs::read_to_string(path).map_err(|e| fail(format!("{}: {e}", path.display())))?;
    let raw = contract::parse(&text).map_err(|e| fail(format!("{}: {e}", path.display())))?;
    let id = expected_id.unwrap_or(&raw.id).to_owned();
    let packages = contract::packages(&raw).map_err(|e| fail(format!("{}: {e}", path.display())))?;
    let mut runs = Vec::new();
    let mut used = Vec::new();
    for package in packages {
        let (def, source) = resolve_package(package, sources).map_err(|e| match e {
            Unresolved::Missing(m) => Unresolved::Missing(m),
            Unresolved::Fail(e) => fail(format!("{id}: {e}")),
        })?;
        let loaded = contract::validate(&raw, &def, &id, package)
            .map_err(|e| fail(format!("{}: {e}", path.display())))?;
        if has_trace(&loaded) {
            let root = match cases {
                Some(root) => root.to_path_buf(),
                None => crate::cases_dir(None).map_err(Unresolved::Fail)?,
            };
            check_trace_links(&loaded, &root).map_err(|e| fail(format!("{id}: {e}")))?;
        }
        runs.push((package, contract::run(&loaded, &def)));
        used.push(source);
    }
    let verdict = contract::combine(&runs);
    Ok(Ran {
        id,
        verdict,
        source: used.join(", "),
    })
}

/// Whether any assertion names a trace case.
fn has_trace(contract: &contract::Contract) -> bool {
    contract
        .asserts
        .iter()
        .flat_map(|a| &a.checks)
        .any(|c| matches!(c, contract::Check::Trace(_)))
}

/// Checks a contract's `trace` links against `cases`: each names a case by its `source`, the case
/// is OK or OPEN with a reason, and every metric the contract asserts is in the linked cases'
/// `required` set (the F1 rule in docs/architecture/feel-contracts.md).
fn check_trace_links(contract: &contract::Contract, cases: &Path) -> Result<(), String> {
    let mut sources = Vec::new();
    let mut required: BTreeSet<String> = BTreeSet::new();
    for assert in &contract.asserts {
        for check in &assert.checks {
            let contract::Check::Trace(source) = check else {
                continue;
            };
            sources.push(source.clone());
            let found = find_case_by_source(cases, source).map_err(|e| format!("trace {source:?}: {e}"))?;
            let Some((path, case)) = found else {
                return Err(format!(
                    "trace {source:?}: no case under {} has that source",
                    cases.display()
                ));
            };
            // An `open` case is OPEN with its reason. Otherwise the fidelity baseline is the last
            // word on the case's status; running the trace needs the private packages.
            if case.open.is_none() {
                let baseline = cases
                    .parent()
                    .map(|p| p.join("fidelity-baseline.txt"))
                    .ok_or("the cases root has no parent for fidelity-baseline.txt")?;
                let text =
                    std::fs::read_to_string(&baseline).map_err(|e| format!("{}: {e}", baseline.display()))?;
                let statuses = parse_baseline(&text).map_err(|e| format!("{}: {e}", baseline.display()))?;
                let package = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .ok_or("the case has no package folder")?;
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .ok_or("the case has no file stem")?;
                let key = format!("{package}/{stem}");
                let status = statuses.get(&key).ok_or_else(|| {
                    format!("trace {source:?}: {key} has no status in {}", baseline.display())
                })?;
                if *status == Status::Fail {
                    return Err(format!(
                        "trace {source:?}: {key} is FAIL in the fidelity baseline, not OK or OPEN"
                    ));
                }
            }
            required.extend(case.required.iter().cloned());
        }
    }
    if sources.is_empty() {
        return Ok(());
    }
    let mut missing: Vec<String> = contract
        .asserts
        .iter()
        .flat_map(|a| &a.checks)
        .filter_map(|c| match c {
            contract::Check::Metric { name, .. } if !required.contains(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    missing.sort();
    missing.dedup();
    if !missing.is_empty() {
        return Err(format!(
            "the linked trace case(s) {} do not have these contract metrics in `required`: {}",
            sources.join(", "),
            missing.join(", ")
        ));
    }
    Ok(())
}

/// `iwbtg feel run <contract>`: prints the verdict and every mismatch; `Ok(false)` on FAIL.
pub fn run_main(path: &Path, sources: Sources<'_>) -> Result<bool, ToolError> {
    // A single named contract whose package is nowhere is an error, not a quiet skip.
    let ran = run_one(path, sources, None, None).map_err(|e| match e {
        Unresolved::Missing(m) => ToolError::Message(m),
        Unresolved::Fail(e) => e,
    })?;
    let (id, verdict) = (ran.id, ran.verdict);
    println!("package source: {}", ran.source);
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

/// How one contract in a sweep ended.
#[derive(Debug)]
enum Outcome {
    /// It ran (or failed to load): its status, a one-line reason when not OK, and the package
    /// source it ran against (empty when it never got that far).
    Ran {
        status: Status,
        reason: String,
        source: String,
    },
    /// Its package is in no source: the folders searched. Not a result and not a regression.
    Skipped(String),
}

/// One contract file to its outcome.
fn sweep_one(root: &Path, path: &Path, sources: Sources<'_>) -> (String, Outcome) {
    let id = contract_id(root, path);
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_one(path, sources, Some(&id), None)
    }));
    let outcome = match ran {
        Ok(Ok(ran)) => {
            let status = match ran.verdict {
                contract::Verdict::Ok => Status::Ok,
                contract::Verdict::Open => Status::Open,
                contract::Verdict::Fail(_) => Status::Fail,
            };
            let reason = match &ran.verdict {
                contract::Verdict::Fail(mismatches) => mismatches
                    .first()
                    .map_or_else(String::new, |m| one_line(&m.to_string(), 100)),
                contract::Verdict::Open => "F1 contract with no evidence".to_owned(),
                contract::Verdict::Ok => String::new(),
            };
            Outcome::Ran {
                status,
                reason,
                source: ran.source,
            }
        }
        Ok(Err(Unresolved::Missing(searched))) => Outcome::Skipped(searched),
        Ok(Err(Unresolved::Fail(e))) => Outcome::Ran {
            status: Status::Fail,
            reason: one_line(&e.to_string(), 100),
            source: String::new(),
        },
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("panic");
            Outcome::Ran {
                status: Status::Fail,
                reason: format!("panic: {}", one_line(msg, 100)),
                source: String::new(),
            }
        }
    };
    (id, outcome)
}

/// One line per contract that ran against a package (with the source it ran against) or did not
/// run; a fixture contract (the public set) is listed only when it is not OK.
fn render(outcomes: &BTreeMap<String, Outcome>) -> String {
    let mut out = String::new();
    for (id, outcome) in outcomes {
        match outcome {
            Outcome::Skipped(searched) => {
                let _ = writeln!(out, "  {id} SKIP: {searched}");
            }
            Outcome::Ran {
                status,
                reason,
                source,
            } => {
                let on_package = !source.is_empty() && source != "fixture";
                if *status == Status::Ok && !on_package {
                    continue;
                }
                let _ = write!(out, "  {id} {}", status.word());
                if on_package {
                    let _ = write!(out, " [{source}]");
                }
                if reason.is_empty() {
                    out.push('\n');
                } else {
                    let _ = writeln!(out, ": {reason}");
                }
            }
        }
    }
    out
}

/// Reads a baseline named by `--baseline`. The path is explicit: a read failure fails the sweep,
/// never a fallback to a run without a baseline.
fn read_baseline(path: &Path) -> Result<Statuses, ToolError> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        let hint = if e.kind() == std::io::ErrorKind::NotFound {
            " (write one with --bless)"
        } else {
            ""
        };
        ToolError::Message(format!("{}: {e}{hint}", path.display()))
    })?;
    parse_baseline(&text).map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))
}

/// `iwbtg feel sweep --contracts <dir> [--packages <dir>] [--converter-packages <dir>]
/// [--baseline <file>] [--bless]`: `Ok(false)` on a regression against the baseline.
pub fn sweep_main(
    contracts: &Path,
    sources: Sources<'_>,
    baseline: Option<&Path>,
    bless: bool,
) -> Result<bool, ToolError> {
    let paths = contract_paths(contracts)?;
    let mut outcomes: BTreeMap<String, Outcome> = BTreeMap::new();
    for path in &paths {
        let (id, outcome) = sweep_one(contracts, path, sources);
        outcomes.insert(id, outcome);
    }
    let now: Statuses = outcomes
        .iter()
        .filter_map(|(id, o)| match o {
            Outcome::Ran { status, .. } => Some((id.clone(), *status)),
            Outcome::Skipped(_) => None,
        })
        .collect();
    let skipped: BTreeSet<String> = outcomes
        .iter()
        .filter(|(_, o)| matches!(o, Outcome::Skipped(_)))
        .map(|(id, _)| id.clone())
        .collect();
    print!("{}", render(&outcomes));
    let n = |s: Status| now.values().filter(|&&v| v == s).count();
    let mut summary = format!(
        "feel: {} contracts: {} OK, {} OPEN, {} FAIL, {} SKIP",
        outcomes.len(),
        n(Status::Ok),
        n(Status::Open),
        n(Status::Fail),
        skipped.len()
    );
    // Without a baseline the sweep gates on the statuses themselves; with one, a status worse
    // than the baseline (or a new failure or a removed contract) is the regression. This mirrors
    // `trace sweep`, so `--bless` can acknowledge a known FAIL without hiding a new one. A skipped
    // contract did not run: it keeps its baseline entry and is neither a change nor a regression.
    let mut ok = if baseline.is_some() {
        true
    } else {
        !now.values().any(|s| *s == Status::Fail)
    };
    if let Some(path) = baseline {
        if bless {
            // Keep the last real result of a contract this run could not reach.
            let mut written = now.clone();
            if !skipped.is_empty() && path.is_file() {
                for (id, status) in read_baseline(path)? {
                    if skipped.contains(&id) {
                        written.insert(id, status);
                    }
                }
            }
            std::fs::write(path, format_baseline_with(FEEL_HEADER, &written))
                .map_err(|e| ToolError::Message(format!("{}: {e}", path.display())))?;
            summary.push_str("; baseline rewritten");
        } else {
            let base: Statuses = read_baseline(path)?
                .into_iter()
                .filter(|(id, _)| !skipped.contains(id))
                .collect();
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

    /// The status and reason of an outcome that ran; panics on a skip.
    fn ran(outcome: &Outcome) -> (Status, String) {
        match outcome {
            Outcome::Ran { status, reason, .. } => (*status, reason.clone()),
            Outcome::Skipped(s) => panic!("skipped: {s}"),
        }
    }

    /// An unknown fixture is FAIL, never a skip: a fixture is code, not a missing port.
    #[test]
    fn an_unknown_fixture_is_fail() {
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
        let (id, outcome) = sweep_one(&root, &path, Sources::default());
        let (status, reason) = ran(&outcome);
        assert_eq!(id, "x");
        assert_eq!(status, Status::Fail, "{reason}");
        assert!(reason.contains("no public fixture"), "{reason}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_public_set_runs_from_the_checkout() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sim/tests/feel");
        let paths = contract_paths(&root).unwrap();
        // Derive the set from the directory: a new contract must not fail this test. #1241 makes
        // the sim test's compiled-in list follow the same rule.
        assert!(!paths.is_empty(), "the public contract set is empty");
        for path in &paths {
            let id = contract_id(&root, path);
            let (got, outcome) = sweep_one(&root, path, Sources::default());
            let (status, reason) = ran(&outcome);
            assert_eq!(got, id);
            assert_eq!(status, Status::Ok, "{id}: {reason}");
        }
    }

    /// CI has no games repo, so it cannot run the port set (#1168). It can still check that every
    /// port contract parses, that its `id` is its path and that it names a package id (not a
    /// fixture), and that the baseline lists exactly the contracts in the folder.
    #[test]
    fn the_port_set_parses_and_matches_its_baseline() {
        let tools = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = tools.join("feel");
        let paths = contract_paths(&root).unwrap();
        assert!(!paths.is_empty(), "the port contract set is empty");
        let mut ids = BTreeSet::new();
        for path in &paths {
            let id = contract_id(&root, path);
            let text = std::fs::read_to_string(path).unwrap();
            let raw = contract::parse(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(
                raw.id, id,
                "a contract's id must equal its path below crates/tools/feel"
            );
            let packages = contract::packages(&raw).unwrap_or_else(|e| panic!("{id}: {e}"));
            for package in packages {
                assert!(
                    contract::fixtures::from_package(package).is_none(),
                    "{id}: a port contract names a games-repo package id, not {package:?}"
                );
            }
            ids.insert(id);
        }
        let text = std::fs::read_to_string(tools.join("feel-baseline.txt")).unwrap();
        let baseline: BTreeSet<String> = parse_baseline(&text).unwrap().into_keys().collect();
        assert_eq!(
            baseline, ids,
            "crates/tools/feel-baseline.txt must list every port contract; run `cargo xtask feel --bless` with IWBTG_GAMES and/or IWBTG_RESEARCH"
        );
    }

    /// A package id is looked up in the games repo, then the converter output (#1168). A package in
    /// neither is SKIP and names the folders searched; a package that is found but does not load
    /// is FAIL and never falls through to the next source. A skip keeps its baseline entry and is
    /// not a regression.
    #[test]
    fn a_missing_port_is_skip_and_a_broken_one_is_fail() {
        let root = scratch("sources");
        let (contracts, games, converter) =
            (root.join("contracts"), root.join("games"), root.join("converter"));
        for dir in [&contracts, &games, &converter] {
            std::fs::create_dir_all(dir.join("physics")).unwrap();
        }
        std::fs::write(
            contracts.join("physics/port.toml"),
            "id = \"physics/port\"
impact = \"F2\"
package = \"some_port\"
start = { room = \"room_0\" }
inputs = \"frame 0: end\"
[[assert]]
tick = 0
kid_count = 1
",
        )
        .unwrap();
        let path = contracts.join("physics/port.toml");
        let both = Sources {
            games: Some(&games),
            converter: Some(&converter),
        };
        // In neither source: SKIP, naming both folders.
        let (_, outcome) = sweep_one(&contracts, &path, both);
        let Outcome::Skipped(searched) = outcome else {
            panic!("{outcome:?}")
        };
        assert!(
            searched.contains("some_port") && searched.contains("games ") && searched.contains("converter "),
            "{searched}"
        );
        // No source at all: still SKIP, and it says how to name one.
        let (_, outcome) = sweep_one(&contracts, &path, Sources::default());
        assert!(
            matches!(&outcome, Outcome::Skipped(s) if s.contains("IWBTG_GAMES")),
            "{outcome:?}"
        );

        // A skip keeps its baseline entry: not a regression, and `--bless` keeps the line.
        let baseline = root.join("feel-baseline.txt");
        std::fs::write(
            &baseline,
            "physics/port OK
",
        )
        .unwrap();
        assert!(sweep_main(&contracts, both, Some(&baseline), false).unwrap());
        assert!(sweep_main(&contracts, both, Some(&baseline), true).unwrap());
        assert!(
            std::fs::read_to_string(&baseline)
                .unwrap()
                .contains("physics/port OK")
        );

        // Found in the converter output but broken: FAIL, labelled with the source.
        std::fs::create_dir_all(converter.join("some_port")).unwrap();
        let (_, outcome) = sweep_one(&contracts, &path, both);
        let (status, reason) = ran(&outcome);
        assert_eq!(status, Status::Fail);
        assert!(reason.contains("converter"), "{reason}");
        // Found in the games repo too: the games copy wins, and a broken one does not fall through.
        std::fs::create_dir_all(games.join("some_port")).unwrap();
        let (_, outcome) = sweep_one(&contracts, &path, both);
        let (status, reason) = ran(&outcome);
        assert_eq!(status, Status::Fail);
        assert!(reason.contains("games"), "{reason}");
        // A FAIL against an OK baseline is a regression.
        assert!(!sweep_main(&contracts, both, Some(&baseline), false).unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A contract's `trace` link must name a case by `source`, the case must be OK or OPEN, and
    /// every metric the contract asserts must be in the linked case's `required` list.
    #[test]
    fn a_trace_link_must_require_every_contract_metric() {
        let root = scratch("trace-link");
        let cases = root.join("cases");
        std::fs::create_dir_all(cases.join("g")).unwrap();
        std::fs::write(
            cases.join("g/case.toml"),
            "source = \"g/t\"\nstart = { room = \"room_0\" }\nfirst_frame = 2\nobject = \"kid\"\nrequired = [\"kid.x\"]\n",
        )
        .unwrap();
        std::fs::write(root.join("fidelity-baseline.txt"), "g/case OK\n").unwrap();
        let def = contract::fixtures::fixture("jump").unwrap();
        let text = r#"
id = "self/trace"
impact = "F1"
package = "fixture:jump"
start = { room = "room_0" }
seed = 0
evidence = ["trace:g/t"]
inputs = "frame 0: end"
[[assert]]
tick = 0
trace = "g/t"
kid.x = { exact = 96.0 }
"#;
        let raw = contract::parse(text).unwrap();
        let contract = contract::validate(&raw, &def, "self/trace", "fixture:jump").unwrap();
        assert_eq!(check_trace_links(&contract, &cases), Ok(()));

        // A metric the trace does not require is a load error, never a silent pass.
        let wrong = text.replace("kid.x = { exact = 96.0 }", "kid.y = { exact = 567.0 }");
        let wrong = contract::parse(&wrong).unwrap();
        let wrong = contract::validate(&wrong, &def, "self/trace", "fixture:jump").unwrap();
        let err = check_trace_links(&wrong, &cases).unwrap_err();
        assert!(err.contains("kid.y"), "{err}");

        // A FAIL in the fidelity baseline is not OK or OPEN.
        std::fs::write(root.join("fidelity-baseline.txt"), "g/case FAIL\n").unwrap();
        let err = check_trace_links(&contract, &cases).unwrap_err();
        assert!(err.contains("FAIL"), "{err}");

        // An `open` case is OPEN with its reason even when the baseline says FAIL.
        std::fs::write(
            cases.join("g/case.toml"),
            "source = \"g/t\"\nstart = { room = \"room_0\" }\nfirst_frame = 2\nobject = \"kid\"\nrequired = [\"kid.x\"]\nopen = \"needs a package\"\n",
        )
        .unwrap();
        assert_eq!(check_trace_links(&contract, &cases), Ok(()));

        // A source no case names is a load error.
        std::fs::write(
            cases.join("g/case.toml"),
            "source = \"g/other\"\nstart = { room = \"room_0\" }\nfirst_frame = 2\nobject = \"kid\"\nrequired = [\"kid.x\"]\n",
        )
        .unwrap();
        let err = check_trace_links(&contract, &cases).unwrap_err();
        assert!(err.contains("no case"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
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
        assert!(sweep_main(&contracts, Sources::default(), Some(&baseline), true).unwrap());
        assert!(
            std::fs::read_to_string(&baseline)
                .unwrap()
                .contains("lifecycle/pass OK")
        );
        // A baseline entry that disappeared is a regression until `--bless`.
        std::fs::write(&baseline, "lifecycle/gone OK\nlifecycle/pass OK\n").unwrap();
        assert!(!sweep_main(&contracts, Sources::default(), Some(&baseline), false).unwrap());
        assert!(sweep_main(&contracts, Sources::default(), Some(&baseline), true).unwrap());
        assert!(
            !std::fs::read_to_string(&baseline)
                .unwrap()
                .contains("lifecycle/gone")
        );
        // A contract that no longer loads is FAIL, and `--bless` acknowledges it.
        std::fs::write(contracts.join("lifecycle/pass.toml"), "not toml").unwrap();
        assert!(!sweep_main(&contracts, Sources::default(), Some(&baseline), false).unwrap());
        assert!(sweep_main(&contracts, Sources::default(), Some(&baseline), true).unwrap());
        let text = std::fs::read_to_string(&baseline).unwrap();
        assert!(text.contains("lifecycle/pass FAIL"), "{text}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
