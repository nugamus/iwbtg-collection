//! The state hash (docs/plans/sim-core.md 10.1).
//!
//! BLAKE3 over the postcard encoding of a canonical `HashView` borrowed from the `World`: every
//! hashed field in a fixed order, `setup_applied` included, `output` and the grid excluded.
//! Postcard writes lengths as varints, so wasm32 and x86_64 give the same bytes. `std::hash`
//! and its hashers are never used here. WP0 declares; WP1 owns.
#![allow(unused_variables, reason = "WP0 stub: WP1 fills the body")]

use crate::world::World;

/// The 32-byte state hash. Replay checkpoints store the first 16 bytes.
pub fn state_hash(w: &World) -> [u8; 32] {
    unimplemented!("WP1: hash::state_hash")
}
