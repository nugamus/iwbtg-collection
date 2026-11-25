//! `cargo xtask <command>`: the repetitive steps of a PR as one command each (#529).
//!
//! - `check [--all]`: rejects free-text `TODO(OPEN` markers that name no issue or URL (#1119),
//!   then rustfmt check, the generated-schema staleness and WP-era ratchet steps when their inputs
//!   changed, then clippy and tests of the crates this branch changed
//!   (against `origin/main`, committed or not) plus a compile check of the crates that depend on
//!   them. `--all` runs the whole workspace, like CI. Writes the summary to
//!   `target/xtask/last-check.txt` for `pr-body`. Each step streams actionable lines and keeps a
//!   complete log with its cwd, argv and exit status in `target/xtask/logs/` (#1032).
//! - `plan [--all]`: prints the machine-readable selection plan shared with CI
//!   (`scripts/ci_scope.py`): test crates, consumer compile checks, gates, inputs and reasons,
//!   without building anything (#1028).
//! - `fidelity [--bless]`: the trace sweep over every converter package; records the run in
//!   `target/xtask/last-fidelity.txt` (#935).
//! - `feel [--bless]`: the public feel contracts (`crates/sim/tests/feel`), plus the port set
//!   (`crates/tools/feel`) when `IWBTG_GAMES` (games repo, looked up first) and/or
//!   `IWBTG_RESEARCH` (its `build/packages` converter output) is set (#1168); exit 0 for
//!   OK/OPEN/SKIP, 1 for a failure or regression, 2 for a tool error (#1167).
//! - `repin`: after a deliberate change to hashed sim state, re-pins the state hash vectors
//!   (`tests/hash_vectors.rs`), the `FINAL_HASH` of `tests/determinism.rs` and the golden
//!   corpus, then runs them again. After the `SIM_VERSION` freeze point it also records the pins
//!   under `SIM_VERSION` and fails when one changed without a bump (#783). The files it rewrites are
//!   listed in `crates/xtask/pin-files.txt`, which `.claude/hooks/conflict_guard.py` reads too (#949).
//! - `regen`: regenerates the generated files: format schemas and the script API docs.
//! - `pr-body [--issue N]`: prints a PR body for this branch: Closes, the commits, the docs it
//!   changed, and the User check from the last `check`.
//!
//! - `fidelity [--bless]`: every trace case on every converter package in
//!   `$IWBTG_RESEARCH/build/packages` (`iwbtg trace sweep`), as a game x OK/OPEN/FAIL matrix;
//!   exits 1 when a case got worse than `crates/tools/fidelity-baseline.txt` (#785). `--bless`
//!   rewrites the baseline.
//! - `doctor [--scope native|browser|wasi|all] [--warm]`: preflight for the selected work. It
//!   reports every missing pinned tool/target, system library, runner and fixture together with a
//!   repair command, and `--warm` runs `cargo fetch --locked` at the default fingerprint (#1030).
//!
//! Standard library only; it shells out to cargo and git.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// The hash-pin files `repin` rewrites, one repo-relative path per line (`#` starts a comment).
/// `.claude/hooks/conflict_guard.py` reads the same file to spot pin-only merge conflicts (#949).
const PIN_FILES: &str = include_str!("../pin-files.txt");

