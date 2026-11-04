//! Game package reading and validation (docs/format/v0.md).
//!
//! A package is read into [`PackageFiles`] (path to bytes), so the client, the server and WASM
//! share one code path. [`check()`] validates it and returns readable [`Diagnostic`]s with file,
//! line and column. [`load_game`] turns it into the `GameDef` the sim runs
//! (docs/plans/sim-core.md 8): WP7a owns `load`, `sprites`, `masks`, `rooms`, `objects`,
//! `order`, `vars` and `tracemap`; WP7b owns `physics`, `params`, `timelines`, `paths` and
//! `sounds`.
#![forbid(unsafe_code)]

pub mod check;
pub mod diag;
pub mod engine_pack;
pub mod files;
pub mod fmt;
pub mod ids;
pub mod key_art;
pub mod manifest;
pub mod number;
#[cfg(feature = "schema")]
pub mod schema;

// Loader (docs/plans/sim-core.md 8.1).
pub mod load;
pub mod masks;
pub mod objects;
pub mod order;
pub mod params;
pub mod paths;
pub mod physics;
pub mod rooms;
pub mod sounds;
pub mod sprites;
pub mod timelines;
pub mod tracemap;
pub mod vars;

pub use check::check;
pub use diag::{Diagnostic, Severity, has_errors};
pub use files::{LoadError, PackageFiles};
pub use load::{LoadMode, load_game};
pub use manifest::{FORMAT_VERSION, Manifest};
