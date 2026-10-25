//! `iwbtg trace run`, `diff`, `check` (docs/plans/sim-core.md 9.4). WP0 declares; WP8a owns.
#![allow(unused_variables, reason = "WP0 stub: WP8a fills the body")]

use std::path::Path;

use iwbtg_format::PackageFiles;
use iwbtg_sim::trace::TraceTable;

use crate::case::TraceCase;

/// Why a tools command failed.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Anything, with a message for the user.
    #[error("{0}")]
    Message(String),
}

/// Runs one case against its package and returns our trace table.
pub fn run_case(pkg: &PackageFiles, case: &TraceCase, research: &Path) -> Result<TraceTable, ToolError> {
    unimplemented!("WP8a: trace_cmd::run_case")
}
