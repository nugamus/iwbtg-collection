//! `iwbtg pkg check` (docs/plans/sim-core.md 9.4). WP0 creates the file; WP8a owns it.
//!
//! Validates and loads the package with `iwbtg_format::load::load_report`, strictly or in trace
//! mode (`--trace-mode`: unmapped script kinds load inert and converter gaps are warnings, for
//! research converter output). Prints every diagnostic, warnings included, then the gameplay
//! hash and `OK (<n> warnings)`, or `FAILED: <n> errors, <n> warnings`.

use std::path::Path;

use iwbtg_format::Severity;
use iwbtg_format::load::{LoadMode, load_report};

use crate::trace_cmd::{ToolError, load_package};

/// `iwbtg pkg check <pkg> [--trace-mode]`: prints each diagnostic and a summary; `Ok(false)` on
/// any error.
pub fn main(dir: &Path, trace_mode: bool) -> Result<bool, ToolError> {
    let files = load_package(dir)?;
    let mode = if trace_mode {
        LoadMode::Trace {
            unmapped_kinds_inert: true,
        }
    } else {
        LoadMode::Strict
    };
    let report = load_report(&files, mode);
    let mut errors = 0usize;
    let mut warnings = 0usize;
    for d in &report.diagnostics {
        match d.severity {
            Severity::Error => errors += 1,
            Severity::Warning => warnings += 1,
        }
        println!("{d}");
    }
    if report.ok() {
        if let Some(hash) = report.gameplay_hash {
            let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
            println!("gameplay_hash: {hex}");
        }
        println!("OK ({warnings} warnings)");
    } else {
        println!("FAILED: {errors} errors, {warnings} warnings");
    }
    Ok(report.ok())
}

/// The two hashes of a package, as lowercase hex: `(gameplay_hash, content_hash)`. The gameplay
/// hash exists only for a package that loads without errors; those errors are returned instead.
pub fn hashes(files: &iwbtg_format::PackageFiles, trace_mode: bool) -> Result<(String, String), Vec<String>> {
    let mode = if trace_mode {
        LoadMode::Trace {
            unmapped_kinds_inert: true,
        }
    } else {
        LoadMode::Strict
    };
    let report = load_report(files, mode);
    let gameplay = match report.gameplay_hash {
        Some(h) if report.ok() => h,
        _ => {
            return Err(report
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .map(ToString::to_string)
                .collect());
        }
    };
    Ok((hex(&gameplay), hex(&files.content_hash())))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `iwbtg pkg hash <pkg>`: prints `gameplay_hash:` and `content_hash:` (docs/format/v0.md,
/// Versioning). `Ok(false)` (exit 1) when the package has errors, which are printed instead.
pub fn hash_main(dir: &Path, trace_mode: bool) -> Result<bool, ToolError> {
    let files = load_package(dir)?;
    match hashes(&files, trace_mode) {
        Ok((gameplay, content)) => {
            println!("gameplay_hash: {gameplay}");
            println!("content_hash: {content}");
            Ok(true)
        }
        Err(errors) => {
            for e in &errors {
                println!("{e}");
            }
            println!(
                "FAILED: the package has {} errors, so it has no gameplay hash",
                errors.len()
            );
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iwbtg_format::PackageFiles;

    fn golden() -> PackageFiles {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sim/tests/golden/one_room.pkg");
        PackageFiles::load_dir(dir).unwrap()
    }

    fn edit(files: &PackageFiles, path: &str, f: impl FnOnce(String) -> String) -> PackageFiles {
        let mut out = files.clone();
        let text = String::from_utf8(files.get(path).unwrap().to_vec()).unwrap();
        out.insert(path, f(text).into_bytes()).unwrap();
        out
    }

    #[test]
    fn a_comment_keeps_the_gameplay_hash_and_changes_the_content_hash() {
        let base = golden();
        let (g0, c0) = hashes(&base, false).unwrap();
        let commented = edit(&base, "rooms/title.toml", |t| {
            format!("# a note for the next editor\n{t}")
        });
        let (g1, c1) = hashes(&commented, false).unwrap();
        assert_eq!(g0, g1, "a comment is not gameplay");
        assert_ne!(c0, c1, "but it is a different file");
    }

    #[test]
    fn music_keeps_the_gameplay_hash() {
        let base = golden();
        let (g0, c0) = hashes(&base, false).unwrap();
        let mut with_music = base.clone();
        with_music
            .insert("music/theme.ogg", b"OggS not really audio".to_vec())
            .unwrap();
        let (g1, c1) = hashes(&with_music, false).unwrap();
        assert_eq!(g0, g1);
        assert_ne!(c0, c1);
    }

    #[test]
    fn a_gameplay_change_changes_both_hashes() {
        let base = golden();
        let (g0, c0) = hashes(&base, false).unwrap();
        let wider = edit(&base, "rooms/title.toml", |t| t.replace("800", "801"));
        let (g1, c1) = hashes(&wider, false).unwrap();
        assert_ne!(g0, g1, "the room size is gameplay");
        assert_ne!(c0, c1);
    }

    #[test]
    fn line_ends_do_not_change_the_content_hash_and_tests_are_not_shipped() {
        let base = golden();
        let crlf = edit(&base, "rooms/title.toml", |t| t.replace('\n', "\r\n"));
        assert_eq!(base.content_hash(), crlf.content_hash());
        let mut with_tests = base.clone();
        with_tests
            .insert("tests/trace-map.toml", b"x = 1\n".to_vec())
            .unwrap();
        assert_eq!(
            base.content_hash(),
            with_tests.content_hash(),
            "tests/ is not shipped"
        );
    }

    #[test]
    fn a_broken_package_has_no_hashes() {
        let broken = edit(&golden(), "rooms/title.toml", |_| "size = [0, 0]\n".to_owned());
        assert!(hashes(&broken, false).is_err());
    }
}
