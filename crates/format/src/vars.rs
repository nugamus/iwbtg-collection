//! Variable name interning (docs/plans/sim-core.md 1.1, 8.2 step 6). Ids come from the sorted
//! name list, so they never depend on run or file order (Design, determinism).
//!
//! The loader interns every name it can see: object and instance params (instance creation code
//! as data) and the globals that room creation actions set. Names first seen at run time
//! (scripts) are interned by the sim in `World::var_names`.

use std::collections::{BTreeMap, BTreeSet};

use iwbtg_sim::def::VarId;

/// Every variable name known at load, with its id.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct VarTable {
    /// Name to id, ids assigned in sorted name order.
    pub ids: BTreeMap<String, VarId>,
}

impl VarTable {
    /// The id of a name, if it was interned.
    pub fn get(&self, name: &str) -> Option<VarId> {
        self.ids.get(name).copied()
    }
}

/// Interns names: sorts and dedups them, then numbers them from 0.
pub fn intern_vars(names: impl Iterator<Item = String>) -> VarTable {
    let sorted: BTreeSet<String> = names.collect();
    let ids = sorted
        .into_iter()
        .zip(0u32..)
        .map(|(name, id)| (name, VarId(id)))
        .collect();
    VarTable { ids }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_follow_sorted_order_whatever_the_input_order() {
        let a = intern_vars(["roomTo", "trg", "dir", "trg"].into_iter().map(String::from));
        let b = intern_vars(["trg", "dir", "roomTo"].into_iter().map(String::from));
        assert_eq!(a, b);
        assert_eq!(a.get("dir"), Some(VarId(0)));
        assert_eq!(a.get("roomTo"), Some(VarId(1)));
        assert_eq!(a.get("trg"), Some(VarId(2)));
        assert_eq!(a.get("x"), None);
    }
}
