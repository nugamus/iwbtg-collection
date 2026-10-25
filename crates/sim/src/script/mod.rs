//! The script host (docs/plans/sim-core.md 11, decision 0001).
//!
//! The sim talks to scripts only through [`ScriptHost`], so the sim core never names Rhai
//! types. WP0 declares; WP9 owns.
#![allow(unused_variables, reason = "WP0 stub: WP9 fills the body")]

pub mod api;
pub mod budget;
pub mod host;
pub mod validate;

use serde::{Deserialize, Serialize};

use crate::events::EventKind;
use crate::world::{Ctx, Flow, InstId};

/// A script error, turned into `SimError::Script` by the caller.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
#[error("{message}")]
pub struct ScriptError {
    /// What went wrong, with the script position when known.
    pub message: String,
}

/// Something that runs script handlers.
pub trait ScriptHost {
    /// Runs the script handler for `ev` on `me`, if the script has one.
    fn on_event(&mut self, ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, ScriptError>;
}

/// A scripted instance's state. WP9 adds the fields.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ScriptState {}

/// Script effects for the client. WP9 adds the variants.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ScriptOutput {}

/// Runs the script part of an event, after the base built-in's handler.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow {
    unimplemented!("WP9: script::on_event")
}
