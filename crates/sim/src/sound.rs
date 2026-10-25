//! The sound model (docs/plans/sim-core.md 7, register S1-S5).
//!
//! Audio output is the client's job; whether a sound **is playing** is sim state, because
//! `sound_isplaying` changes game state in 15 games. WP0 declares; WP10 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP10 fills the bodies")]

pub mod cleanmem;
pub mod gmfmod;
pub mod supersound;

use serde::{Deserialize, Serialize};

use crate::def::{GameDef, SoundId};
use crate::error::SimError;
use crate::rules::SoundRules;
use crate::world::{Ctx, World};

/// Playing copies and DLL adapter state. Hashed. WP10 adds the fields.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SoundState {}

/// Sound events for the client (play, stop, stop all, volume, pitch). WP10 adds the variants.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SoundOutput {}

/// How DLL sound calls behave.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum DllMode {
    /// Modeled adapters, for play and replays.
    #[default]
    Model,
    /// Every call returns 0 or "", like the recorder's `--stub-dlls`.
    Stub,
}

/// GML `sound_play` / `sound_loop`.
pub fn play(ctx: &mut Ctx, s: SoundId, looping: bool) -> Result<(), SimError> {
    unimplemented!("WP10: sound::play")
}

/// GML `sound_stop`.
pub fn stop(ctx: &mut Ctx, s: SoundId) {
    unimplemented!("WP10: sound::stop")
}

/// GML `sound_stop_all`.
pub fn stop_all(ctx: &mut Ctx) {
    unimplemented!("WP10: sound::stop_all")
}

/// GML `sound_isplaying` (latency per S1).
pub fn is_playing(w: &World, def: &GameDef, s: SoundId, rules: &SoundRules) -> bool {
    unimplemented!("WP10: sound::is_playing")
}

/// Ends non-looping copies whose length in ticks has passed.
pub fn tick(ctx: &mut Ctx) {
    unimplemented!("WP10: sound::tick")
}
