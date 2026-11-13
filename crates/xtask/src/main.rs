//! `cargo xtask <command>`: the repetitive steps of a PR as one command each (#529).
//!
//! - `check [--all]`: rustfmt check, then clippy and tests of the crates this branch changed
//!   (against `origin/main`, committed or not) plus a compile check of the crates that depend on
//!   them. `--all` runs the whole workspace, like CI. Writes the summary to
//!   `target/xtask/last-check.txt` for `pr-body`.
//! - `repin`: after a deliberate change to hashed sim state, re-pins the state hash vectors
//!   (`tests/hash_vectors.rs`), the `FINAL_HASH` of `tests/wp1_determinism.rs` and the golden
//!   corpus, then runs them again.
//! - `regen`: regenerates the generated files: format schemas and the script API docs.
//! - `pr-body [--issue N]`: prints a PR body for this branch: Closes, the commits, the docs it
//!   changed, and the User check from the last `check`.
//!
//! - `doctor`: checks the machine's tools against the repo; today the installed `wasm-bindgen` CLI
//!   against the `wasm-bindgen` version in `Cargo.lock` (#723).
//!
//! Standard library only; it shells out to cargo and git.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Workspace crates by folder, with the crates that depend on each.
const CRATES: [(&str, &str, &[&str]); 5] = [
    (
        "crates/sim/",
        "iwbtg-sim",
        &["iwbtg-format", "iwbtg-tools", "iwbtg-client"],
    ),
    ("crates/format/", "iwbtg-format", &["iwbtg-tools", "iwbtg-client"]),
    ("crates/tools/", "iwbtg-tools", &[]),
    ("crates/client/", "iwbtg-client", &[]),
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
        Some("doctor") => doctor(&root),
        _ => Err(
            "usage: cargo xtask <check [--all] | repin | regen | pr-body [--issue N] | doctor>".to_string(),
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
            "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml" | ".cargo/config.toml"
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
        f.starts_with("docs/format/")
            || f.starts_with("docs/architecture/")
            || f.starts_with("docs/decisions/")
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
    let path = root.join("crates/sim/tests/wp1_determinism.rs");
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
    let path = root.join("crates/sim/tests/hash_vectors.rs");
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
fn doctor(root: &Path) -> Result<(), String> {
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).map_err(|e| format!("Cargo.lock: {e}"))?;
    let installed = output(root, "wasm-bindgen", &["--version"], &[])
        .ok()
        .filter(|(ok, _)| *ok)
        .map(|(_, text)| text);
    match wasm_bindgen_problem(&lock, installed.as_deref()) {
        Ok(()) => {
            println!("ok: wasm-bindgen CLI matches Cargo.lock");
            Ok(())
        }
        Err(e) => Err(e),
    }
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
    fn hashes_and_left_values_are_found() {
        let h = "07b30b2355be052d149bb06b392d4924d3a80a477a607191779b9d63bd167092";
        let text = format!(
            "assertion `left == right` failed\n  left: \"{h}\"\n right: \"{}\"",
            "0".repeat(64)
        );
        assert_eq!(left_value(&text).and_then(first_hash), Some(h));
        assert_eq!(first_hash(&format!("x{h}0")), None, "65 digits is not a hash");
    }
}