fn pin_files() -> impl Iterator<Item = &'static str> {
    PIN_FILES
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// The listed pin file whose path ends with `name`.
fn pin_file(root: &Path, name: &str) -> Result<PathBuf, String> {
    pin_files()
        .find(|p| p.ends_with(name))
        .map(|p| root.join(p))
        .ok_or_else(|| format!("{name} is not listed in crates/xtask/pin-files.txt"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = workspace_root();
    let result = match args.first().map(String::as_str) {
        Some("check") => check(&root, args.iter().any(|a| a == "--all")),
        Some("plan") => explain_plan(&root, args.iter().any(|a| a == "--all")),
        Some("repin") => repin(&root),
        Some("regen") => regen(&root),
        Some("pr-body") => pr_body(&root, flag_value(&args, "--issue")),
        Some("doctor") => {
            let warm = args.iter().any(|a| a == "--warm");
            match flag_value(&args, "--scope") {
                Some(s) => Scope::parse(&s).and_then(|scope| doctor(&root, scope, warm)),
                None => doctor(&root, Scope::All, warm),
            }
        }
        Some("fidelity") => fidelity(&root, args.iter().any(|a| a == "--bless")),
        Some("feel") => feel(&root, args.iter().any(|a| a == "--bless")),
        _ => Err(
            "usage: cargo xtask <check [--all] | plan [--all] | repin | regen | pr-body [--issue N] | doctor [--scope native|browser|wasi|all] [--warm] | fidelity [--bless] | feel [--bless]>"
                .to_string(),
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Runs a command in the workspace root, echoing it; Err when it fails.
fn run(root: &Path, program: &str, args: &[&str], env: &[(&str, &str)]) -> Result<(), String> {
    println!(
        "$ {}{program} {}",
        env.iter().map(|(k, v)| format!("{k}={v} ")).collect::<String>(),
        args.join(" ")
    );
    let status = Command::new(program)
        .args(args)
        .envs(env.iter().copied())
        .current_dir(root)
        .status()
        .map_err(|e| format!("{program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program} {}` failed", args.join(" ")))
    }
}

/// Runs a command and returns its stdout and stderr together, whatever its exit status.
fn output(root: &Path, program: &str, args: &[&str], env: &[(&str, &str)]) -> Result<(bool, String), String> {
    let out = Command::new(program)
        .args(args)
        .envs(env.iter().copied())
        .current_dir(root)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.success(), text))
}

/// Files this branch changed against `origin/main`; with `uncommitted`, also working-tree changes.
fn changed_files(root: &Path, uncommitted: bool) -> Result<Vec<String>, String> {
    let mut files = BTreeSet::new();
    let mut commands = vec![&["diff", "--name-only", "origin/main...HEAD"][..]];
    if uncommitted {
        commands.push(&["diff", "--name-only", "HEAD"]);
        commands.push(&["ls-files", "--others", "--exclude-standard"]);
    }
    for args in commands {
        let (ok, text) = output(root, "git", args, &[])?;
        if !ok {
            return Err(format!("git {}: {text}", args.join(" ")));
        }
        files.extend(text.lines().map(str::to_owned));
    }
    Ok(files.into_iter().collect())
}

/// The shared selection plan from `scripts/ci_scope.py`: changed crates to test, transitive
/// consumers to compile-check locally (CI tests both).
struct Plan {
    full: bool,
    code: bool,
    test: Vec<String>,
    compile: Vec<String>,
    identity: String,
    step_identities: BTreeMap<String, String>,
}

fn python_command(root: &Path) -> Result<&'static str, String> {
    for candidate in ["python3", "python"] {
        if let Ok((true, _)) = output(root, candidate, &["-I", "-c", "import tomllib"], &[]) {
            return Ok(candidate);
        }
    }
    Err("python3 or python (3.11+, for tomllib) not found; the shared plan is scripts/ci_scope.py (docs/dev-setup.md)".into())
}

fn plan_text(root: &Path, files: &[String], full: bool, format: &str) -> Result<String, String> {
    let python = python_command(root)?;
    let mut command = Command::new(python);
    command
        .args(["-I", "scripts/ci_scope.py", "--paths-stdin", "--format", format])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if full {
        command.arg("--full");
    }
    let mut child = command.spawn().map_err(|e| format!("shared plan: {e}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("shared plan: no stdin")?;
        for file in files {
            writeln!(stdin, "{file}").map_err(|e| format!("shared plan: {e}"))?;
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("shared plan: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "shared plan failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn names(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn shared_plan(root: &Path, files: &[String], full: bool) -> Result<Plan, String> {
    let text = plan_text(root, files, full, "lines")?;
    let mut plan = Plan {
        full: false,
        code: false,
        test: Vec::new(),
        compile: Vec::new(),
        identity: String::new(),
        step_identities: BTreeMap::new(),
    };
    let mut saw_code = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "full" => plan.full = value == "true",
            "code" => {
                plan.code = value == "true";
                saw_code = true;
            }
            "test" => plan.test = names(value),
            "compile" => plan.compile = names(value),
            "identity" => plan.identity = value.to_owned(),
            "step-identity" => {
                if let Some((step, identity)) = value.split_once('=') {
                    plan.step_identities.insert(step.to_owned(), identity.to_owned());
                }
            }
            _ => {}
        }
    }
    if !saw_code || plan.identity.is_empty() {
        return Err("shared plan: missing code or identity field".into());
    }
    Ok(plan)
}

/// `plan [--all]`: print the machine-readable selection plan without building anything.
fn explain_plan(root: &Path, full: bool) -> Result<(), String> {
    let files = changed_files(root, true)?;
    print!("{}", plan_text(root, &files, full, "json")?);
    Ok(())
}

/// A successful or failed step. Only an `ok` receipt whose identity, argv and cwd still match is
/// reused (#1026). The checksum detects damaged or hand-edited receipts, not a hostile writer.
struct Receipt {
    identity: String,
    argv: String,
    cwd: String,
    status: String,
    exit: String,
    log: String,
    duration_ms: u128,
}

fn receipt_path(root: &Path, what: &str) -> PathBuf {
    root.join("target/xtask/receipts")
        .join(format!("{}.txt", log_slug(0, what)))
}

fn receipt_checksum<'a>(lines: impl Iterator<Item = &'a str>) -> String {
    let mut hasher = DefaultHasher::new();
    for line in lines {
        line.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn write_receipt(root: &Path, what: &str, receipt: &Receipt) -> Result<(), String> {
    let path = receipt_path(root, what);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let body = format!(
        "version=1\nidentity={}\nargv={}\ncwd={}\nstatus={}\nexit={}\nlog={}\nduration_ms={}\n",
        receipt.identity,
        receipt.argv,
        receipt.cwd,
        receipt.status,
        receipt.exit,
        receipt.log,
        receipt.duration_ms
    );
    let checksum = receipt_checksum(body.lines());
    let temp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&temp, format!("{body}checksum={checksum}\n"))
        .map_err(|e| format!("{}: {e}", temp.display()))?;
    std::fs::rename(&temp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

fn read_receipt(root: &Path, what: &str) -> Option<Receipt> {
    let text = std::fs::read_to_string(receipt_path(root, what)).ok()?;
    let mut lines: Vec<&str> = text.lines().collect();
    let stored = lines.pop()?.strip_prefix("checksum=")?;
    if receipt_checksum(lines.iter().copied()) != stored {
        return None;
    }
    let mut fields = BTreeMap::new();
    for line in lines {
        let (key, value) = line.split_once('=')?;
        fields.insert(key.to_owned(), value.to_owned());
    }
    if fields.get("version").map(String::as_str) != Some("1") {
        return None;
    }
    Some(Receipt {
        identity: fields.get("identity")?.clone(),
        argv: fields.get("argv")?.clone(),
        cwd: fields.get("cwd")?.clone(),
        status: fields.get("status")?.clone(),
        exit: fields.get("exit")?.clone(),
        log: fields.get("log")?.clone(),
        duration_ms: fields.get("duration_ms")?.parse().ok()?,
    })
}

fn receipt_usable(receipt: &Receipt, identity: &str, argv: &str, cwd: &str) -> bool {
    receipt.status == "ok" && receipt.identity == identity && receipt.argv == argv && receipt.cwd == cwd
}

/// The `pr-body` marker when its stored plan identity is missing or no longer current.
fn stale_marker(stored: Option<&str>, current: Option<&str>) -> &'static str {
    match (stored, current) {
        (Some(a), Some(b)) if a == b => "",
        _ => "STALE: inputs changed or this check has no plan identity; rerun `cargo xtask check`.\n",
    }
}

fn plan_identity_from_check(check: &str) -> Option<&str> {
    check.lines().find_map(|line| line.strip_prefix("plan="))
}

/// How many trailing lines a failed step prints; the complete output is in its log.
const LOG_TAIL: usize = 120;
/// Print a progress line every this many output lines, so a silent build still shows life.
const LOG_PROGRESS_EVERY: u64 = 50;

struct StepReport {
    ok: bool,
    log: PathBuf,
    tail: Vec<String>,
    results: Vec<String>,
    exit: String,
}

/// Hide obvious credential assignments and URL userinfo from the console and the retained log.
fn redact(line: &str) -> String {
    let mut out = line.to_owned();
    for key in ["TOKEN=", "PASSWORD=", "SECRET=", "token=", "password=", "secret="] {
        let mut search = 0;
        while let Some(found) = out[search..].find(key) {
            let start = search + found + key.len();
            let end = out[start..]
                .find(char::is_whitespace)
                .map_or(out.len(), |i| start + i);
            if end > start {
                out.replace_range(start..end, "***");
            }
            search = start + 3;
            if search >= out.len() {
                break;
            }
        }
    }
    if let Some(scheme) = out.find("://") {
        let start = scheme + 3;
        if let Some(at) = out[start..].find('@') {
            let at = start + at;
            let slash = out[start..].find('/').map_or(out.len(), |i| start + i);
            if at < slash {
                out.replace_range(start..at, "***");
            }
        }
    }
    out
}

/// Lines worth showing live: compiler/test failures and progress, not hundreds of `test ... ok`.
fn actionable(line: &str) -> bool {
    let l = line.trim_start();
    l.starts_with("error")
        || l.starts_with("warning:")
        || l.starts_with("FAILED")
        || l.starts_with("failures:")
        || l.starts_with("panicked")
        || l.starts_with("test result:")
        || l.starts_with("---- ")
        || l.starts_with("--> ")
        || l.starts_with("note:")
        || (l.starts_with("thread ") && l.contains("panicked"))
}

/// A path-safe log name: `03-compile-check-of-dependents-a-b`.
fn log_slug(index: usize, what: &str) -> String {
    let mut s = format!("{index:02}-");
    for c in what.chars().take(60) {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    while s.ends_with('-') {
        s.pop();
    }
    s
}

fn pump<R: std::io::Read + Send + 'static>(
    reader: R,
    log: Arc<Mutex<std::fs::File>>,
    tail: Arc<Mutex<VecDeque<String>>>,
    results: Arc<Mutex<Vec<String>>>,
    failed: Arc<AtomicBool>,
    what: String,
    log_path: PathBuf,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        let mut count = 0u64;
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => {
                    failed.store(true, Ordering::SeqCst);
                    break;
                }
            }
            // Lossy decoding keeps non-UTF-8 tool output from cutting the log short.
            let line = redact(String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']));
            if let Ok(mut file) = log.lock() {
                let _ = writeln!(file, "{line}");
            }
            if line.trim_start().starts_with("test result:")
                && let Ok(mut results) = results.lock()
            {
                results.push(line.clone());
            }
            if let Ok(mut tail) = tail.lock() {
                if tail.len() == LOG_TAIL {
                    tail.pop_front();
                }
                tail.push_back(line.clone());
            }
            count += 1;
            if actionable(&line) {
                println!("{line}");
            } else if count.is_multiple_of(LOG_PROGRESS_EVERY) {
                println!("  ... {what}: {count} lines (log: {})", log_path.display());
            }
        }
    })
}

/// Run a step with both pipes pumped by reader threads, a complete log and a bounded tail.
fn run_logged(
    root: &Path,
    what: &str,
    index: usize,
    program: &str,
    args: &[&str],
) -> Result<StepReport, String> {
    let dir = root.join("target/xtask/logs");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let log_path = dir.join(format!("{}.log", log_slug(index, what)));
    let mut file = std::fs::File::create(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
    let _ = writeln!(file, "cwd: {}", root.display());
    let _ = writeln!(file, "$ {program} {}", args.join(" "));
    let _ = file.flush();
    let log = Arc::new(Mutex::new(file));
    let tail = Arc::new(Mutex::new(VecDeque::new()));
    let results = Arc::new(Mutex::new(Vec::new()));
    let failed = Arc::new(AtomicBool::new(false));
    let mut child = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let mut handles = Vec::new();
    if let Some(out) = child.stdout.take() {
        handles.push(pump(
            out,
            log.clone(),
            tail.clone(),
            results.clone(),
            failed.clone(),
            what.to_owned(),
            log_path.clone(),
        ));
    }
    if let Some(err) = child.stderr.take() {
        handles.push(pump(
            err,
            log.clone(),
            tail.clone(),
            results.clone(),
            failed.clone(),
            what.to_owned(),
            log_path.clone(),
        ));
    }
    let status = child.wait().map_err(|e| format!("{program}: {e}"))?;
    let mut joined = true;
    for handle in handles {
        if handle.join().is_err() {
            joined = false;
        }
    }
    let reader_failed = failed.load(Ordering::SeqCst);
    let exit = status.code().map_or_else(
        || "terminated by signal".to_owned(),
        |c| format!("exit status: {c}"),
    );
    if let Ok(mut file) = log.lock() {
        let _ = writeln!(file, "{exit}");
        let _ = file.flush();
    }
    let tail = tail
        .lock()
        .map(|t| t.iter().cloned().collect())
        .unwrap_or_default();
    let results = results.lock().map(|r| r.clone()).unwrap_or_default();
    Ok(StepReport {
        ok: status.success() && joined && !reader_failed,
        log: log_path,
        tail,
        results,
        exit,
    })
}

/// Whether a line is a free-text `TODO(OPEN` marker: no issue (`#123`) and no URL on the marker line.
fn open_todo_offends(line: &str) -> bool {
    let line = line.trim();
    if !line.contains("TODO(OPEN") {
        return false;
    }
    let bytes = line.as_bytes();
    let issue = (0..bytes.len().saturating_sub(1)).any(|i| bytes[i] == b'#' && bytes[i + 1].is_ascii_digit());
    !issue && !line.contains("http://") && !line.contains("https://")
}

/// `crates/*/src/**/*.rs`: every `TODO(OPEN` names an issue or a URL (#1119). Cheap and offline.
/// The xtask crate is skipped: it holds the pattern and this function's tests.
fn check_open_todos(root: &Path) -> Result<(), String> {
    let crates = root.join("crates");
    let mut bad = Vec::new();
    let entries = std::fs::read_dir(&crates).map_err(|e| format!("{}: {e}", crates.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() && entry.file_name() != "xtask" {
            scan_open_todos(&entry.path().join("src"), &mut bad)?;
        }
    }
    if bad.is_empty() {
        return Ok(());
    }
    for (path, line, text) in &bad {
        println!("{}:{line}: {text}", path.display());
    }
    Err(format!(
        "{} free-text `TODO(OPEN` marker(s): name an issue (`TODO(#123)`) or a URL, or delete the marker (#1119)",
        bad.len()
    ))
}

fn scan_open_todos(dir: &Path, bad: &mut Vec<(PathBuf, usize, String)>) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(()); // a crate without src/ has nothing to scan
    };
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            scan_open_todos(&path, bad)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            for (n, line) in text.lines().enumerate() {
                if open_todo_offends(line) {
                    bad.push((path.clone(), n + 1, line.trim().to_owned()));
                }
            }
        }
    }
    Ok(())
}

/// Packages whose sources or doc comments feed a generated schema: the sim's `schema` feature
/// renders `PhysicsProfileDraft` and its drafts from their doc comments, and the format crate owns
/// the generator (#1064).
const SCHEMA_INPUT_PACKAGES: [&str; 2] = ["iwbtg-sim", "iwbtg-format"];

/// Whether the plan touches an input to a generated schema, as a changed crate or as a consumer of
/// one. A doc-comment-only sim change puts the sim in `test` and the format crate in `compile`, so
/// both sets are consulted.
fn schema_step_needed(plan: &Plan) -> bool {
    plan.test
        .iter()
        .chain(plan.compile.iter())
        .any(|p| SCHEMA_INPUT_PACKAGES.contains(&p.as_str()))
}

/// The receipt identity for a derived step over `packages`' inputs. `scripts/ci_scope.py` owns
/// step identities and does not record these steps, so hash the recorded steps that mention one of
/// the packages; fall back to the whole-plan identity when none is recorded.
fn package_step_identity(plan: &Plan, packages: &[&str]) -> String {
    let parts: Vec<String> = plan
        .step_identities
        .iter()
        .filter(|(key, _)| packages.iter().any(|p| key.contains(p)))
        .map(|(key, identity)| format!("{key}={identity}"))
        .collect();
    if parts.is_empty() {
        plan.identity.clone()
    } else {
        parts.join("\n")
    }
}

/// The receipt identity for the schema step.
fn schema_step_identity(plan: &Plan) -> String {
    package_step_identity(plan, &SCHEMA_INPUT_PACKAGES)
}

/// Packages whose changes can add a WP-era id in an area the ratchet counts. A format or sim
/// change never selects the xtask tests, so without this step a new id reaches main and fails only
/// on its full run (#1519, #1522).
const WP_RATCHET_PACKAGES: [&str; 2] = ["iwbtg-format", "iwbtg-sim"];

/// Whether the plan can add a WP-era id the ratchet must see locally.
fn wp_ratchet_needed(plan: &Plan) -> bool {
    plan.test
        .iter()
        .chain(plan.compile.iter())
        .any(|p| WP_RATCHET_PACKAGES.contains(&p.as_str()))
}

/// The receipt identity for the WP-ratchet step.
fn wp_ratchet_identity(plan: &Plan) -> String {
    package_step_identity(plan, &WP_RATCHET_PACKAGES)
}

fn check(root: &Path, all: bool) -> Result<(), String> {
    let files = changed_files(root, true)?;
    let plan = shared_plan(root, &files, all)?;
    if plan.full {
        println!("selection: full workspace (shared, config or unknown input)");
    }
    let mut summary = String::new();
    let mut index = 0usize;
    // Runs before the cargo steps: it is cheap and offline, and it must also run on a docs-only branch.
    let open_todos = check_open_todos(root);
    match &open_todos {
        Ok(()) => {
            println!("TODO(OPEN markers: every one names an issue or URL");
            let _ = writeln!(summary, "TODO(OPEN markers -> ok");
        }
        Err(e) => {
            let _ = writeln!(summary, "TODO(OPEN markers -> {e}");
        }
    }
    let mut step =
        |root: &Path, what: &str, kind: &str, packages: &[String], args: &[&str], provided: Option<&str>| {
            let key = format!("{kind}:{}", packages.join(","));
            let identity = match provided {
                Some(identity) => identity.to_owned(),
                None => plan
                    .step_identities
                    .get(&key)
                    .ok_or_else(|| format!("shared plan: missing identity for step `{key}`"))?
                    .clone(),
            };
            let argv = format!("cargo {}", args.join(" "));
            let cwd = root.display().to_string();
            if let Some(receipt) = read_receipt(root, what) {
                if receipt_usable(&receipt, &identity, &argv, &cwd) {
                    println!("{what}: reused (inputs unchanged; log: {})", receipt.log);
                    let _ = writeln!(
                        summary,
                        "cargo {} -> reused (inputs unchanged) (log: {})",
                        args.join(" "),
                        receipt.log
                    );
                    return Ok(());
                }
                println!("{what}: invalidated (inputs or command changed)");
            }
            index += 1;
            let started = Instant::now();
            let report = run_logged(root, what, index, "cargo", args)?;
            write_receipt(
                root,
                what,
                &Receipt {
                    identity: identity.clone(),
                    argv,
                    cwd,
                    status: if report.ok { "ok" } else { "failed" }.to_owned(),
                    exit: report.exit.clone(),
                    log: report.log.display().to_string(),
                    duration_ms: started.elapsed().as_millis(),
                },
            )?;
            let results: Vec<&str> = report.results.iter().map(String::as_str).collect();
            let (passed, failed) = results.iter().fold((0u64, 0u64), |(p, f), l| {
                let n = |word: &str| {
                    l.split(';')
                        .find(|s| s.contains(word))
                        .and_then(|s| s.split_whitespace().find_map(|w| w.parse::<u64>().ok()))
                        .unwrap_or(0)
                };
                (p + n("passed"), f + n("failed"))
            });
            let line = if results.is_empty() {
                format!("{what}: {}", if report.ok { "ok" } else { "FAILED" })
            } else {
                format!(
                    "{what}: {} test binaries, {passed} passed, {failed} failed",
                    results.len()
                )
            };
            println!("{line}");
            let _ = writeln!(
                summary,
                "cargo {} -> {line} (log: {})",
                args.join(" "),
                report.log.display()
            );
            if report.ok {
                Ok(())
            } else {
                // The complete output is on disk; show the unfiltered tail so a nested cause is visible
                // without rerunning the command (#1032).
                Err(format!(
                    "{what} failed ({}); full log: {}\n--- last {} lines ---\n{}",
                    report.exit,
                    report.log.display(),
                    report.tail.len(),
                    report.tail.join("\n")
                ))
            }
        };
    let result = (|| {
        open_todos?;
        step(
            root,
            "rustfmt",
            "rustfmt",
            &[],
            &["fmt", "--all", "--", "--check"],
            None,
        )?;
        if !plan.code {
            println!("no crate changed (docs or harness only): nothing to build");
            return Ok(());
        }
        // Cheap and deterministic, and it runs before the long crate suites so a stale generated
        // schema fails fast. A doc-comment-only change to the sim can stale the physics schema
        // because `schemars` reads the doc comments (#1060, #1207); the shared plan then selects
        // the sim tests and a format compile check, neither of which regenerates the schema (#1064).
        if schema_step_needed(&plan) {
            let identity = schema_step_identity(&plan);
            step(
                root,
                "schema staleness (iwbtg-format)",
                "schema",
                &[],
                &[
                    "test",
                    "-p",
                    "iwbtg-format",
                    "--test",
                    "schemas",
                    "schema_files_are_not_stale",
                ],
                Some(&identity),
            )?;
        }
        // A format or sim change never selects the xtask tests, so a new WP-era id used to reach
        // main and fail only there (#1519, #1522). This runs one xtask test binary, not the
        // workspace, and sits before the crate suites so it fails fast.
        if wp_ratchet_needed(&plan) {
            let identity = wp_ratchet_identity(&plan);
            step(
                root,
                "WP-era identifier ratchet (xtask)",
                "wp-ratchet",
                &[],
                &["test", "-p", "xtask", "no_new_wp_era_identifiers"],
                Some(&identity),
            )?;
        }
        let mut pkgs = Vec::new();
        for c in &plan.test {
            pkgs.extend(["-p", c]);
        }
        let mut clippy = vec!["clippy"];
        clippy.extend(&pkgs);
        clippy.extend(["--tests", "--", "-D", "warnings"]);
        step(
            root,
            &format!("clippy ({})", plan.test.join(", ")),
            "clippy",
            &plan.test,
            &clippy,
            None,
        )?;
        let mut t = vec!["test"];
        t.extend(&pkgs);
        step(
            root,
            &format!("tests ({})", plan.test.join(", ")),
            "test",
            &plan.test,
            &t,
            None,
        )?;
        if !plan.compile.is_empty() {
            let mut c = vec!["check", "--tests"];
            for p in &plan.compile {
                c.extend(["-p", p]);
            }
            step(
                root,
                &format!("compile check of dependents ({})", plan.compile.join(", ")),
                "compile-check",
                &plan.compile,
                &c,
                None,
            )?;
        }
        Ok(())
    })();
    let dir = root.join("target/xtask");
    let _ = std::fs::create_dir_all(&dir);
    let status = if result.is_ok() { "PASSED" } else { "FAILED" };
    let _ = std::fs::write(
        dir.join("last-check.txt"),
        format!(
            "plan={}\nfull={}\n{summary}xtask check: {status}\n",
            plan.identity, plan.full
        ),
    );
    result
}

/// The first 64-digit lowercase hex string in `s`.
fn first_hash(s: &str) -> Option<&str> {
    let b = s.as_bytes();
    (0..b.len().saturating_sub(63)).find_map(|i| {
        let w = &s[i..i + 64];
        let edge = |j: usize| j >= b.len() || !b[j].is_ascii_hexdigit();
        (w.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            && (i == 0 || edge(i - 1))
            && edge(i + 64))
        .then_some(w)
    })
}

/// The `left:` value of a failed `assert_eq!` in test output.
fn left_value(text: &str) -> Option<&str> {
    text.lines().find_map(|l| l.trim_start().strip_prefix("left:"))
}

fn repin(root: &Path) -> Result<(), String> {
    // FINAL_HASH in determinism.rs.
    let path = pin_file(root, "/determinism.rs")?;
    let (ok, text) = output(
        root,
        "cargo",
        &["test", "-p", "iwbtg-sim", "--test", "determinism"],
        &[],
    )?;
    if !ok {
        let got = left_value(&text)
            .and_then(first_hash)
            .ok_or_else(|| format!("determinism failed without a new hash:\n{text}"))?;
        replace_const(&path, "const FINAL_HASH: &str = \"", got)?;
        println!("FINAL_HASH -> {got}");
    }
    // The state hash vectors.
    let path = pin_file(root, "/hash_vectors.rs")?;
    let (ok, text) = output(
        root,
        "cargo",
        &["test", "-p", "iwbtg-sim", "--test", "it", "hash_vectors::"],
        &[],
    )?;
    if !ok {
        let left =
            left_value(&text).ok_or_else(|| format!("hash_vectors failed without a new value:\n{text}"))?;
        let mut src = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        for entry in left.split("\", \"") {
            let entry = entry.trim_matches(|c| c == '[' || c == ']' || c == '"' || c == ' ');
            let Some((name, hash)) = entry.split_once(": ") else {
                continue;
            };
            let key = format!("\"{name}\",\n        \"");
            let at = src
                .find(&key)
                .ok_or_else(|| format!("vector {name} not found in hash_vectors.rs"))?
                + key.len();
            src.replace_range(at..at + 64, hash);
            println!("hash vector {name} -> {hash}");
        }
        std::fs::write(&path, src).map_err(|e| e.to_string())?;
        println!(
            "The state hash encoding changed: bump HASH_LAYOUT only if the encoding (not just the data) changed."
        );
    }
    // The golden corpus rewrites its own expected file.
    run(
        root,
        "cargo",
        &["test", "-p", "iwbtg-sim", "--test", "golden_corpus"],
        &[("UPDATE_GOLDEN", "1")],
    )?;
    // Everything again, now pinned.
    run(
        root,
        "cargo",
        &[
            "test",
            "-p",
            "iwbtg-sim",
            "--test",
            "determinism",
            "--test",
            "golden_corpus",
        ],
        &[],
    )?;
    run(
        root,
        "cargo",
        &["test", "-p", "iwbtg-sim", "--test", "it", "hash_vectors::"],
        &[],
    )?;
    // After the freeze point, record the pins under SIM_VERSION; fails when a pin changed without
    // a SIM_VERSION bump (docs/architecture/versioning.md, #783). Inert before the freeze.
    run(
        root,
        "cargo",
        &["test", "-p", "iwbtg-sim", "--test", "it", "sim_version_bump::"],
        &[("UPDATE_SIM_PINS", "1")],
    )
    .map_err(|e| format!("{e}: a pinned hash changed after the freeze point; bump SIM_VERSION and repin"))?;
    println!("Re-pinned. Review `git diff` and say in the PR why the hashes changed.");
    Ok(())
}

fn replace_const(path: &Path, prefix: &str, value: &str) -> Result<(), String> {
    let mut src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let at = src
        .find(prefix)
        .ok_or_else(|| format!("{prefix} not found in {}", path.display()))?
        + prefix.len();
    src.replace_range(at..at + 64, value);
    std::fs::write(path, src).map_err(|e| e.to_string())
}

fn regen(root: &Path) -> Result<(), String> {
    // The feel contract list follows its folder (#1241); the schemas and script docs follow below.
    if write_feel_contracts(root)? {
        println!("regen: wrote {FEEL_CONTRACTS_FILE}");
    }
    // Write the schemas with the writer test alone, then run the whole file against them. In one
    // run the other tests read the schema files in parallel with the writer and can validate
    // against the old ones, failing once and passing on the next regen (#703).
    run(
        root,
        "cargo",
        &[
            "test",
            "-p",
            "iwbtg-format",
            "--test",
            "schemas",
            "--",
            "--exact",
            "schema_files_are_not_stale",
        ],
        &[("UPDATE_SCHEMAS", "1")],
    )?;
    run(
        root,
        "cargo",
        &["test", "-p", "iwbtg-format", "--test", "schemas"],
        &[],
    )?;
    run(
        root,
        "cargo",
        &["run", "-q", "-p", "iwbtg-tools", "--", "script", "api-docs"],
        &[],
    )?;
    let (_, status) = output(
        root,
        "git",
        &["status", "--short", "docs/format", FEEL_CONTRACTS_FILE],
        &[],
    )?;
    println!(
        "{}",
        if status.trim().is_empty() {
            "generated files were already up to date"
        } else {
            status.trim_end()
        }
    );
    Ok(())
}

fn pr_body(root: &Path, issue: Option<String>) -> Result<(), String> {
    let (_, branch) = output(root, "git", &["branch", "--show-current"], &[])?;
    let branch = branch.trim();
    let issue = issue.or_else(|| {
        branch
            .strip_prefix("issue-")
            .and_then(|r| r.split('-').next())
            .map(str::to_owned)
    });
    let (_, log) = output(
        root,
        "git",
        &[
            "log",
            "--format=- %s",
            "--first-parent",
            "--no-merges",
            "origin/main..HEAD",
        ],
        &[],
    )?;
    let files = changed_files(root, false)?;
    let docs: Vec<&String> = files
        .iter()
        .filter(|f| f.ends_with(".md") || f.starts_with("docs/"))
        .collect();
    let check = std::fs::read_to_string(root.join("target/xtask/last-check.txt"))
        .unwrap_or_else(|_| "(run `cargo xtask check` first)\n".into());
    let was_full = check
        .lines()
        .find_map(|line| line.strip_prefix("full="))
        .map(|value| value == "true")
        .unwrap_or(false);
    let current_identity = changed_files(root, true)
        .ok()
        .and_then(|files| shared_plan(root, &files, was_full).ok())
        .map(|plan| plan.identity);
    let stale = stale_marker(plan_identity_from_check(&check), current_identity.as_deref());
    let mut body = String::new();
    let _ = writeln!(body, "Closes #{}\n", issue.as_deref().unwrap_or("<issue>"));
    let _ = writeln!(body, "## What changed\n\n{}", log.trim_end());
    let _ = writeln!(
        body,
        "\n## Docs updated\n\n{}",
        if docs.is_empty() {
            "none: <why none are needed>".into()
        } else {
            docs.iter()
                .map(|d| format!("- {d}"))
                .collect::<Vec<_>>()
                .join("\n")
        }
    );
    let _ = writeln!(
        body,
        "\n## Follow-up issues\n\nNone, or the issues filed for findings (link them here).\n"
    );
    let _ = writeln!(body, "## User check\n\n```\n{}{}```\n", stale, check);
    print!("{body}");
    Ok(())
}

/// The version of the `[[package]]` named `name` in a `Cargo.lock`'s text.
fn lock_version<'a>(lock: &'a str, name: &str) -> Option<&'a str> {
    let wanted = format!("name = \"{name}\"");
    let mut lines = lock.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if line == wanted {
            let version = lines.next()?.strip_prefix("version = \"")?;
            return version.strip_suffix('"');
        }
    }
    None
}

