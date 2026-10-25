//! Views and runner view follow (docs/plans/sim-core.md 4.5, R19).
//!
//! Views affect gameplay through `view_xview` reads, so they are hashed sim state. WP0
//! declares; WP4b owns.
#![allow(unused_variables, reason = "WP0 stub: WP4b fills the body")]

use serde::{Deserialize, Serialize};

use crate::collide::Target;
use crate::error::SimError;
use crate::num::Real;
use crate::world::Ctx;

/// One view's run-time state.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct View {
    /// `view_enabled` for the room.
    pub enabled: bool,
    /// `view_visible`.
    pub visible: bool,
    /// `view_xview`.
    pub x: Real,
    /// `view_yview`.
    pub y: Real,
    /// `view_wview`.
    pub w: Real,
    /// `view_hview`.
    pub h: Real,
    /// Port on the screen.
    pub port: ViewPort,
    /// What the runner follows.
    pub follow: Option<Target>,
    /// `view_hborder`.
    pub hborder: Real,
    /// `view_vborder`.
    pub vborder: Real,
    /// `view_hspeed`.
    pub hspeed: Real,
    /// `view_vspeed`.
    pub vspeed: Real,
    /// `view_angle`.
    pub angle: Real,
}

/// A view's port on the screen.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ViewPort {
    /// `view_xport`.
    pub x: Real,
    /// `view_yport`.
    pub y: Real,
    /// `view_wport`.
    pub w: Real,
    /// `view_hport`.
    pub h: Real,
}

/// The runner's view follow, once per tick before the Draw slot (Inferred, R19).
pub fn follow_update(ctx: &mut Ctx) -> Result<(), SimError> {
    unimplemented!("WP4b: view::follow_update")
}
