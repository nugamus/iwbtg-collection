//! Events the sim sends to the client (docs/plans/sim-core.md 1.5).
//!
//! Audio and visual effects only: the sim never reads anything back. Not hashed. Each owner
//! defines its own part in its own module; this enum only routes them.

use serde::{Deserialize, Serialize};

use crate::{builtins, kid, room, script, sound};

/// One output event for the client.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum OutputEvent {
    /// Room changes and transitions (WP4a, `room::RoomOutput`).
    Room(room::RoomOutput),
    /// Kid effects: death, blood (WP5a, `kid::KidOutput`).
    Kid(kid::KidOutput),
    /// Built-in effects (WP6a, `builtins::BuiltinOutput`).
    Builtin(builtins::BuiltinOutput),
    /// Sound play, stop, volume, pitch (WP10, `sound::SoundOutput`).
    Sound(sound::SoundOutput),
    /// Script effects (WP9, `script::ScriptOutput`).
    Script(script::ScriptOutput),
}
