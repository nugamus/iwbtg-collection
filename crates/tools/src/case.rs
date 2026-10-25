//! Trace case files `crates/tools/cases/<package-id>/<case>.toml` (docs/plans/sim-core.md 9.2).
//! WP0 creates the file; WP8a owns it.

/// One trace case: source trace, start point, first frame, traced object, DLL mode, setup,
/// variable map, ignored columns. WP8a adds the fields.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TraceCase {}
