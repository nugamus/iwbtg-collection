//! Variable name interning (docs/plans/sim-core.md 1.1, 8.2 step 6). Ids come from the sorted
//! name list, so they never depend on run or file order. WP0 declares; WP7a owns.
#![allow(unused_variables, reason = "WP0 stub: WP7a fills the body")]

use std::collections::BTreeMap;

use iwbtg_sim::def::VarId;

/// Every variable name known at load, with its id.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct VarTable {
    /// Name to id, ids assigned in sorted name order.
    pub ids: BTreeMap<String, VarId>,
}

/// Interns names: sorts and dedups them, then numbers them from 0.
pub fn intern_vars(names: impl Iterator<Item = String>) -> VarTable {
    unimplemented!("WP7a: vars::intern_vars")
}
