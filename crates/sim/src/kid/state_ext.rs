//! Families A2-A5 and C kid state (ladders, ice, slopes, flips, triple jump). WP0 declares it
//! empty; WP5c owns and adds the fields.

use serde::{Deserialize, Serialize};

/// Families A2-A5 and C fields of a kid.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidStateExt {}

/// Families A2-A5 and C kid globals that outlive the kid: the game-side latched vine-key flags
/// (wanna, rori, fairy-tale, showdown; I6).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidGlobalsExt {}
