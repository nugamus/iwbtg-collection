//! Deterministic simulation for IWBTG Collection.
//!
//! No Bevy, no IO, no threads, no clock. The same package and inputs give bit-identical state on
//! Windows, WASM and Linux. See `docs/architecture.md`.
#![forbid(unsafe_code)]

pub mod input;
pub mod num;
pub mod replay;
pub mod trace;
