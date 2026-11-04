//! Family B kid state and globals (kid/B.md: `cur_jumps` counter, yflag, reverse). WP0 declares
//! them empty; WP5b owns and adds the fields.

use serde::{Deserialize, Serialize};

/// Family B fields of a kid. `curJumps` itself is `KidState::cur_jumps` (the core counter).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidStateB {
    /// `maxJumps`: the last numbered jump (2); the counter passes it in the water.
    pub max_jumps: u8,
    /// `infJump`: unlimited jumps at `jump[maxJumps]`.
    pub inf_jump: bool,
}

/// Family B kid globals that outlive the kid (kid/B.md: `reverse`, which the next kid's Create
/// reads stale).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidGlobalsB {
    /// `global.reverse`: gravity is flipped. The next kid's Create reads it (a retry's new kid
    /// sees the value from before the save file is read: kid/B.md Restart).
    pub reverse: bool,
}
