//! Sound entries to `SoundDef` (kind, length in ticks; docs/plans/sim-core.md 7, 8.3). WP0
//! creates the file; WP7b owns it.
#![allow(
    unused_variables,
    clippy::ptr_arg,
    reason = "WP0 stub: WP7b fills the body and pushes diagnostics"
)]

use iwbtg_sim::def::SoundDef;

use crate::diag::Diagnostic;
use crate::files::PackageFiles;

/// Reads every sound entry (step 7). The result's index is the `SoundId`;
/// each def carries its package name. `load::load_game` (WP7a) calls this; WP7b owns the body.
pub fn load(files: &PackageFiles, diags: &mut Vec<Diagnostic>) -> Vec<SoundDef> {
    unimplemented!("WP7b: sounds::load")
}
