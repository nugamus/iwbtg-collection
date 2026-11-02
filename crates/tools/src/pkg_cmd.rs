//! `iwbtg pkg check` (docs/plans/sim-core.md 9.4). WP0 creates the file; WP8a owns it.
//!
//! Minimal version written by WP7a so its user check runs before WP8a merges: loads the package
//! with `iwbtg_format::load_report`, prints every diagnostic, then `OK` or the error count.

use std::path::PathBuf;
use std::process::ExitCode;

use iwbtg_format::load::{LoadMode, load_report};
use iwbtg_format::{PackageFiles, Severity};

/// Arguments of `iwbtg pkg check <pkg> [--trace-mode]`.
#[derive(Clone, PartialEq, Eq, Debug, clap::Args)]
pub struct PkgCheckArgs {
    /// Package folder.
    pub pkg: PathBuf,
    /// Accept research converter output: unmapped kinds load inert, converter gaps are warnings.
    #[arg(long)]
    pub trace_mode: bool,
}

/// Runs the check and prints the result.
pub fn main(args: &PkgCheckArgs) -> ExitCode {
    let files = match PackageFiles::load_dir(&args.pkg) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mode = if args.trace_mode {
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
        ExitCode::SUCCESS
    } else {
        println!("FAILED: {errors} errors, {warnings} warnings");
        ExitCode::FAILURE
    }
}
