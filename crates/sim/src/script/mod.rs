//! The script host (docs/plans/sim-core.md 11, decision 0001).
//!
//! The sim talks to scripts only through [`ScriptHost`], implemented by [`ScriptRuntime`], so
//! the sim core never names Rhai types. `Sim` owns one `ScriptRuntime`, built from
//! `GameDef::scripts`, and lends it to every [`Ctx`] as `ctx.scripts`. WP0 declares; WP9 owns.

pub mod api;
pub mod budget;
mod consts;
pub mod docs;
pub mod host;
pub mod inst;
pub mod validate;

use serde::{Deserialize, Serialize};

pub use host::ScriptRuntime;

use crate::error::SimError;
use crate::events::EventKind;
use crate::world::{Ctx, Flow, InstId};

/// A script error, turned into `SimError::Script` by the caller.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
#[error("{message}")]
pub struct ScriptError {
    /// What went wrong, with the script position when known.
    pub message: String,
}

impl From<ScriptError> for SimError {
    fn from(e: ScriptError) -> SimError {
        SimError::Script { message: e.message }
    }
}

/// Something that runs script handlers.
///
/// Takes `&self`: a handler that spawns runs the new instance's Create inside `Ctx::spawn`,
/// which may call back into the host, so the host must be re-entrant. Per-call state (scopes,
/// the step budget counter) lives inside the host (interior mutability is allowed there and
/// only there, architecture "Scripts").
pub trait ScriptHost {
    /// Runs the script handler for `ev` on `me`, if the script has one.
    fn on_event(&self, ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, ScriptError>;
}

/// A scripted instance's state. WP9 adds the fields.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ScriptState {}

/// Script effects for the client.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ScriptOutput {
    /// `emit(name)`: a named signal for the client (a sound cue, a UI flash).
    Emit {
        /// The signal name, at most 64 bytes.
        name: String,
    },
    /// `shake(ticks, strength)`: shake the screen.
    Shake {
        /// How many ticks.
        ticks: u32,
        /// Pixels of shake.
        strength: crate::num::Real,
    },
}

/// Runs the script part of an event, after the base built-in's handler, through `ctx.scripts`.
/// An error halts the sim (through `events::dispatch`).
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    match ctx.scripts.clone().on_event(ctx, me, ev) {
        Ok(f) => Ok(f),
        Err(e) => Err(e.into()),
    }
}
