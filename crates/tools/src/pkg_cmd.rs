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

/// `iwbtg pkg fmt <pkg> [--check]`: formats every `.toml` file of the package (`tests/` included:
/// its files are data too) in the canonical layout, writing the ones that change. With `check`
/// nothing is written, the files that would change are listed and the exit code is 1.
pub fn fmt_main(dir: &Path, check: bool) -> Result<bool, ToolError> {
    let files = load_package(dir)?;
    let mut changed = Vec::new();
    let mut broken = false;
    for (path, bytes) in files.iter().filter(|(p, _)| p.ends_with(".toml")) {
        let Ok(text) = std::str::from_utf8(bytes) else {
            println!("{path}: not UTF-8");
            broken = true;
            continue;
        };
        match iwbtg_format::fmt::format_toml(text) {
            Ok(out) if out == text => {}
            Ok(out) => {
                if !check {
                    let target = dir.join(path);
                    std::fs::write(&target, out.as_bytes())
                        .map_err(|e| ToolError::Message(format!("{}: {e}", target.display())))?;
                }
                changed.push(path.to_owned());
            }
            Err(e) => {
                println!("{path}: {e}");
                broken = true;
            }
        }
    }
    let verb = if check { "would change" } else { "formatted" };
    for p in &changed {
        println!("{verb}: {p}");
    }
    if changed.is_empty() && !broken {
        println!("no changes");
    } else if !changed.is_empty() {
        println!("{} file(s) {verb}", changed.len());
    }
    Ok(!broken && !(check && !changed.is_empty()))
}

#[cfg(test)]
mod fmt_tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("iwbtg-fmt-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("rooms")).unwrap();
        dir
    }

    #[test]
    fn fmt_writes_then_a_second_run_and_check_find_nothing() {
        let dir = scratch("write");
        std::fs::write(
            dir.join("rooms/a.toml"),
            "size=[800,608]\ninstances=[{x=1,y=2,kind=\"spike\"}]\n",
        )
        .unwrap();
        std::fs::write(dir.join("game.toml"), "id = \"x\"\n").unwrap();
        assert!(!fmt_main(&dir, true).unwrap(), "check reports the file");
        assert_eq!(
            std::fs::read_to_string(dir.join("rooms/a.toml")).unwrap(),
            "size=[800,608]\ninstances=[{x=1,y=2,kind=\"spike\"}]\n",
            "check writes nothing"
        );
        assert!(fmt_main(&dir, false).unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.join("rooms/a.toml")).unwrap(),
            "size = [800, 608]\ninstances = [\n  { kind = \"spike\", x = 1, y = 2 },\n]\n"
        );
        assert!(fmt_main(&dir, false).unwrap(), "idempotent: nothing left to do");
        assert!(fmt_main(&dir, true).unwrap(), "and check agrees");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fmt_fails_on_a_file_that_is_not_toml() {
        let dir = scratch("bad");
        std::fs::write(dir.join("rooms/a.toml"), "a = \n").unwrap();
        assert!(!fmt_main(&dir, false).unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }
}
