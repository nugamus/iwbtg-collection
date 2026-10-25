//! Timeline files to `TimelineDef` (docs/plans/sim-core.md 8.2 step 7). WP0 creates the file;
//! WP7b owns it.
#![allow(
    unused_variables,
    clippy::ptr_arg,
    reason = "WP0 stub: WP7b fills the body and pushes diagnostics"
)]

use iwbtg_sim::def::TimelineDef;

use crate::diag::Diagnostic;
use crate::files::PackageFiles;

/// Reads every timeline file (step 7). The result's index is the `TimelineId`;
/// each def carries its package name. `load::load_game` (WP7a) calls this; WP7b owns the body.
pub fn load(files: &PackageFiles, diags: &mut Vec<Diagnostic>) -> Vec<TimelineDef> {
    unimplemented!("WP7b: timelines::load")
}
