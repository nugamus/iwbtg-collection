//! Package to `GameDef` (docs/plans/sim-core.md 8.1, 8.2). WP0 declares; WP7a owns (WP7b adds
//! the physics and params steps).
#![allow(unused_variables, reason = "WP0 stub: WP7a fills the body")]

use iwbtg_sim::def::GameDef;

use crate::diag::Diagnostic;
use crate::files::PackageFiles;

/// How strict loading is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadMode {
    /// Every kind must be mapped; any error fails the load.
    Strict,
    /// Trace runs on converter output.
    Trace {
        /// Unmapped script kinds load as inert (no events, mask still collides) with a warning.
        unmapped_kinds_inert: bool,
    },
}

/// Parses and checks a package into the definition the sim runs.
pub fn load_game(files: &PackageFiles, mode: LoadMode) -> Result<GameDef, Vec<Diagnostic>> {
    unimplemented!("WP7a: load::load_game")
}
