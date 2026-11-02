//! The state hash (docs/plans/sim-core.md 10.1).
//!
//! BLAKE3 over the postcard encoding of a canonical [`HashView`] borrowed from the `World`: every
//! hashed field in a fixed order, `setup_applied` included, `output`, the grid and the per-tick
//! `room_requests` counter excluded. Postcard writes lengths as varints and reals as their
//! little-endian bits, so wasm32 and x86_64 give the same bytes. `std::hash` and its hashers are
//! never used here. WP0 declared; WP1 owns.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::def::{RoomId, VarId};
use crate::input_state::InputState;
use crate::num::Real;
use crate::rng::Rng;
use crate::room::RoomRequest;
use crate::save::SaveSlot;
use crate::sound::SoundState;
use crate::timing::TimingState;
use crate::view::View;
use crate::world::{EngineGlobals, Instance, SimStatus, Value, World};

/// Version of the [`HashView`] layout. Part of the hashed bytes, so a layout change can never
/// match an old hash by accident.
pub const HASH_LAYOUT: u32 = 1;

/// Replay checkpoints are taken every this many ticks (architecture "Replays").
pub const CHECKPOINT_INTERVAL: u64 = 500;

/// Every hashed part of a `World`, in a fixed order.
#[derive(Serialize)]
pub struct HashView<'a> {
    layout: u32,
    tick: u64,
    room: RoomId,
    room_tick: u64,
    room_speed: Real,
    instances: &'a [Instance],
    next_id: u64,
    var_names: &'a BTreeMap<String, VarId>,
    globals: &'a BTreeMap<VarId, Value>,
    engine: &'a EngineGlobals,
    rng: &'a Rng,
    input: &'a InputState,
    views: &'a [View; 8],
    pending: &'a Option<RoomRequest>,
    persistent_rooms: &'a BTreeMap<RoomId, Vec<Instance>>,
    save: &'a Option<SaveSlot>,
    sound: &'a SoundState,
    timing: &'a TimingState,
    status: &'a SimStatus,
    setup_applied: bool,
}

impl<'a> HashView<'a> {
    /// The view of `w`.
    pub fn of(w: &'a World) -> HashView<'a> {
        HashView {
            layout: HASH_LAYOUT,
            tick: w.tick,
            room: w.room,
            room_tick: w.room_tick,
            room_speed: w.room_speed,
            instances: w.instances(),
            next_id: w.next_id,
            var_names: &w.var_names,
            globals: &w.globals,
            engine: &w.engine,
            rng: &w.rng,
            input: &w.input,
            views: &w.views,
            pending: &w.pending,
            persistent_rooms: &w.persistent_rooms,
            save: &w.save,
            sound: &w.sound,
            timing: &w.timing,
            status: &w.status,
            setup_applied: w.setup_applied,
        }
    }
}

/// A postcard flavor that feeds BLAKE3 instead of a buffer.
struct Blake3Flavor(blake3::Hasher);

impl postcard::ser_flavors::Flavor for Blake3Flavor {
    type Output = blake3::Hasher;

    fn try_push(&mut self, data: u8) -> postcard::Result<()> {
        self.0.update(&[data]);
        Ok(())
    }

    fn try_extend(&mut self, data: &[u8]) -> postcard::Result<()> {
        self.0.update(data);
        Ok(())
    }

    fn finalize(self) -> postcard::Result<blake3::Hasher> {
        Ok(self.0)
    }
}

/// The 32-byte state hash. Replay checkpoints store the first 16 bytes.
///
/// Serializing the view cannot fail for these types; if it ever did, the hash covers a fixed
/// marker and the error text instead, so the result is still deterministic and never equals a
/// real state's hash.
pub fn state_hash(w: &World) -> [u8; 32] {
    match postcard::serialize_with_flavor(&HashView::of(w), Blake3Flavor(blake3::Hasher::new())) {
        Ok(h) => *h.finalize().as_bytes(),
        Err(e) => {
            let mut h = blake3::Hasher::new();
            h.update(b"iwbtg-sim state hash: serialization failed: ");
            h.update(e.to_string().as_bytes());
            *h.finalize().as_bytes()
        }
    }
}

/// The replay checkpoint at this tick: the first 16 bytes of [`state_hash`] on ticks that are
/// a multiple of [`CHECKPOINT_INTERVAL`] (tick 0 included), else `None` (tick row 16).
pub fn checkpoint(w: &World) -> Option<[u8; 16]> {
    if !w.tick.is_multiple_of(CHECKPOINT_INTERVAL) {
        return None;
    }
    let full = state_hash(w);
    let mut out = [0u8; 16];
    out.copy_from_slice(full.get(..16)?);
    Some(out)
}
