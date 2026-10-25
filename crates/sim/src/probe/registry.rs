//! GM variable names to probe fields (docs/plans/sim-core.md 9.3): `djump`, `curJumps`,
//! `jump[1]`, `alarm[0]`, `image_index`, ... WP0 declares; WP8a owns.
#![allow(unused_variables, reason = "WP0 stub: WP8a fills the body")]

use super::FieldRef;
use crate::def::GameDef;

/// The field a GM variable name refers to; `None` when unknown.
pub fn resolve(name: &str, def: &GameDef) -> Option<FieldRef> {
    unimplemented!("WP8a: probe::registry::resolve")
}