/// The version in the output of `wasm-bindgen --version` (`wasm-bindgen 0.2.120`).
fn cli_version(output: &str) -> Option<&str> {
    output.split_whitespace().nth(1)
}

/// What to tell the user about the CLI, or Ok when it matches the lock.
fn wasm_bindgen_problem(lock: &str, installed: Option<&str>) -> Result<(), String> {
    let want = lock_version(lock, "wasm-bindgen").ok_or("Cargo.lock has no wasm-bindgen package")?;
    let fix = format!("cargo install -f wasm-bindgen-cli --version {want} --locked");
    match installed.and_then(cli_version) {
        Some(have) if have == want => Ok(()),
        Some(have) => Err(format!(
            "wasm-bindgen CLI is {have}, Cargo.lock has {want}; run: {fix}"
        )),
        None => Err(format!(
            "wasm-bindgen CLI is missing or unreadable; Cargo.lock has {want}; run: {fix}"
        )),
    }
}

/// `doctor`: machine checks against the repo. Fails when one does.
/// The committed fidelity baseline: `<game>/<case> <STATUS>` lines, no private data.
const FIDELITY_BASELINE: &str = "crates/tools/fidelity-baseline.txt";

/// The public feel contracts: fixtures only, so every CI target can run them.
const FEEL_PUBLIC: &str = "crates/sim/tests/feel";

