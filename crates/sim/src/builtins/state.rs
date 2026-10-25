//! Per-instance built-in state (docs/plans/sim-core.md 1.5).
//!
//! WP6a owns this file. Each built-in with state adds one variant here that wraps a struct from
//! its own `builtins/<kind>.rs`; WP6b and WP6c add one line each, merged in wave order.

use serde::{Deserialize, Serialize};

/// A built-in instance's state.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum BuiltinState {
    /// No per-instance state.
    #[default]
    Stateless,
}
