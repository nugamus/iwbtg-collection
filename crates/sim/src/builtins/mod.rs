//! Built-in kinds other than the kid (docs/plans/sim-core.md 6).
//!
//! One file per built-in, documented in `docs/format/builtins/<kind>.md`. WP0 declares the
//! module tree and the cross-WP signatures; WP6a owns this file and `state.rs` (WP6b and WP6c
//! add one line each there and own their built-ins' files).
#![allow(unused_variables, reason = "WP0 stubs: WP6a fills the bodies")]

pub mod state;

// Tier 1 (WP6a).
pub mod block;
pub mod blood;
pub mod blood_emitter;
pub mod bow;
pub mod camera;
pub mod difficulty_warp;
pub mod game_over;
pub mod kid_bullet;
pub mod kid_start;
pub mod killer;
pub mod platform;
pub mod save;
pub mod spike;
pub mod warp;
pub mod world;

// Tier 2 (WP6b).
pub mod boss_gate;
pub mod boss_item;
pub mod bullet_blocker;
pub mod drop_platform;
pub mod edge_warp;
pub mod fake_block;
pub mod game_clear;
pub mod gravity_flip;
pub mod hidden_block;
pub mod item_viewer;
pub mod moving_platform;
pub mod music;
pub mod playerface;
pub mod secret_item;
pub mod slope;
pub mod trap;
pub mod trigger;
pub mod vine;
pub mod water;

// Tier 3 (WP6c).
pub mod breakable_block;
pub mod conveyor;
pub mod decor;
pub mod jump_refresher;
pub mod kid_param_zone;
pub mod ladder;
pub mod orbit;
pub mod path_platform;
pub mod scripted;
pub mod sign;
pub mod spring;
pub mod time_limit;
pub mod trap_stopper;

use serde::{Deserialize, Serialize};

pub use state::BuiltinState;

use crate::def::{Builtin, KindDef, Params};
use crate::events::{EventKind, EventSet};
use crate::world::{Ctx, Flow, InstId};

/// Mechanism globals: trigger models, boss and item flags. WP6a adds the fields.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct MechGlobals {}

/// Built-in effects for the client. WP6a adds the variants.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct BuiltinOutput {}

/// One parameter a built-in accepts; `crates/format` validates package params against these.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ParamSpec {
    /// Param name in the package.
    pub name: &'static str,
    /// Its type.
    pub ty: ParamType,
    /// Must be given (no default).
    pub required: bool,
}

/// Type of a built-in parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParamType {
    /// A real.
    Real,
    /// A flag.
    Bool,
    /// One of these words.
    Enum(&'static [&'static str]),
    /// A room id.
    Room,
    /// A kind id.
    Kind,
    /// A sound id.
    Sound,
}

/// A param set a built-in cannot use.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
#[error("param {param}: {message}")]
pub struct ParamError {
    /// The param name.
    pub param: String,
    /// What is wrong.
    pub message: String,
}

/// Runs one event on a built-in instance (`match` on [`Builtin`]).
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow {
    unimplemented!("WP6a: builtins::on_event")
}

/// What a kind with this base handles.
pub fn event_set(b: Builtin, kind: &KindDef) -> EventSet {
    unimplemented!("WP6a: builtins::event_set")
}

/// The params a built-in accepts.
pub fn param_specs(b: Builtin) -> &'static [ParamSpec] {
    unimplemented!("WP6a: builtins::param_specs")
}

/// The initial state of a new built-in instance from its params.
pub fn init_state(b: Builtin, params: &Params) -> Result<BuiltinState, ParamError> {
    unimplemented!("WP6a: builtins::init_state")
}