/// The generated list the sim test includes; it must follow the folder (#1241).
const FEEL_CONTRACTS_FILE: &str = "crates/sim/tests/feel/contracts.rs";

/// The port feel contracts and their baseline (the games repo, #1168).
const FEEL_PORT: &str = "crates/tools/feel";
const FEEL_BASELINE: &str = "crates/tools/feel-baseline.txt";

/// Every `*.toml` below the public feel folder, as `(id, include path)` sorted by id. The include
/// path is relative to `contracts.rs`, which sits in the same folder.
fn feel_contract_entries(root: &Path) -> Result<Vec<(String, String)>, String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
            if path.is_dir() {
                walk(&path, out)?;
            } else if path.extension().is_some_and(|e| e == "toml") {
                out.push(path);
            }
        }
        Ok(())
    }
    let dir = root.join(FEEL_PUBLIC);
    let mut paths = Vec::new();
    walk(&dir, &mut paths)?;
    let mut entries: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            let rel = path.strip_prefix(&dir).unwrap_or(path);
            let include = rel.to_string_lossy().replace('\\', "/");
            let id = rel.with_extension("").to_string_lossy().replace('\\', "/");
            (id, include)
        })
        .collect();
    entries.sort();
    Ok(entries)
}

/// The generated `contracts.rs`: a sorted `CONTRACTS` list of `(id, include_str!)` entries. The
/// sim test includes it, so a filesystem-less target runs exactly the folder's contracts.
fn feel_contracts_text(entries: &[(String, String)]) -> String {
    let mut out = String::from(
        "// @generated by `cargo xtask feel` from every `*.toml` below `crates/sim/tests/feel/`.\n\
         // Do not edit: add a contract file and run `cargo xtask feel` (or `cargo xtask regen`).\n\
         pub const CONTRACTS: &[(&str, &str)] = &[\n",
    );
    for (id, include) in entries {
        let _ = writeln!(out, "    ({id:?}, include_str!({include:?})),");
    }
    out.push_str("];\n");
    out
}

/// Rewrites `contracts.rs` when the folder changed; `Ok(true)` when it wrote.
fn write_feel_contracts(root: &Path) -> Result<bool, String> {
    let entries = feel_contract_entries(root)?;
    let text = feel_contracts_text(&entries);
    let path = root.join(FEEL_CONTRACTS_FILE);
    if std::fs::read_to_string(&path).is_ok_and(|current| current == text) {
        return Ok(false);
    }
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(true)
}

/// `feel`: the public contract set, then the port set (#1168) when `IWBTG_GAMES` (the games repo)
/// and/or `IWBTG_RESEARCH` (its `build/packages` converter output) is set. The tool looks a
/// contract's package up in that order, prints the source each contract ran against, and reports
/// a package in neither as SKIP.
///
/// The tool exits 0 when every contract is OK/OPEN, 1 on a failure or a regression against the
/// baseline, and 2 on a tool error; this wrapper keeps those codes so CI can tell them apart.
fn feel(root: &Path, bless: bool) -> Result<(), String> {
    // Keep the sim test's compiled-in list in step with the folder (#1241); the test also fails
    // when it is stale, so a developer who skips this command is told to run it.
    if write_feel_contracts(root)? {
        println!("feel: regenerated {FEEL_CONTRACTS_FILE}");
    }
    run(
        root,
        "cargo",
        &["build", "-q", "-p", "iwbtg-tools", "--bin", "iwbtg"],
        &[],
    )?;
    let mut failed = feel_sweep(root, &["--contracts", FEEL_PUBLIC])? != 0;
    let games = env_dir("IWBTG_GAMES", None)?;
    let converter = env_dir("IWBTG_RESEARCH", Some("build/packages"))?;
    let has_port_set = root.join(FEEL_PORT).is_dir();
    if !has_port_set {
        if bless {
            println!("feel: no port contract set ({FEEL_PORT}) to bless");
        }
    } else if games.is_none() && converter.is_none() {
        // Say so out loud: a silent pass here would read as "every port's contracts hold".
        println!(
            "feel: port set ({FEEL_PORT}) not run: set IWBTG_GAMES (the games checkout) and/or IWBTG_RESEARCH (its build/packages){}",
            if bless { "; nothing blessed" } else { "" }
        );
    } else {
        let mut args = vec!["--contracts", FEEL_PORT, "--baseline", FEEL_BASELINE];
        if let Some(games) = &games {
            args.extend(["--packages", games.as_str()]);
        }
        if let Some(converter) = &converter {
            args.extend(["--converter-packages", converter.as_str()]);
        }
        if bless {
            args.push("--bless");
        }
        failed |= feel_sweep(root, &args)? != 0;
    }
    if failed {
        Err("feel: a contract failed (or the sweep could not run); see above".into())
    } else {
        Ok(())
    }
}

