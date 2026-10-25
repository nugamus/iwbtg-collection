//! The kid built-in (docs/plans/sim-core.md 5).
//!
//! The kid's events run ordered lists of named actions read from `physics.toml`; every spec rule
//! is a field value or a named action in `actions/`. Nothing family-specific hides in an
//! `if family == B`. WP0 declares the module tree and the cross-WP signatures; WP5a owns this
//! file (WP5b and WP5c own their files under `actions/`, `collisions/` and `state_*.rs`).
#![allow(unused_variables, reason = "WP0 stubs: WP5a fills the bodies")]

pub mod api;
pub mod death;
pub mod presets;
pub mod state;
pub mod state_b;
pub mod state_ext;

/// Named kid actions, one file per family or topic.
pub mod actions {
    pub mod a1;
    pub mod a2;
    pub mod a3;
    pub mod a4;
    pub mod a5;
    pub mod b;
    pub mod c;
    pub mod common;
    pub mod flip;
    pub mod ice;
    pub mod ladder;
    pub mod slope;
}

/// Named collision runs (`events.collisions` entries).
pub mod collisions {
    pub mod b;
    pub mod block;
    pub mod moving_block;
    pub mod platform;
    pub mod slope;
}

use serde::{Deserialize, Serialize};

pub use state::{KidGlobals, KidState};

use crate::def::PhysicsProfile;
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

/// Kid effects for the client (death, blood). WP5a adds the variants.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidOutput {}

/// Runs one event on a kid instance: the profile's action list for that event.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow {
    unimplemented!("WP5a: kid::on_event")
}

/// What a kid kind with this profile handles.
pub fn event_set(p: &PhysicsProfile) -> EventSet {
    unimplemented!("WP5a: kid::event_set")
}
