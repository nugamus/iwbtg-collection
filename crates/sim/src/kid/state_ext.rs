//! Families A2-A5 and C kid state (ladders, ice, slopes, flips, triple jump). WP0 declares it
//! empty; WP5c owns and adds the fields.

use serde::{Deserialize, Serialize};

/// Families A2-A5 and C fields of a kid.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidStateExt {
    /// `onVineL` (family C): a left vine within 1 px and free space 1 px in the gravity
    /// direction, computed by `vine_probe` before the jump code.
    pub on_vine_l: bool,
    /// `onVineR` (family C).
    pub on_vine_r: bool,
}

/// Families A2-A5 and C kid globals that outlive the kid: the game-side latched vine-key flags
/// (wanna, rori, fairy-tale, showdown; I6).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KidGlobalsExt {
    /// `button01`: Left, 1 from its press tick to its release tick (`input.vine_keys =
    /// "latched_flags"`, action `key_flags`).
    pub key_left: bool,
    /// `button02`: Right.
    pub key_right: bool,
    /// `button03`: Jump.
    pub key_jump: bool,
}