/// A folder named by an environment variable (plus `sub`), as a plain absolute path; `None`
/// when the variable is unset. A set variable that does not resolve is an error, not a skip.
fn env_dir(var: &str, sub: Option<&str>) -> Result<Option<String>, String> {
    let Some(base) = std::env::var_os(var).map(PathBuf::from) else {
        return Ok(None);
    };
    let dir = sub.map_or_else(|| base.clone(), |s| base.join(s));
    let dir = dir
        .canonicalize()
        .map_err(|e| format!("{var} {}: {e}", dir.display()))?;
    let dir = dir.to_string_lossy().into_owned();
    // Windows `canonicalize` returns a verbatim (`\\?\`) path; the plain form reads better.
    Ok(Some(dir.strip_prefix(r"\\?\").unwrap_or(&dir).to_owned()))
}

/// One `iwbtg feel sweep` run; returns the tool's exit code (0 OK/OPEN, 1 failure, 2 tool error)
/// and stops the wrapper with 2 on a tool error.
fn feel_sweep(root: &Path, extra: &[&str]) -> Result<i32, String> {
    let mut args = vec!["run", "-q", "-p", "iwbtg-tools", "--", "feel", "sweep"];
    args.extend_from_slice(extra);
    println!("$ cargo {}", args.join(" "));
    let status = Command::new("cargo")
        .args(&args)
        .current_dir(root)
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    let code = status.code().unwrap_or(2);
    if code == 2 {
        // The tool could not read the contracts root or the baseline: a tool error, not a
        // contract failure.
        std::process::exit(2);
    }
    Ok(code)
}

/// `fidelity`: one `iwbtg trace sweep` run (all packages and cases in one process).
fn fidelity(root: &Path, bless: bool) -> Result<(), String> {
    let Some(research) = std::env::var_os("IWBTG_RESEARCH") else {
        return Err(
            "set IWBTG_RESEARCH to the research checkout (for example ../iwbtg-collection-research)".into(),
        );
    };
    // Relative to the folder cargo xtask was started in, not the workspace root it runs in.
    let research = Path::new(&research)
        .canonicalize()
        .map_err(|e| format!("IWBTG_RESEARCH {}: {e}", Path::new(&research).display()))?;
    let research = research.to_string_lossy();
    // Windows `canonicalize` returns a verbatim (`\\?\`) path; the plain form is easier to read.
    let research = research.strip_prefix(r"\\?\").unwrap_or(&research).to_owned();
    let mut args = vec![
        "run",
        "-q",
        "-p",
        "iwbtg-tools",
        "--",
        "trace",
        "sweep",
        "--cases",
        "crates/tools/cases",
        "--baseline",
        FIDELITY_BASELINE,
        "--research",
        &research,
    ];
    if bless {
        args.push("--bless");
    }
    // Build first: a failed build is no sweep and must not write the stamp (#935).
    run(
        root,
        "cargo",
        &["build", "-q", "-p", "iwbtg-tools", "--bin", "iwbtg"],
        &[],
    )?;
    println!("$ cargo {}", args.join(" "));
    // Capture the output, not just the status: the sweep prints the per-package missing-script
    // counts the progress stamp records (#1425). It is a matrix plus one line per changed case,
    // so buffering it loses no useful live progress.
    let out = Command::new("cargo")
        .args(&args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    print!("{stdout}");
    eprint!("{}", String::from_utf8_lossy(&out.stderr));
    let code = out.status.code();
    // The tool exits 0 when every case is as good as the baseline, 1 on a regression, 2 on a tool error and
    // 101 on a panic: only 0 and 1 mean the sweep ran.
    if matches!(code, Some(0 | 1)) {
        record_fidelity_run(root);
        record_game_progress(root, Path::new(&research), &stdout);
    }
    if code == Some(0) {
        Ok(())
    } else {
        Err(format!(
            "fidelity: a case got worse than {FIDELITY_BASELINE} (or the sweep could not run); see above"
        ))
    }
}

/// `target/xtask/last-fidelity.txt`: `<origin/main commit> <unix seconds>`, written after every sweep that ran, clean or
/// with a regression, so `issues.py start` can warn when sim or format commits landed since (#935). Best effort.
fn record_fidelity_run(root: &Path) {
    let Ok((true, sha)) = output(root, "git", &["rev-parse", "origin/main"], &[]) else {
        return;
    };
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let dir = root.join("target/xtask");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("last-fidelity.txt"), format!("{} {secs}\n", sha.trim()));
}

/// The per-game progress stamp `issues.py start` reads (#1354): a sibling of `last-fidelity.txt`
/// so the sweep-age line keeps its format.
const GAME_PROGRESS_STAMP: &str = "target/xtask/game-progress.txt";

/// `target/xtask/game-progress.txt`, written after a sweep that ran. It records the packages input
/// (present or absent) and one `missing-scripts <game> <n>` line per package from the sweep's own
/// trace-mode load (#1425), so no second `pkg check` pass is needed. Fidelity OK/OPEN/FAIL counts
/// are not copied here: the public `crates/tools/fidelity-baseline.txt` is what the hook reads, so
/// there is one source for them. Best effort: a missing stamp only makes the hook print
/// `not measured`.
fn record_game_progress(root: &Path, research: &Path, sweep_output: &str) {
    let Ok((true, sha)) = output(root, "git", &["rev-parse", "origin/main"], &[]) else {
        return;
    };
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let packages = research.join("build").join("packages");
    let present = packages.is_dir();
    let counts = sweep_missing_scripts(sweep_output);
    let text = game_progress_text(sha.trim(), secs, &packages, present, &counts);
    let path = root.join(GAME_PROGRESS_STAMP);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, text);
}

/// The `missing-scripts <game> <n>` lines `iwbtg trace sweep` prints, keyed by game id. The count
/// comes from the load the sweep already ran, so the stamp matches what `pkg check --trace-mode`
/// reported without a second pass (#1425).
fn sweep_missing_scripts(output: &str) -> BTreeMap<String, usize> {
    output
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            (words.next()? == "missing-scripts").then_some(())?;
            let game = words.next()?.to_owned();
            let count = words.next()?.parse().ok()?;
            words.next().is_none().then_some((game, count))
        })
        .collect()
}

/// The stamp text: a comment, the commit and time, the packages input, then sorted per-game counts.
fn game_progress_text(
    sha: &str,
    secs: u64,
    packages: &Path,
    present: bool,
    counts: &BTreeMap<String, usize>,
) -> String {
    let mut out = format!(
        "# Per-game port progress recorded by `cargo xtask fidelity` (#1354); generated.\n\
         # {sha} {secs}\n\
         missing-scripts-input {} {}\n",
        if present { "present" } else { "absent" },
        packages.display()
    );
    for (game, count) in counts {
        let _ = writeln!(out, "missing-scripts {game} {count}");
    }
    out
}

/// What a preflight run needs: only the selected scope's tools are required (#1030).
#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Native,
    Browser,
    Wasi,
    All,
}

impl Scope {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "native" => Ok(Self::Native),
            "browser" => Ok(Self::Browser),
            "wasi" => Ok(Self::Wasi),
            "all" => Ok(Self::All),
            _ => Err(format!(
                "unknown doctor scope `{s}`; use native, browser, wasi or all"
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Browser => "browser",
            Self::Wasi => "wasi",
            Self::All => "all",
        }
    }

    fn needs_native(self) -> bool {
        matches!(self, Self::Native | Self::All)
    }

    fn needs_browser(self) -> bool {
        matches!(self, Self::Browser | Self::All)
    }

    fn needs_wasi(self) -> bool {
        matches!(self, Self::Wasi | Self::All)
    }
}

struct Prerequisite {
    name: &'static str,
    ok: bool,
    detail: String,
    fix: Option<String>,
}

fn ready(name: &'static str, detail: impl Into<String>) -> Prerequisite {
    Prerequisite {
        name,
        ok: true,
        detail: detail.into(),
        fix: None,
    }
}

fn missing(name: &'static str, detail: impl Into<String>, fix: impl Into<String>) -> Prerequisite {
    Prerequisite {
        name,
        ok: false,
        detail: detail.into(),
        fix: Some(fix.into()),
    }
}

fn tool_line(root: &Path, program: &str, args: &[&str]) -> Option<String> {
    output(root, program, args, &[])
        .ok()
        .filter(|(ok, _)| *ok)
        .map(|(_, text)| text.trim().to_owned())
}

/// The `channel` in a `rust-toolchain.toml` (the pinned version `rustup` installs).
fn toolchain_channel(text: &str) -> Option<&str> {
    text.lines().find_map(|l| {
        l.trim()
            .strip_prefix("channel = \"")
            .and_then(|rest| rest.strip_suffix('"'))
    })
}

/// `rustc 1.99.1 (hash date)` matches the pinned channel `1.99.1`.
fn tool_version_matches(line: &str, want: &str) -> bool {
    line.split_whitespace().nth(1) == Some(want)
}

fn list_has(list: &str, wanted: &str) -> bool {
    list.lines().any(|l| l.trim() == wanted)
}

