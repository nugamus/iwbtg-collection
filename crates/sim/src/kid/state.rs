//! Kid state (docs/plans/sim-core.md 5.1).
//!
//! WP5a owns this file: the core fields every family uses. Family B fields live in `KidStateB`
//! (WP5b), the rest in `KidStateExt` (WP5c), so no WP edits another's struct. Fields are plain
//! data; the actions (`kid/actions/`) and `kid::api` are the only code that reads or writes
//! them, and other WPs go through `kid::api` (plan 1.6).
//!
//! The image alpha, scales and sprite are the instance's own `Image`, not kid fields: the fade-in
//! (A1 Step 7) and the bow snapshot read `Instance::img`. Numbers such as `run_speed` are copied
//! from the profile by `create_defaults` because kid parameter zones change them at run time
//! (mechanics `builtin.kid_param_zone`).

use serde::{Deserialize, Serialize};

use super::state_b::{KidGlobalsB, KidStateB};
use super::state_ext::{KidGlobalsExt, KidStateExt};
use crate::num::Real;

/// One kid instance's state. Names follow the spec's GM variables in the doc comments.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct KidState {
    /// `djump`: a double jump is available (A1 Create sets it true, landing and refreshers too).
    pub djump: bool,
    /// `curJumps` (family B jump counter; 1 means the next air jump is `jump[1]`).
    pub cur_jumps: u8,
    /// `tjump`: the third (triple) jump is available (showdown).
    pub tjump: bool,
    /// `onPlatform`. Not set in A1 Create: it reads as 0.
    pub on_platform: bool,
    /// `ladder`: the kid is on a ladder.
    pub ladder: bool,
    /// `ladderud`: the kid moved on a ladder this Step.
    pub ladder_ud: bool,
    /// `ladderjump`: the kid left a ladder by jumping this Step.
    pub ladder_jump: bool,
    /// `frozen`: input blocked (per-kid in A1).
    pub frozen: bool,
    /// `frozen2`: the movement action is skipped (per-kid in A1).
    pub frozen2: bool,
    /// `yflag`: 1 normal, -1 flipped (family B and later).
    pub yflag: i8,
    /// The kid is the flipped variant (`player2`): gravity inverted, mask flipped.
    pub flipped: bool,
    /// Facing, -1 or 1 (`image_xscale`'s sign).
    pub facing: i8,
    /// `tempHspeed`: showdown ice carry-over speed.
    pub temp_hspeed: Real,
    /// `maxSpeed`: the run speed.
    pub run_speed: Real,
    /// `jump`, `jump2`, `jump3`: ground jump speed then air jumps in order (positive magnitudes
    /// as the profile lists them; the sign comes from gravity and flip).
    pub jump_speeds: Vec<Real>,
    /// `maxVspeed`: the speed clamp.
    pub max_vspeed: Real,
    /// `maxFallSpeed`: read only by `fall_cap = "shift_then_clamp"`.
    pub max_fall_speed: Real,
    /// `muteki`: killing is ignored while set (A1 Death step 2).
    pub invincible: bool,
    /// `scale`: machinary's layer scale (1 everywhere else).
    pub scale: Real,
    /// The previous tick's slope-contact flag (slope families).
    pub on_slope_last: bool,
    /// Family B fields (WP5b).
    pub b: KidStateB,
    /// Families A2-A5 and C fields (WP5c).
    pub ext: KidStateExt,
}

impl Default for KidState {
    /// A kid before `create_defaults` ran: facing right, normal gravity side, scale 1, every
    /// number 0 and no jump speeds. `create_defaults` copies the profile's numbers in.
    fn default() -> KidState {
        KidState {
            djump: false,
            cur_jumps: 0,
            tjump: false,
            on_platform: false,
            ladder: false,
            ladder_ud: false,
            ladder_jump: false,
            frozen: false,
            frozen2: false,
            yflag: 1,
            flipped: false,
            facing: 1,
            temp_hspeed: Real::ZERO,
            run_speed: Real::ZERO,
            jump_speeds: Vec::new(),
            max_vspeed: Real::ZERO,
            max_fall_speed: Real::ZERO,
            invincible: false,
            scale: Real::ONE,
            on_slope_last: false,
            b: KidStateB::default(),
            ext: KidStateExt::default(),
        }
    }
}

/// Kid globals that persist while no kid exists (A3 Inferred, I6), split like [`KidState`]:
/// the core fields are here; B's `reverse` lives in `b` (WP5b); the latched vine-key flags of
/// wanna, rori, fairy-tale and showdown live in `ext` (WP5c).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KidGlobals {
    /// `global.grav`: the next kid is the flipped one (A1 Step 5 swap, saved in byte 9).
    pub grav: bool,
    /// `global.image_xscale`: the facing handed to the next kid, -1 or 0 (A1 Create step 2;
    /// a save load sets 0, so a loaded kid faces right).
    pub image_xscale: i8,
    /// `global.frozen`: input blocked (family B and later).
    pub frozen: bool,
    /// `global.frozen2`: the movement action is skipped (family B and later).
    pub frozen2: bool,
    /// Family B globals (WP5b).
    pub b: KidGlobalsB,
    /// Families A2-A5 and C globals (WP5c).
    pub ext: KidGlobalsExt,
}

impl Default for KidGlobals {
    /// Normal gravity, no stored facing, nothing frozen.
    fn default() -> KidGlobals {
        KidGlobals {
            grav: false,
            image_xscale: 0,
            frozen: false,
            frozen2: false,
            b: KidGlobalsB::default(),
            ext: KidGlobalsExt::default(),
        }
    }
}
