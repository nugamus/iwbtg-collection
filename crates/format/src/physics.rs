//! `physics.toml` to `PhysicsProfileDraft`, then `kid::presets::apply`; legacy drafts map to
//! their family preset with a warning (docs/plans/sim-core.md 8.2 step 7). WP0 creates the file;
//! WP7b owns it.
#![allow(
    unused_variables,
    clippy::ptr_arg,
    reason = "WP0 stub: WP7b fills the body and pushes diagnostics"
)]

use iwbtg_sim::def::PhysicsProfile;

use crate::diag::Diagnostic;
use crate::files::PackageFiles;

/// Reads `physics.toml` (step 7). `None` when it cannot be read; every problem is a diagnostic,
/// and a legacy draft maps to its family preset with a warning. `load::load_game` (WP7a) calls
/// this; WP7b owns the body.
pub fn load(files: &PackageFiles, diags: &mut Vec<Diagnostic>) -> Option<PhysicsProfile> {
    unimplemented!("WP7b: physics::load")
}