fn native_prerequisites(root: &Path) -> Vec<Prerequisite> {
    let mut checks = Vec::new();
    let want = std::fs::read_to_string(root.join("rust-toolchain.toml")).ok();
    let channel = want.as_deref().and_then(toolchain_channel).unwrap_or("pinned");
    match tool_line(root, "rustc", &["--version"]) {
        Some(line)
            if want
                .as_deref()
                .and_then(toolchain_channel)
                .is_none_or(|w| tool_version_matches(&line, w)) =>
        {
            checks.push(ready("rustc", line))
        }
        Some(line) => checks.push(missing(
            "rustc",
            format!("{line}; rust-toolchain.toml pins {channel}"),
            "rustup show (from the repo root) or rustup toolchain install <channel>",
        )),
        None => checks.push(missing(
            "rustc",
            "not found",
            "install rustup, then run `rustup show` in the repo root",
        )),
    }
    match tool_line(root, "cargo", &["--version"]) {
        Some(line) => checks.push(ready("cargo", line)),
        None => checks.push(missing(
            "cargo",
            "not found",
            "install rustup (it provides cargo)",
        )),
    }
    for (name, args) in [("rustfmt", &["--version"][..]), ("clippy", &["--version"][..])] {
        let program = if name == "clippy" { "cargo" } else { name };
        let args = if name == "clippy" {
            &["clippy", "--version"][..]
        } else {
            args
        };
        match tool_line(root, program, args) {
            Some(line) => checks.push(ready(name, line)),
            None => checks.push(missing(name, "not found", format!("rustup component add {name}"))),
        }
    }
    let probe_dir = root.join("target/xtask");
    let writable = std::fs::create_dir_all(&probe_dir)
        .and_then(|()| std::fs::write(probe_dir.join("doctor-write-test"), b"ok"))
        .and_then(|()| std::fs::remove_file(probe_dir.join("doctor-write-test")));
    match writable {
        Ok(()) => checks.push(ready("target/", "writable")),
        Err(e) => checks.push(missing(
            "target/",
            format!("{e}"),
            "fix the permissions or free disk space under target/",
        )),
    }
    for fixture in [
        "crates/sim/tests/golden/one_room.pkg",
        "crates/client/showcase.pkg",
    ] {
        if root.join(fixture).is_dir() {
            checks.push(ready("fixture", fixture));
        } else {
            checks.push(missing(
                "fixture",
                format!("{fixture} is missing"),
                "git restore the fixture (a partial checkout omits it)",
            ));
        }
    }
    if cfg!(target_os = "linux") {
        let libs = [
            "wayland-client",
            "xkbcommon",
            "x11",
            "xcursor",
            "xi",
            "xrandr",
            "libudev",
            "alsa",
        ];
        let mut args = vec!["--exists", "--print-errors"];
        args.extend(libs);
        match output(root, "pkg-config", &args, &[]) {
            Ok((true, _)) => checks.push(ready("system libraries", "pkg-config found all client libraries")),
            Ok((false, text)) => checks.push(missing(
                "system libraries",
                text.trim().lines().next().unwrap_or("pkg-config reported missing libraries"),
                "sudo apt-get update && sudo apt-get install -y --no-install-recommends pkg-config libwayland-dev libxkbcommon-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev libudev-dev libasound2-dev",
            )),
            Err(e) => checks.push(missing(
                "system libraries",
                e,
                "install pkg-config and the client's Linux development packages (docs/dev-setup.md)",
            )),
        }
    }
    checks
}

fn browser_prerequisites(root: &Path, targets: Option<&str>) -> Vec<Prerequisite> {
    let mut checks = Vec::new();
    match targets {
        Some(list) if list_has(list, "wasm32-unknown-unknown") => {
            checks.push(ready("wasm32-unknown-unknown", "target installed"))
        }
        Some(_) => checks.push(missing(
            "wasm32-unknown-unknown",
            "target not installed",
            "rustup target add wasm32-unknown-unknown",
        )),
        None => checks.push(missing(
            "wasm32-unknown-unknown",
            "rustup target list unavailable",
            "rustup target add wasm32-unknown-unknown",
        )),
    }
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).unwrap_or_default();
    let installed = tool_line(root, "wasm-bindgen", &["--version"]);
    let want = lock_version(&lock, "wasm-bindgen").unwrap_or("in Cargo.lock");
    let fix = format!("cargo install -f wasm-bindgen-cli --version {want} --locked");
    match wasm_bindgen_problem(&lock, installed.as_deref()) {
        Ok(()) => checks.push(ready("wasm-bindgen", format!("CLI {want} matches Cargo.lock"))),
        Err(e) => checks.push(missing("wasm-bindgen", e, fix)),
    }
    match tool_line(root, "node", &["--version"]) {
        Some(line) => checks.push(ready("node", line)),
        None => checks.push(missing(
            "node",
            "not found (the browser golden-corpus runner uses node)",
            "install Node.js 18 or newer",
        )),
    }
    checks
}

fn wasi_prerequisites(root: &Path, targets: Option<&str>) -> Vec<Prerequisite> {
    let mut checks = Vec::new();
    match targets {
        Some(list) if list_has(list, "wasm32-wasip1") => {
            checks.push(ready("wasm32-wasip1", "target installed"))
        }
        Some(_) => checks.push(missing(
            "wasm32-wasip1",
            "target not installed",
            "rustup target add wasm32-wasip1",
        )),
        None => checks.push(missing(
            "wasm32-wasip1",
            "rustup target list unavailable",
            "rustup target add wasm32-wasip1",
        )),
    }
    let wasmtime = tool_line(root, "wasmtime", &["--version"]);
    let node = tool_line(root, "node", &["--version"]);
    match (wasmtime, node) {
        (Some(line), _) => checks.push(ready("wasi runner", format!("wasmtime {line}"))),
        (None, Some(line)) => checks.push(ready("wasi runner", format!("node {line} (WASI fallback)"))),
        (None, None) => checks.push(missing(
            "wasi runner",
            "neither wasmtime nor node found",
            "install wasmtime (docs/dev-setup.md) or Node.js 18 or newer",
        )),
    }
    checks
}

