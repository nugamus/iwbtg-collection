//! Param validation against `builtins::param_specs` (docs/plans/sim-core.md 8.2 step 7).
//! WP0 creates the file; WP7b owns it.
#![allow(
    unused_variables,
    clippy::ptr_arg,
    reason = "WP0 stub: WP7b fills the body and pushes diagnostics"
)]

use iwbtg_sim::def::GameDef;

use crate::diag::Diagnostic;

/// Checks every kind's and placed instance's params against `builtins::param_specs` of its
/// base (step 7), after `load::load_game` (WP7a) has built the definition. Problems are
/// diagnostics. WP7b owns the body.
pub fn check(def: &GameDef, diags: &mut Vec<Diagnostic>) {
    unimplemented!("WP7b: params::check")
}
