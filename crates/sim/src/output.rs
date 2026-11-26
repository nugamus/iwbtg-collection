//! Events the sim sends to the client (docs/architecture/sim/world.md).
//!
//! Audio and visual effects only: the sim never reads anything back. Not hashed. Each owner
//! defines its own part in its own module; this enum only routes them.

use serde::{Deserialize, Serialize};

use crate::{builtins, kid, room, script, sound};

/// One output event for the client.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum OutputEvent {
    /// Room changes and transitions (`room::RoomOutput`).
    Room(room::RoomOutput),
    /// Kid effects: death, blood (`kid::KidOutput`).
    Kid(kid::KidOutput),
    /// Built-in effects (`builtins::BuiltinOutput`).
    Builtin(builtins::BuiltinOutput),
    /// Sound play, stop, volume, pitch (`sound::SoundOutput`).
    Sound(sound::SoundOutput),
    /// Script effects (`script::ScriptOutput`).
    Script(script::ScriptOutput),
}