/// `doctor [--scope native|browser|wasi|all] [--warm]`: required tools and fixtures for the
/// selected work, all reported together; `--warm` runs `cargo fetch --locked` at default settings.
fn doctor(root: &Path, scope: Scope, warm: bool) -> Result<(), String> {
    let targets = tool_line(root, "rustup", &["target", "list", "--installed"]);
    let mut checks = Vec::new();
    if scope.needs_native() {
        checks.extend(native_prerequisites(root));
    }
    if scope.needs_browser() {
        checks.extend(browser_prerequisites(root, targets.as_deref()));
    }
    if scope.needs_wasi() {
        checks.extend(wasi_prerequisites(root, targets.as_deref()));
    }
    let missing_count = checks.iter().filter(|c| !c.ok).count();
    for check in &checks {
        if check.ok {
            println!("ok: {} ({})", check.name, check.detail);
        } else {
            println!("MISSING: {}: {}", check.name, check.detail);
            if let Some(fix) = &check.fix {
                println!("  fix: {fix}");
            }
        }
    }
    println!(
        "doctor ({}): {} checks, {missing_count} missing",
        scope.label(),
        checks.len()
    );
    if missing_count > 0 {
        return Err(format!(
            "{missing_count} missing prerequisite(s) for scope `{}`; run the fixes above",
            scope.label()
        ));
    }
    if warm {
        run(root, "cargo", &["fetch", "--locked"], &[])?;
        println!("ok: cargo fetch --locked warmed the registry cache at the default fingerprint");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: &str = "[[package]]
name = \"wasm-bindgen-macro\"
version = \"0.2.129\"

[[package]]
name = \"wasm-bindgen\"
version = \"0.2.129\"
source = \"x\"
";

    /// WP-era ids (`WP` plus a number) are retired (docs/architecture/testing.md, "Naming
    /// tests"). Each count is a ceiling: an area that gains one fails. #986 removed the ownership
    /// headers and #1407 excluded the generated schemas, so #1426 set each ceiling to the tracked
    /// count at `fe3d58c`; the PR that removes an area's last ids lowers its ceiling here.
    /// `crates/tools` was 1 when #1350 opened: if that PR changes the count, adjust that entry
    /// there.
    const WP_CEILINGS: &[(&str, usize)] = &[
        ("(root)", 1),
        (".claude", 1),
        (".github", 0),
        ("crates/format", 63),
        ("crates/sim", 2),
        ("crates/sim/src", 7),
        ("crates/sim/tests", 127),
        ("crates/tools", 1),
        ("crates/xtask", 0),
        ("docs", 9),
        ("docs/architecture", 12),
        ("docs/format", 32),
        ("docs/lanes", 0),
        ("docs/plans", 377),
        ("scripts", 0),
    ];

    /// Generated files mirror authored text: the dashboard snapshots quote GitHub issue titles and
    /// the format schemas repeat sim/format doc comments. Their WP ids are counted where they are
    /// authored (the issues; `crates/sim/src`), so counting the generated copy again double-counts
    /// and makes a new generated file break a ceiling without a new authored id (#1407).
    fn wp_excluded(rel: &str) -> bool {
        rel.starts_with("docs/format/schema/")
            || matches!(
                rel,
                "progress/data.json"
                    | "progress/history.json"
                    | "progress/index.html"
                    | "progress/exporter.json"
            )
    }

    #[test]
    fn generated_files_do_not_count_against_the_wp_ratchet() {
        for rel in [
            "docs/format/schema/v0/physics.schema.json",
            "docs/format/schema/v1/physics.schema.json",
            "progress/data.json",
            "progress/index.html",
        ] {
            assert!(wp_excluded(rel), "{rel} is generated output");
        }
        for rel in [
            "docs/format/physics.md",
            "progress/README.md",
            "crates/sim/src/def/physics.rs",
            "docs/format/converter-requests.md",
        ] {
            assert!(!wp_excluded(rel), "{rel} is authored text");
        }
    }

    #[test]
    fn no_new_wp_era_identifiers() {
        let root = workspace_root();
        let files = tracked_files(&root).expect("the WP ratchet reads tracked files");
        let problems = wp_ceiling_problems(&wp_counts(&root, &files));
        assert!(
            problems.is_empty(),
            "new WP-era identifiers (docs/architecture/testing.md, \"Naming tests\"); a WP id may stay only where it explains history a reader needs:\n{}",
            problems.join("\n")
        );
    }

    /// A gitignored local file must not count against the ratchet (#1345). Before this fix the
    /// walk read every file on disk, so a `.scratch/` folder (this repo ignores it) failed a clean
    /// local checkout while CI, which has no such file, passed.
    #[test]
    fn a_gitignored_file_with_a_wp_id_does_not_count() {
        let root = workspace_root();
        let probe = root.join(".scratch").join("wp-ratchet-probe.txt");
        std::fs::create_dir_all(probe.parent().expect("probe parent")).expect("create probe folder");
        // Spell the ids in pieces so this source file does not itself carry a WP id.
        let id = "WP";
        std::fs::write(&probe, format!("{id}1 {id}2\n")).expect("write probe");
        let tracked = tracked_files(&root).expect("git ls-files");
        let mut with_probe = tracked.clone();
        with_probe.push(probe.clone());
        let clean = wp_counts(&root, &tracked);
        let naive = wp_counts(&root, &with_probe);
        let _ = std::fs::remove_file(&probe);
        assert!(!tracked.contains(&probe), "the probe must not be tracked");
        // Two ids in the probe would break the `(root)` ceiling if a folder walk counted them.
        assert!(
            wp_ceiling_problems(&naive)
                .iter()
                .any(|p| p.starts_with("(root)")),
            "the probe must be able to break the ratchet when counted: {naive:?}"
        );
        // The tracked scan does not see the probe: its `(root)` count is the naive one minus the
        // probe's two ids. Check only this, so an unrelated area over its ceiling (a merge wave
        // waiting for its own fix) does not make this regression test fail for the wrong reason.
        let root_ids = |counts: &BTreeMap<String, usize>| counts.get("(root)").copied().unwrap_or(0);
        assert_eq!(root_ids(&naive), root_ids(&clean) + 2, "clean: {clean:?}");
    }

    /// The repository's tracked files (`git ls-files -z`), so a gitignored local file never counts.
    /// Err when git cannot list them: silently reading every file on disk is the bug this replaces.
    fn tracked_files(root: &Path) -> Result<Vec<PathBuf>, String> {
        let out = Command::new("git")
            .args(["ls-files", "-z"])
            .current_dir(root)
            .output()
            .map_err(|e| format!("git ls-files: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "git ls-files failed in {}: {}",
                root.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(out
            .stdout
            .split(|b| *b == 0)
            .filter(|name| !name.is_empty())
            .map(|name| root.join(String::from_utf8_lossy(name).as_ref()))
            .collect())
    }

    /// WP ids per [`WP_CEILINGS`] area over `files`.
    fn wp_counts(root: &Path, files: &[PathBuf]) -> BTreeMap<String, usize> {
        let mut found: BTreeMap<String, usize> = BTreeMap::new();
        for path in files {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            if wp_excluded(&rel) {
                continue;
            }
            // The folder walk this replaces skipped files over 1 MiB as not text.
            if std::fs::metadata(path).map_or(true, |m| m.len() > 1 << 20) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue; // binary or not UTF-8
            };
            let count = wp_ids(&text);
            if count > 0 {
                *found.entry(area_of(root, path)).or_default() += count;
            }
        }
        found
    }

    /// Ceiling violations, one line each: over its ceiling or missing one entirely.
    fn wp_ceiling_problems(found: &BTreeMap<String, usize>) -> Vec<String> {
        let mut problems = Vec::new();
        for (area, count) in found {
            match WP_CEILINGS.iter().find(|(name, _)| name == area) {
                Some((_, ceiling)) if count > ceiling => {
                    problems.push(format!("{area}: {count} (ceiling {ceiling})"));
                }
                Some(_) => {}
                None => problems.push(format!("{area}: {count} (no ceiling; add one in the same PR)")),
            }
        }
        problems
    }

    /// The group a path counts under in [`WP_CEILINGS`].
    fn area_of(root: &Path, path: &Path) -> String {
        let rel = path.strip_prefix(root).unwrap_or(path);
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        match parts.first().map(String::as_str) {
            Some("crates")
                if parts.len() > 2 && parts[1] == "sim" && matches!(parts[2].as_str(), "src" | "tests") =>
            {
                format!("crates/sim/{}", parts[2])
            }
            Some("crates") if parts.len() > 1 => format!("crates/{}", parts[1]),
            Some("docs")
                if parts.len() > 1
                    && matches!(parts[1].as_str(), "plans" | "format" | "architecture" | "lanes") =>
            {
                format!("docs/{}", parts[1])
            }
            Some("docs") => "docs".to_string(),
            Some(name) if matches!(name, ".github" | ".claude" | "scripts") => name.to_string(),
            _ => "(root)".to_string(),
        }
    }

    /// `WP`/`wp` followed by an ASCII digit, at a word start.
    fn wp_ids(text: &str) -> usize {
        let bytes = text.as_bytes();
        (0..bytes.len().saturating_sub(2))
            .filter(|&i| {
                ((bytes[i] == b'W' && bytes[i + 1] == b'P') || (bytes[i] == b'w' && bytes[i + 1] == b'p'))
                    && bytes[i + 2].is_ascii_digit()
                    && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'))
            })
            .count()
    }

    #[test]
    fn every_listed_pin_file_exists() {
        let root = workspace_root();
        let listed: Vec<_> = pin_files().collect();
        assert!(listed.len() >= 4, "pin-files.txt lists {listed:?}");
        for p in listed {
            assert!(
                root.join(p).is_file(),
                "pin-files.txt lists {p}, which does not exist"
            );
        }
        assert!(pin_file(&root, "/hash_vectors.rs").is_ok());
        assert!(pin_file(&root, "/nope.rs").is_err());
    }

    #[test]
    fn the_lock_entry_is_found_by_exact_name() {
        assert_eq!(lock_version(LOCK, "wasm-bindgen"), Some("0.2.129"));
        assert_eq!(lock_version(LOCK, "nope"), None);
        let only_macro = "[[package]]
name = \"wasm-bindgen-macro\"
version = \"1.0.0\"
";
        assert_eq!(lock_version(only_macro, "wasm-bindgen"), None);
    }

    #[test]
    fn a_wasm_bindgen_mismatch_or_missing_cli_prints_the_install_command() {
        assert_eq!(wasm_bindgen_problem(LOCK, Some("wasm-bindgen 0.2.129\n")), Ok(()));
        let fix = "cargo install -f wasm-bindgen-cli --version 0.2.129 --locked";
        let old = wasm_bindgen_problem(LOCK, Some("wasm-bindgen 0.2.120\n")).unwrap_err();
        assert!(old.contains("0.2.120") && old.ends_with(fix), "{old}");
        assert!(wasm_bindgen_problem(LOCK, None).unwrap_err().ends_with(fix));
    }

    fn planned(paths: &[&str], full: bool) -> Plan {
        let files: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        shared_plan(&workspace_root(), &files, full).expect("shared plan")
    }

    #[test]
    fn a_sim_change_tests_the_sim_and_checks_what_depends_on_it() {
        let plan = planned(&["crates/sim/src/lib.rs"], false);
        assert_eq!(plan.test, vec!["iwbtg-sim".to_string()]);
        assert_eq!(
            plan.compile,
            vec![
                "iwbtg-client".to_string(),
                "iwbtg-format".to_string(),
                "iwbtg-tools".to_string()
            ]
        );
        assert!(plan.code && !plan.full);
    }

    #[test]
    fn docs_only_builds_nothing_and_manifests_build_everything() {
        let docs = planned(&["README.md", ".claude/stats.py"], false);
        assert!(!docs.code && docs.test.is_empty() && docs.compile.is_empty());
        let full = planned(&["Cargo.lock"], false);
        assert!(full.full && full.code);
        assert_eq!(full.test.len(), 6);
        assert!(full.compile.is_empty());
    }

    #[test]
    fn prose_docs_do_not_rebuild_sim_but_test_fixtures_do() {
        assert!(!planned(&["docs/architecture/client/shell.md"], false).code);
        for path in include_str!("../test-docs.txt")
            .lines()
            .map(str::trim)
            .filter(|p| !p.is_empty() && !p.starts_with('#'))
        {
            let plan = planned(&[path], false);
            assert!(
                plan.test.contains(&"iwbtg-sim".to_string()),
                "{path}: {:?}",
                plan.test
            );
            assert!(
                plan.test.contains(&"iwbtg-format".to_string()),
                "{path}: {:?}",
                plan.test
            );
            assert!(
                plan.compile.contains(&"iwbtg-client".to_string()),
                "{path}: {:?}",
                plan.compile
            );
        }
    }

    #[test]
    fn hashes_and_left_values_are_found() {
        let h = "07b30b2355be052d149bb06b392d4924d3a80a477a607191779b9d63bd167092";
        let text = format!(
            "assertion `left == right` failed\n  left: \"{h}\"\n right: \"{}\"",
            "0".repeat(64)
        );
        assert_eq!(left_value(&text).and_then(first_hash), Some(h));
        assert_eq!(first_hash(&format!("x{h}0")), None, "65 digits is not a hash");
    }

    /// The `game-progress.txt` layout the status hook parses (#1354).
    #[test]
    fn the_game_progress_stamp_has_a_fixed_layout() {
        let mut counts = BTreeMap::new();
        counts.insert("kaenbyou".to_string(), 1200);
        counts.insert("animus".to_string(), 0);
        let text = game_progress_text("abc123", 42, Path::new("/r/build/packages"), true, &counts);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with("# Per-game port progress"), "{text}");
        assert_eq!(lines[1], "# abc123 42");
        assert_eq!(lines[2], "missing-scripts-input present /r/build/packages");
        assert_eq!(lines[3], "missing-scripts animus 0");
        assert_eq!(lines[4], "missing-scripts kaenbyou 1200");
        let absent = game_progress_text(
            "abc123",
            42,
            Path::new("/r/build/packages"),
            false,
            &BTreeMap::new(),
        );
        assert!(
            absent.contains("missing-scripts-input absent /r/build/packages"),
            "{absent}"
        );
        assert!(!absent.contains("missing-scripts "), "{absent}");
    }

    /// The stamp's counts come from the sweep's own output lines (#1425).
    #[test]
    fn the_sweep_missing_scripts_are_parsed() {
        let output = "game  OK OPEN FAIL\n\
                      kaenbyou  3 1 0\n\
                      \n\
                      missing scripts per package (objects whose script does not exist):\n\
                      missing-scripts animus 0\n\
                      missing-scripts kaenbyou 37\n\
                      \n\
                      fidelity: 2 games (2 with cases), 4 cases: 3 OK, 1 OPEN, 0 FAIL\n";
        let counts = sweep_missing_scripts(output);
        assert_eq!(counts.get("animus"), Some(&0));
        assert_eq!(counts.get("kaenbyou"), Some(&37));
        assert_eq!(counts.len(), 2);
        assert!(sweep_missing_scripts("no counts here\n").is_empty());
        // A malformed or unrelated line is never read as a count.
        assert!(sweep_missing_scripts("missing-scripts game\n").is_empty());
        assert!(sweep_missing_scripts("missing scripts game 3\n").is_empty());
    }

    #[test]
    fn actionability_and_redaction_are_bounded() {
        assert!(actionable("error[E0308]: mismatched types"));
        assert!(actionable("test result: FAILED. 1 passed; 1 failed"));
        assert!(actionable("  --> src/main.rs:1:1"));
        assert!(!actionable("test some_case ... ok"));
        let red = redact("CARGO_REGISTRIES_TOKEN=abc123 https://user:pass@example.com/x");
        assert!(!red.contains("abc123") && !red.contains("pass@"), "{red}");
        assert!(red.contains("***"), "{red}");
    }

    #[test]
    fn log_names_are_path_safe() {
        assert_eq!(
            log_slug(3, "compile check of dependents (a, b)"),
            "03-compile-check-of-dependents-a-b"
        );
        assert!(!log_slug(1, "../../escape").contains('/'));
    }

    #[test]
    fn a_failing_command_logs_its_nested_cause_without_a_rerun() {
        let root = workspace_root();
        let (program, args) = if cfg!(windows) {
            (
                "cmd",
                vec![
                    "/C".to_owned(),
                    "echo outer-cause & echo inner-cause 1>&2 & exit 3".to_owned(),
                ],
            )
        } else {
            (
                "sh",
                vec![
                    "-c".to_owned(),
                    "echo outer-cause; echo inner-cause >&2; exit 3".to_owned(),
                ],
            )
        };
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let report = run_logged(&root, "synthetic failure", 999, program, &refs).expect("runs");
        assert!(!report.ok);
        assert_eq!(report.exit, "exit status: 3");
        let text = std::fs::read_to_string(&report.log).expect("log written");
        assert!(
            text.contains("inner-cause") && text.contains("outer-cause"),
            "{text}"
        );
        assert!(
            report.tail.iter().any(|l| l.contains("inner-cause")),
            "{:?}",
            report.tail
        );
    }

    #[test]
    fn receipts_round_trip_and_reject_tampering() {
        let root = workspace_root();
        let what = "receipt self-test";
        let path = receipt_path(&root, what);
        let _ = std::fs::remove_file(&path);
        let receipt = Receipt {
            identity: "abc".into(),
            argv: "cargo test -p x".into(),
            cwd: "/w".into(),
            status: "ok".into(),
            exit: "exit status: 0".into(),
            log: "target/x.log".into(),
            duration_ms: 12,
        };
        write_receipt(&root, what, &receipt).expect("write");
        let read = read_receipt(&root, what).expect("read");
        assert_eq!(read.identity, "abc");
        assert_eq!(read.argv, "cargo test -p x");
        assert_eq!(read.status, "ok");
        assert_eq!(read.duration_ms, 12);
        assert!(receipt_usable(&read, "abc", "cargo test -p x", "/w"));
        assert!(!receipt_usable(&read, "other", "cargo test -p x", "/w"));
        let text = std::fs::read_to_string(&path).expect("receipt");
        std::fs::write(&path, text.replace("status=ok", "status=failed")).expect("tamper");
        assert!(
            read_receipt(&root, what).is_none(),
            "a tampered receipt must not be reused"
        );
        std::fs::write(&path, "not a receipt").expect("malformed");
        assert!(read_receipt(&root, what).is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_shared_plan_provides_every_local_step_identity() {
        let plan = planned(&["crates/sim/src/lib.rs"], false);
        for key in [
            "rustfmt:",
            "clippy:iwbtg-sim",
            "test:iwbtg-sim",
            "compile-check:iwbtg-client,iwbtg-format,iwbtg-tools",
        ] {
            assert!(
                plan.step_identities.contains_key(key),
                "{key}: {:?}",
                plan.step_identities
            );
        }
        assert!(!plan.identity.is_empty());
    }

    /// A schema input selects the staleness test; the identity is input-sensitive and comes from
    /// the shared plan's recorded steps (#1064).
    #[test]
    fn a_schema_input_change_selects_the_schema_staleness_test() {
        // The sim's doc comments feed the physics schema (the case that cost #1060 and #1207).
        let sim = planned(&["crates/sim/src/def/physics.rs"], false);
        assert!(schema_step_needed(&sim), "{:?}", sim.test);
        let identity = schema_step_identity(&sim);
        assert!(identity.contains("test:iwbtg-sim="), "{identity}");
        assert!(identity.contains("iwbtg-format"), "{identity}");
        // The format crate owns the generator.
        let format = planned(&["crates/format/src/schema.rs"], false);
        assert!(schema_step_needed(&format), "{:?}", format.test);
        assert!(schema_step_identity(&format).contains("test:iwbtg-format="));
        // A client-only or docs-only change never feeds a generated schema.
        assert!(!schema_step_needed(&planned(
            &["crates/client/src/main.rs"],
            false
        )));
        assert!(!schema_step_needed(&planned(
            &["docs/architecture/client/shell.md"],
            false
        )));
    }

    /// A format or sim change runs the WP ratchet; docs-only and client-only plans do not (#1522).
    #[test]
    fn a_format_or_sim_change_runs_the_wp_ratchet() {
        for path in [
            "crates/format/src/load.rs",
            "crates/format/tests/load_objects.rs",
            "crates/sim/src/lib.rs",
        ] {
            let plan = planned(&[path], false);
            assert!(wp_ratchet_needed(&plan), "{path}: {:?}", plan.test);
            let identity = wp_ratchet_identity(&plan);
            assert!(
                identity.contains("iwbtg-format") || identity.contains("iwbtg-sim"),
                "{path}: {identity}"
            );
        }
        assert!(!wp_ratchet_needed(&planned(
            &["docs/architecture/client/shell.md"],
            false
        )));
        assert!(!wp_ratchet_needed(&planned(
            &["crates/client/src/main.rs"],
            false
        )));
    }

    #[test]
    fn stale_marker_flags_missing_or_changed_inputs() {
        assert_eq!(stale_marker(Some("a"), Some("a")), "");
        assert!(stale_marker(Some("a"), Some("b")).starts_with("STALE"));
        assert!(stale_marker(None, Some("b")).starts_with("STALE"));
        assert!(stale_marker(Some("a"), None).starts_with("STALE"));
        assert_eq!(plan_identity_from_check("plan=abc\nrest\n"), Some("abc"));
        assert_eq!(plan_identity_from_check("rest\n"), None);
    }

    #[test]
    fn doctor_scopes_select_only_their_prerequisites() {
        assert!(Scope::Native.needs_native());
        assert!(!Scope::Native.needs_browser() && !Scope::Native.needs_wasi());
        assert!(Scope::Browser.needs_browser());
        assert!(!Scope::Browser.needs_native() && !Scope::Browser.needs_wasi());
        assert!(Scope::Wasi.needs_wasi());
        assert!(!Scope::Wasi.needs_native() && !Scope::Wasi.needs_browser());
        assert!(Scope::All.needs_native() && Scope::All.needs_browser() && Scope::All.needs_wasi());
        assert_eq!(Scope::parse("wasi").unwrap().label(), "wasi");
        assert!(Scope::parse("everything").is_err());
    }

    #[test]
    fn pinned_toolchain_versions_and_installed_targets_are_read() {
        let text = "[toolchain]\nchannel = \"1.99.1\"\ncomponents = [\"rustfmt\"]\n";
        assert_eq!(toolchain_channel(text), Some("1.99.1"));
        assert_eq!(toolchain_channel("profile = \"minimal\""), None);
        assert!(tool_version_matches(
            "rustc 1.99.1 (58cf8f9c3 2026-10-13)",
            "1.99.1"
        ));
        assert!(!tool_version_matches("rustc 1.98.0 (x)", "1.99.1"));
        assert!(list_has(
            "x86_64-pc-windows-msvc\nwasm32-wasip1\n",
            "wasm32-wasip1"
        ));
        assert!(!list_has("wasm32-wasip1\n", "wasm32-unknown-unknown"));
    }

    #[test]
    fn missing_prerequisites_carry_a_repair_command() {
        let absent = missing("target", "not installed", "rustup target add wasm32-wasip1");
        assert!(!absent.ok);
        assert_eq!(absent.fix.as_deref(), Some("rustup target add wasm32-wasip1"));
        let present = ready("target", "installed");
        assert!(present.ok && present.fix.is_none());
    }

    #[test]
    fn the_todo_guard_scans_crate_sources() {
        let root = std::env::temp_dir().join(format!("xtask-todo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("crates/a/src")).unwrap();
        let file = root.join("crates/a/src/lib.rs");
        std::fs::write(&file, "// TODO(OPEN: a free-text marker\n").unwrap();
        assert!(check_open_todos(&root).is_err());
        std::fs::write(&file, "// TODO(OPEN: see #1119\n").unwrap();
        assert!(check_open_todos(&root).is_ok());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn open_todo_markers_must_name_an_issue_or_url() {
        assert!(open_todo_offends(
            "// TODO(OPEN: runner differences between GM 8.0 and 8.1, from the CLONE_SPEC"
        ));
        assert!(!open_todo_offends("// TODO(OPEN: runner differences, see #1099"));
        assert!(!open_todo_offends(
            "// TODO(OPEN: kind list, https://example.com/spec)"
        ));
        assert!(!open_todo_offends("// TODO(#1119): link every open question"));
        assert!(!open_todo_offends("// an ordinary comment"));
    }

    /// The `schema` feature (JSON Schema through `schemars`) is for docs and tests only: never in
    /// the client, native or web (#794). `cargo test --workspace` runs this in CI.
    #[test]
    fn the_client_does_not_depend_on_schemars() {
        for target in [None, Some("wasm32-unknown-unknown")] {
            let mut args = vec!["tree", "-p", "iwbtg-client", "-i", "schemars", "--locked"];
            if let Some(t) = target {
                args.extend(["--target", t]);
            }
            let out = std::process::Command::new(env!("CARGO"))
                .args(&args)
                .current_dir(workspace_root())
                .output()
                .expect("cargo runs");
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(
                !out.status.success() && err.contains("did not match any packages"),
                "schemars is in the client's graph (target {target:?}): {}{err}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
    }

    #[test]
    fn feel_contracts_follow_the_folder() {
        let root = std::env::temp_dir().join(format!("xtask-feel-contracts-{}", std::process::id()));
        let dir = root.join(FEEL_PUBLIC);
        std::fs::create_dir_all(dir.join("b")).unwrap();
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::write(dir.join("b/z.toml"), "").unwrap();
        std::fs::write(dir.join("a/y.toml"), "").unwrap();
        std::fs::write(dir.join("a/ignore.txt"), "").unwrap();
        let entries = feel_contract_entries(&root).unwrap();
        assert_eq!(
            entries,
            vec![
                ("a/y".to_string(), "a/y.toml".to_string()),
                ("b/z".to_string(), "b/z.toml".to_string()),
            ]
        );
        let text = feel_contracts_text(&entries);
        assert!(text.starts_with("// @generated by `cargo xtask feel`"), "{text}");
        assert!(
            text.contains(r#"    ("a/y", include_str!("a/y.toml")),"#),
            "{text}"
        );
        // Writing follows the folder and is idempotent.
        assert!(write_feel_contracts(&root).unwrap());
        assert!(!write_feel_contracts(&root).unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }
}
