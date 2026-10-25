//! Trace setup actions, behind the `trace-setup` feature (docs/plans/sim-core.md 9.2, 9.4, 10.4).
//!
//! A fixed vocabulary that replaces the recorder's `--gml` assignments, `--x` and `--y`.
//! Applying one sets `World::setup_applied`, which is hashed, so a run that used setup can never
//! match a clean verification. The server refuses to start when this feature is on. WP0
//! declares; WP8a owns.
#![allow(unused_variables, reason = "WP0 stub: WP8a fills the body")]

use crate::def::VarId;
use crate::error::SimError;
use crate::probe::{FieldRef, ProbeTarget, SaveField};
use crate::sim::Sim;
use crate::world::Value;

/// One setup action.
#[derive(Clone, PartialEq, Debug)]
pub enum SetupAction {
    /// Set a field of an instance (the kid, the first of a kind, a named instance).
    SetField {
        /// Whose field.
        target: ProbeTarget,
        /// Which field.
        field: FieldRef,
        /// The value.
        value: Value,
    },
    /// Set a global.
    SetGlobal {
        /// The global.
        var: VarId,
        /// The value.
        value: Value,
    },
    /// Set a save slot field (creates the slot if missing).
    SetSave {
        /// The field.
        field: SaveField,
        /// The value.
        value: Value,
    },
}

impl Sim {
    /// Applies one setup action and sets `World::setup_applied`.
    pub fn apply_setup(&mut self, s: &SetupAction) -> Result<(), SimError> {
        unimplemented!("WP8a: Sim::apply_setup")
    }
}
