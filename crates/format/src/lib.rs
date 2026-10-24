//! Game package reading and validation (docs/format/v0.md).
//!
//! A package is read into [`PackageFiles`] (path to bytes), so the client, the server and WASM
//! share one code path. [`check`] validates it and returns readable [`Diagnostic`]s with file,
//! line and column.
#![forbid(unsafe_code)]

pub mod check;
pub mod diag;
pub mod files;
pub mod ids;
pub mod manifest;
pub mod number;

pub use check::check;
pub use diag::{Diagnostic, Severity, has_errors};
pub use files::{LoadError, PackageFiles};
pub use manifest::{FORMAT_VERSION, Manifest};
