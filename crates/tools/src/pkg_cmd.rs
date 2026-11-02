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
