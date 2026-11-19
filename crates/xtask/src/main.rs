//! `cargo xtask <command>`: the repetitive steps of a PR as one command each (#529).
//!
//! - `check [--all]`: rustfmt check, then clippy and tests of the crates this branch changed
//!   (against `origin/main`, committed or not) plus a compile check of the crates that depend on
//!   them. `--all` runs the whole workspace, like CI. Writes the summary to
//!   `target/xtask/last-check.txt` for `pr-body`.
//! - `fidelity [--bless]`: the trace sweep over every converter package; records the run in
//!   `target/xtask/last-fidelity.txt` (#935).
//! - `repin`: after a deliberate change to hashed sim state, re-pins the state hash vectors
//!   (`tests/hash_vectors.rs`), the `FINAL_HASH` of `tests/wp1_determinism.rs` and the golden
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

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

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

/// Workspace crates by folder, with the crates that depend on each.
const CRATES: [(&str, &str, &[&str]); 6] = [
    (
        "crates/sim/",
        "iwbtg-sim",
        &["iwbtg-format", "iwbtg-tools", "iwbtg-client"],
    ),
    ("crates/format/", "iwbtg-format", &["iwbtg-tools", "iwbtg-client"]),
    ("crates/tools/", "iwbtg-tools", &[]),
    ("crates/client/", "iwbtg-client", &[]),
    // No dependents yet: the skeleton (#152) links neither sim nor format; it gains them when the
    // verify worker lands, and this line and sim/format's lists change with it.
    ("crates/server/", "iwbtg-server", &[]),
    ("crates/xtask/", "xtask", &[]),
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = workspace_root();
    let result = match args.first().map(String::as_str) {
        Some("check") => check(&root, args.iter().any(|a| a == "--all")),
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
        _ => Err(
            "usage: cargo xtask <check [--all] | repin | regen | pr-body [--issue N] | doctor [--scope native|browser|wasi|all] [--warm] | fidelity [--bless]>"
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

/// (crates to test, crates to compile-check) for a list of changed files.
fn affected(files: &[String]) -> (BTreeSet<&'static str>, BTreeSet<&'static str>) {
    let mut test = BTreeSet::new();
    let mut check = BTreeSet::new();
    let workspace_wide = files.iter().any(|f| {
        matches!(
            f.as_str(),
            "Cargo.toml"
                | "Cargo.lock"
                | "rust-toolchain.toml"
                | ".cargo/config.toml"
                | "crates/xtask/test-docs.txt"
        )
    });
    for (dir, name, dependents) in CRATES {
        if workspace_wide || files.iter().any(|f| f.starts_with(dir)) {
            test.insert(name);
            check.extend(dependents.iter().copied());
        }
    }
    // Docs the sim tests read (doc examples, script API, format examples) count as sim/format input.
    if files.iter().any(|f| {
        include_str!("../test-docs.txt")
            .lines()
            .map(str::trim)
            .filter(|p| !p.is_empty() && !p.starts_with('#'))
            .any(|prefix| f.starts_with(prefix))
    }) {
        test.insert("iwbtg-sim");
        test.insert("iwbtg-format");
    }
    let check = check.difference(&test).copied().collect();
    (test, check)
}

fn check(root: &Path, all: bool) -> Result<(), String> {
    let mut summary = String::new();
    let mut step = |root: &Path, what: &str, args: &[&str]| -> Result<(), String> {
        let (ok, text) = output(root, "cargo", args, &[])?;
        let results: Vec<&str> = text.lines().filter(|l| l.starts_with("test result:")).collect();
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
            format!("{what}: {}", if ok { "ok" } else { "FAILED" })
        } else {
            format!(
                "{what}: {} test binaries, {passed} passed, {failed} failed",
                results.len()
            )
        };
        println!("{line}");
        let _ = writeln!(summary, "cargo {} -> {line}", args.join(" "));
        if ok {
            Ok(())
        } else {
            let tail: Vec<&str> = text
                .lines()
                .filter(|l| {
                    l.contains("error")
                        || l.contains("FAILED")
                        || l.contains("panicked")
                        || l.starts_with("  -->")
                        || l.starts_with("---- ")
                })
                .take(40)
                .collect();
            Err(format!("{what} failed:\n{}", tail.join("\n")))
        }
    };
    let result = (|| {
        step(root, "rustfmt", &["fmt", "--all", "--", "--check"])?;
        if all {
            step(
                root,
                "clippy (workspace)",
                &["clippy", "--workspace", "--tests", "--", "-D", "warnings"],
            )?;
            return step(root, "tests (workspace)", &["test", "--workspace"]);
        }
        let files = changed_files(root, true)?;
        let (test, compile) = affected(&files);
        if test.is_empty() {
            println!("no crate changed (docs or harness only): nothing to build");
            return Ok(());
        }
        let mut pkgs = Vec::new();
        for c in &test {
            pkgs.extend(["-p", c]);
        }
        let mut clippy = vec!["clippy"];
        clippy.extend(&pkgs);
        clippy.extend(["--tests", "--", "-D", "warnings"]);
        step(
            root,
            &format!("clippy ({})", test.iter().copied().collect::<Vec<_>>().join(", ")),
            &clippy,
        )?;
        let mut t = vec!["test"];
        t.extend(&pkgs);
        step(
            root,
            &format!("tests ({})", test.iter().copied().collect::<Vec<_>>().join(", ")),
            &t,
        )?;
        if !compile.is_empty() {
            let mut c = vec!["check", "--tests"];
            for p in &compile {
                c.extend(["-p", p]);
            }
            step(
                root,
                &format!(
                    "compile check of dependents ({})",
                    compile.iter().copied().collect::<Vec<_>>().join(", ")
                ),
                &c,
            )?;
        }
        Ok(())
    })();
    let dir = root.join("target/xtask");
    let _ = std::fs::create_dir_all(&dir);
    let status = if result.is_ok() { "PASSED" } else { "FAILED" };
    let _ = std::fs::write(
        dir.join("last-check.txt"),
        format!("{summary}xtask check: {status}\n"),
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
    // FINAL_HASH in wp1_determinism.rs.
    let path = pin_file(root, "/wp1_determinism.rs")?;
    let (ok, text) = output(
        root,
        "cargo",
        &["test", "-p", "iwbtg-sim", "--test", "wp1_determinism"],
        &[],
    )?;
    if !ok {
        let got = left_value(&text)
            .and_then(first_hash)
            .ok_or_else(|| format!("wp1_determinism failed without a new hash:\n{text}"))?;
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
            "wp1_determinism",
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
    let (_, status) = output(root, "git", &["status", "--short", "docs/format"], &[])?;
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
    let _ = writeln!(body, "\n## Follow-up issues\n\nnone\n");
    let _ = writeln!(body, "## User check\n\n```\n{}```\n", check);
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

/// `fidelity`: one `iwbtg trace sweep` run (all packages and cases in one process).
fn fidelity(root: &Path, bless: bool) -> Result<(), String> {
    let Some(research) = std::env::var_os("IWBTG_RESEARCH") else {
        return Err("set IWBTG_RESEARCH to the research checkout (for example ../research)".into());
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
    let code = Command::new("cargo")
        .args(&args)
        .current_dir(root)
        .status()
        .map_err(|e| format!("cargo: {e}"))?
        .code();
    // The tool exits 0 when every case is as good as the baseline, 1 on a regression, 2 and 3 on an error or a
    // panic: only 0 and 1 mean the sweep ran.
    if matches!(code, Some(0 | 1)) {
        record_fidelity_run(root);
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

    #[test]
    fn a_sim_change_tests_the_sim_and_checks_what_depends_on_it() {
        let (test, check) = affected(&["crates/sim/src/lib.rs".to_string()]);
        assert_eq!(test.into_iter().collect::<Vec<_>>(), ["iwbtg-sim"]);
        assert_eq!(
            check.into_iter().collect::<Vec<_>>(),
            ["iwbtg-client", "iwbtg-format", "iwbtg-tools"]
        );
    }

    #[test]
    fn docs_only_builds_nothing_and_cargo_toml_builds_everything() {
        assert!(
            affected(&["README.md".to_string(), ".claude/stats.py".to_string()])
                .0
                .is_empty()
        );
        assert_eq!(affected(&["Cargo.lock".to_string()]).0.len(), CRATES.len());
    }

    #[test]
    fn prose_docs_do_not_rebuild_sim_but_test_fixtures_do() {
        assert!(
            affected(&["docs/architecture/client/shell.md".to_string()])
                .0
                .is_empty()
        );
        for path in include_str!("../test-docs.txt")
            .lines()
            .map(str::trim)
            .filter(|p| !p.is_empty() && !p.starts_with('#'))
        {
            let (test, _) = affected(&[path.to_string()]);
            assert!(test.contains("iwbtg-sim") && test.contains("iwbtg-format"));
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
}
