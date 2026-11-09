//! Validator fixtures (docs/architecture/testing.md, Testing strategy).
//!
//! - `tests/valid/<pkg>/`: must check with no diagnostics at all.
//! - `tests/invalid/<case>/`: must give exactly the one diagnostic listed under `[<case>]` in
//!   `tests/invalid/expected.toml` (file, line, column, severity, and a message fragment).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use iwbtg_format::{Diagnostic, Manifest, PackageFiles, Severity, check};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    file: String,
    line: u32,
    column: u32,
    #[serde(default = "error_severity")]
    severity: String,
    message: String,
}

fn error_severity() -> String {
    "error".to_owned()
}

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

fn packages(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .expect("fixture folder exists")
        .map(|e| e.expect("readable entry").path())
        .filter(|p| p.is_dir())
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), p))
        .collect();
    out.sort();
    out
}

fn check_dir(dir: &Path) -> Vec<Diagnostic> {
    check(&PackageFiles::load_dir(dir).expect("fixture loads"))
}

#[test]
fn valid_packages_have_no_diagnostics() {
    let pkgs = packages(&tests_dir().join("valid"));
    assert!(pkgs.len() >= 2, "valid fixtures missing");
    for (name, dir) in pkgs {
        let diags = check_dir(&dir);
        assert!(diags.is_empty(), "valid/{name}:\n{}", render(&diags));
    }
}

#[test]
fn invalid_packages_give_their_one_expected_diagnostic() {
    let dir = tests_dir().join("invalid");
    let text = std::fs::read_to_string(dir.join("expected.toml")).expect("expected.toml exists");
    let mut expected: BTreeMap<String, Expected> = toml::from_str(&text).expect("expected.toml parses");

    let mut failures = Vec::new();
    for (name, pkg) in packages(&dir) {
        let diags = check_dir(&pkg);
        let Some(want) = expected.remove(&name) else {
            failures.push(format!(
                "{name}: no entry in expected.toml; got:\n{}",
                render(&diags)
            ));
            continue;
        };
        let ok = match diags.as_slice() {
            [d] => {
                d.file == want.file
                    && d.line == want.line
                    && d.column == want.column
                    && d.severity.to_string() == want.severity
                    && d.message.contains(&want.message)
            }
            _ => false,
        };
        if !ok {
            failures.push(format!(
                "{name}: want {}:{}:{}: {}: ..{}..; got:\n{}",
                want.file,
                want.line,
                want.column,
                want.severity,
                want.message,
                render(&diags)
            ));
        }
    }
    for name in expected.keys() {
        failures.push(format!(
            "{name}: listed in expected.toml but has no fixture folder"
        ));
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn full_manifest_reads_every_field() {
    let files = PackageFiles::load_dir(tests_dir().join("valid/full")).expect("fixture loads");
    let text = std::str::from_utf8(files.get("game.toml").unwrap()).unwrap();
    let m = Manifest::parse(text).expect("parses");
    assert_eq!(m.id.get_ref(), "full_example");
    assert_eq!(*m.runner.get_ref(), iwbtg_format::manifest::Runner::Gm81);
    assert_eq!(
        *m.source.get_ref().permission.get_ref(),
        iwbtg_format::manifest::Permission::CommunityAssumed
    );
    assert_eq!(m.creators.get_ref().len(), 3);
    assert_eq!(m.tick_rate(), 50);
    assert_eq!(m.ops_per_tick(), 2_000_000);
    assert_eq!(m.timing.get_ref().stages.len(), 2);
    assert_eq!(m.third_party.len(), 2);
    let volume = m.music.values().next().unwrap().volume.as_ref().unwrap();
    assert_eq!(volume.get_ref().0.to_bits(), 0.5f64.to_bits());
}

#[test]
fn minimal_manifest_uses_defaults() {
    let files = PackageFiles::load_dir(tests_dir().join("valid/minimal")).expect("fixture loads");
    let text = std::str::from_utf8(files.get("game.toml").unwrap()).unwrap();
    let m = Manifest::parse(text).expect("parses");
    assert_eq!(m.tick_rate(), 50);
    assert_eq!(m.ops_per_tick(), 1_000_000);
    assert!(m.third_party.is_empty());
    assert!(m.source.get_ref().permission_date.is_none());
}

#[test]
fn warnings_do_not_count_as_errors() {
    let diags = check_dir(&tests_dir().join("invalid/schema_comment_missing"));
    assert!(diags.iter().all(|d| d.severity == Severity::Warning));
    assert!(!iwbtg_format::has_errors(&diags));
}

fn render(diags: &[Diagnostic]) -> String {
    if diags.is_empty() {
        return "  (no diagnostics)".to_owned();
    }
    diags
        .iter()
        .map(|d| format!("  {d}"))
        .collect::<Vec<_>>()
        .join("\n")
}
