//! Deterministic simulation for IWBTG Collection.
//!
//! No Bevy, no IO, no threads, no clock. The same package and inputs give bit-identical state on
//! Windows, WASM and Linux. See `docs/architecture/sim.md` and the open-question registers in
//! `docs/plans/sim-core/`.
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
pub mod mods;
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
pub mod particles;
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
pub mod stripped;
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

/// Changes whenever a sim change could alter any outcome, including any Rhai upgrade and any
/// state-hash encoding change (`HASH_LAYOUT`) (architecture "Versions").
///
/// 1: the first sim. 2: `HASH_LAYOUT` 3, the named RNG streams added to the hashed state
/// (#1157/#1292, bumped by #1303).
pub const SIM_VERSION: u32 = 2;

/// The freeze point (docs/architecture/versioning.md): the first `SIM_VERSION` whose outcomes are
/// kept. `None` until the freeze, and the bump check (`tests/sim_version_bump.rs`) is inert. Once
/// set, a change to any pinned golden hash without a `SIM_VERSION` bump fails that test and
/// `cargo xtask repin`.
pub const SIM_VERSION_FREEZE: Option<u32> = None;
