//! Deterministic simulation for IWBTG Collection.
//!
//! No Bevy, no IO, no threads, no clock. The same package and inputs give bit-identical state on
//! Windows, WASM and Linux. See `docs/architecture.md` and `docs/plans/sim-core.md`.
//!
//! Module owners after WP0 are listed in each module's docs and in plan section 13. Only WP0
//! edits this file.
#![forbid(unsafe_code)]
#![deny(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::float_cmp,
    clippy::cast_possible_truncation
)]

// Existing modules.
pub mod input;
pub mod num;
pub mod replay;
pub mod trace;

// Contracts (WP0) and their owners' modules (plan 13).
pub mod alarm;
pub mod anim;
pub mod builtins;
pub mod collide;
pub mod def;
pub mod error;
pub mod events;
pub mod hash;
pub mod input_state;
pub mod kid;
pub mod motion;
pub mod output;
pub mod path;
pub mod probe;
pub mod restart;
pub mod rng;
pub mod room;
pub mod rules;
pub mod run_globals;
pub mod save;
pub mod script;
#[cfg(feature = "trace-setup")]
pub mod setup;
pub mod sim;
pub mod sound;
pub mod tick;
pub mod timeline;
pub mod timing;
pub mod view;
pub mod world;

pub use def::GameDef;
pub use error::SimError;
pub use room::StartPoint;
pub use rules::{Runner, RunnerRules};
pub use sim::{Sim, SimOptions, StepResult};
pub use world::{Ctx, SimStatus, World};

/// Changes whenever a sim change could alter any outcome, including any Rhai upgrade
/// (architecture "Versions").
pub const SIM_VERSION: u32 = 0;
