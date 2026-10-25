//! `object_order` to dispatch ranks (docs/plans/sim-core.md 1.3, 8.2 step 5, CR4). WP0
//! declares; WP7a owns.
#![allow(unused_variables, reason = "WP0 stub: WP7a fills the body")]

use iwbtg_sim::def::{KindDef, RankTable, VariantSel};

use crate::diag::Diagnostic;

/// One `object_order` entry: a kind name, maybe with a variant selector.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct OrderEntry {
    /// Kind name (`block`, `@world`).
    pub kind: String,
    /// Selector (`{ kind = "spike", dir = "up" }`).
    pub variant: Option<VariantSel>,
}

/// Ranks for every kind; every kind placed or spawnable must get one.
pub fn resolve_ranks(order: &[OrderEntry], kinds: &[KindDef]) -> Result<RankTable, Diagnostic> {
    unimplemented!("WP7a: order::resolve_ranks")
}
