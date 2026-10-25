//! Family presets: fill a draft profile's defaults from its `preset` (docs/plans/sim-core.md
//! 5.1). WP0 declares; WP5a owns (WP5b and WP5c ask WP5a for their family's defaults).
#![allow(unused_variables, reason = "WP0 stub: WP5a fills the body")]

use crate::def::{PhysicsProfile, PhysicsProfileDraft};

/// The full profile: the draft's values over its family's defaults.
pub fn apply(p: &mut PhysicsProfileDraft) -> PhysicsProfile {
    unimplemented!("WP5a: kid::presets::apply")
}
