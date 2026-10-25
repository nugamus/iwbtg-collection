//! Family B kid state (kid/B.md: `cur_jumps` counter, yflag, reverse). WP0 declares it empty;
//! WP5b owns and adds the fields.

use serde::{Deserialize, Serialize};

/// Family B fields of a kid.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidStateB {}
