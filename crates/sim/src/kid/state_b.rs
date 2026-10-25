//! Family B kid state and globals (kid/B.md: `cur_jumps` counter, yflag, reverse). WP0 declares
//! them empty; WP5b owns and adds the fields.

use serde::{Deserialize, Serialize};

/// Family B fields of a kid.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidStateB {}

/// Family B kid globals that outlive the kid (kid/B.md: `reverse`, which the next kid's Create
/// reads stale).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidGlobalsB {}
