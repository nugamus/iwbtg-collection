//! GM motion variables and automatic motion (docs/plans/sim-core.md 3.1, register M1-M7).
//!
//! WP0 declares the struct, the Inferred defaults and the read accessors; WP2 owns the file and
//! writes the coupling (setting h or v recomputes speed and direction and the reverse) in
//! `recompute_polar` and `recompute_cartesian`. Every setter rejects a non-finite value with
//! `SimError::NonFinite` (N15).
#![allow(unused_variables, reason = "WP0 stubs: WP2 fills the bodies")]

use serde::{Deserialize, Serialize};

use crate::def::GameDef;
use crate::error::SimError;
use crate::num::Real;
use crate::path::PathRun;
use crate::rules::MotionRules;

/// Position, speeds, gravity and friction of one instance.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Motion {
    x: Real,
    y: Real,
    xstart: Real,
    ystart: Real,
    xprevious: Real,
    yprevious: Real,
    hspeed: Real,
    vspeed: Real,
    speed: Real,
    direction: Real,
    gravity: Real,
    gravity_direction: Real,
    friction: Real,
}

/// Events automatic motion raised.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MotionEvents {
    /// The path ended this tick (End of Path event).
    pub end_of_path: bool,
}

impl Motion {
    /// At `(x, y)`, also the start and previous position; speed 0, direction 0, gravity 0,
    /// gravity_direction 270, friction 0 (Inferred defaults).
    pub fn new_at(x: Real, y: Real) -> Motion {
        Motion {
            x,
            y,
            xstart: x,
            ystart: y,
            xprevious: x,
            yprevious: y,
            hspeed: Real::ZERO,
            vspeed: Real::ZERO,
            speed: Real::ZERO,
            direction: Real::ZERO,
            gravity: Real::ZERO,
            gravity_direction: Real::new(270.0),
            friction: Real::ZERO,
        }
    }

    /// GM `x`.
    pub fn x(&self) -> Real {
        self.x
    }
    /// GM `y`.
    pub fn y(&self) -> Real {
        self.y
    }
    /// GM `xstart`.
    pub fn xstart(&self) -> Real {
        self.xstart
    }
    /// GM `ystart`.
    pub fn ystart(&self) -> Real {
        self.ystart
    }
    /// GM `xprevious`.
    pub fn xprevious(&self) -> Real {
        self.xprevious
    }
    /// GM `yprevious`.
    pub fn yprevious(&self) -> Real {
        self.yprevious
    }
    /// GM `hspeed`.
    pub fn hspeed(&self) -> Real {
        self.hspeed
    }
    /// GM `vspeed`.
    pub fn vspeed(&self) -> Real {
        self.vspeed
    }
    /// GM `speed`.
    pub fn speed(&self) -> Real {
        self.speed
    }
    /// GM `direction`.
    pub fn direction(&self) -> Real {
        self.direction
    }
    /// GM `gravity`.
    pub fn gravity(&self) -> Real {
        self.gravity
    }
    /// GM `gravity_direction`.
    pub fn gravity_direction(&self) -> Real {
        self.gravity_direction
    }
    /// GM `friction`.
    pub fn friction(&self) -> Real {
        self.friction
    }

    /// Sets `x`.
    pub fn set_x(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_x")
    }
    /// Sets `y`.
    pub fn set_y(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_y")
    }
    /// Sets `xstart`.
    pub fn set_xstart(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_xstart")
    }
    /// Sets `ystart`.
    pub fn set_ystart(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_ystart")
    }
    /// Sets `xprevious`.
    pub fn set_xprevious(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_xprevious")
    }
    /// Sets `yprevious`.
    pub fn set_yprevious(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_yprevious")
    }
    /// Sets `hspeed`; recomputes speed and direction.
    pub fn set_hspeed(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_hspeed")
    }
    /// Sets `vspeed`; recomputes speed and direction.
    pub fn set_vspeed(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_vspeed")
    }
    /// Sets `speed`; recomputes hspeed and vspeed.
    pub fn set_speed(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_speed")
    }
    /// Sets `direction` in degrees; recomputes hspeed and vspeed.
    pub fn set_direction(&mut self, deg: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_direction")
    }
    /// GML `motion_set`.
    pub fn motion_set(&mut self, dir: Real, speed: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::motion_set")
    }
    /// GML `motion_add`.
    pub fn motion_add(&mut self, dir: Real, speed: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::motion_add")
    }
    /// Sets `gravity`.
    pub fn set_gravity(&mut self, g: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_gravity")
    }
    /// Sets `gravity_direction`.
    pub fn set_gravity_direction(&mut self, d: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_gravity_direction")
    }
    /// Sets `friction`.
    pub fn set_friction(&mut self, f: Real) -> Result<(), SimError> {
        unimplemented!("WP2: Motion::set_friction")
    }
    /// `xprevious = x`, `yprevious = y` (R1 decides when).
    pub fn capture_previous(&mut self) {
        unimplemented!("WP2: Motion::capture_previous")
    }
}

/// Automatic motion for one instance (tick row 8, M1): friction, gravity as a vector, path,
/// then `x += hspeed`, `y += vspeed`, no rounding.
pub fn apply_auto_motion(
    m: &mut Motion,
    path: Option<&mut PathRun>,
    def: &GameDef,
    rules: &MotionRules,
) -> Result<MotionEvents, SimError> {
    unimplemented!("WP2: motion::apply_auto_motion")
}
