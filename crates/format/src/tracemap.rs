//! `tests/trace-map.toml`: original objects to our kinds, for trace runs (docs/plans/sim-core.md
//! 8.1, 9). Used by tools only. WP0 declares; WP7a owns.
#![allow(unused_variables, reason = "WP0 stub: WP7a fills the body")]

use crate::diag::Diagnostic;

/// The parsed trace map. WP7a adds the fields.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TraceMap {}

/// Parses `tests/trace-map.toml`.
pub fn load_trace_map(text: &str) -> Result<TraceMap, Diagnostic> {
    unimplemented!("WP7a: tracemap::load_trace_map")
}
