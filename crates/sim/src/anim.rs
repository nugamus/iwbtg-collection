//! Sprite and image variables and the per-tick animation update (docs/plans/sim-core.md 2.1
//! rows 8b and 13, R7, R7b-R7d).
//!
//! WP0 declares; WP1 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP1 fills the bodies")]

use serde::{Deserialize, Serialize};

use crate::def::{MaskId, SpriteId};
use crate::error::SimError;
use crate::num::Real;
use crate::rules::TickRules;

/// An instance's sprite, mask and image variables. Setters reject non-finite values (N15).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Image {
    sprite: Option<SpriteId>,
    mask: Option<MaskId>,
    index: Real,
    speed: Real,
    xscale: Real,
    yscale: Real,
    angle: Real,
    alpha: Real,
}

impl Default for Image {
    /// Default image state (Inferred, R7d): no sprite, index 0, image_speed 1, scales 1,
    /// angle 0, alpha 1.
    fn default() -> Image {
        Image {
            sprite: None,
            mask: None,
            index: Real::ZERO,
            speed: Real::ONE,
            xscale: Real::ONE,
            yscale: Real::ONE,
            angle: Real::ZERO,
            alpha: Real::ONE,
        }
    }
}

impl Image {
    /// `sprite_index`.
    pub fn sprite(&self) -> Option<SpriteId> {
        self.sprite
    }
    /// `mask_index`.
    pub fn mask(&self) -> Option<MaskId> {
        self.mask
    }
    /// `image_index`.
    pub fn index(&self) -> Real {
        self.index
    }
    /// `image_speed`.
    pub fn speed(&self) -> Real {
        self.speed
    }
    /// `image_xscale`.
    pub fn xscale(&self) -> Real {
        self.xscale
    }
    /// `image_yscale`.
    pub fn yscale(&self) -> Real {
        self.yscale
    }
    /// `image_angle`.
    pub fn angle(&self) -> Real {
        self.angle
    }
    /// `image_alpha`.
    pub fn alpha(&self) -> Real {
        self.alpha
    }

    /// Sets `sprite_index`.
    pub fn set_sprite(&mut self, s: Option<SpriteId>) {
        unimplemented!("WP1: Image::set_sprite")
    }
    /// Sets `mask_index`.
    pub fn set_mask(&mut self, m: Option<MaskId>) {
        unimplemented!("WP1: Image::set_mask")
    }
    /// Sets `image_index`.
    pub fn set_index(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP1: Image::set_index")
    }
    /// Sets `image_speed`.
    pub fn set_speed(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP1: Image::set_speed")
    }
    /// Sets `image_xscale`.
    pub fn set_xscale(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP1: Image::set_xscale")
    }
    /// Sets `image_yscale`.
    pub fn set_yscale(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP1: Image::set_yscale")
    }
    /// Sets `image_angle`.
    pub fn set_angle(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP1: Image::set_angle")
    }
    /// Sets `image_alpha`.
    pub fn set_alpha(&mut self, v: Real) -> Result<(), SimError> {
        unimplemented!("WP1: Image::set_alpha")
    }

    /// The subimage drawn and used for separate masks: `floor(index) mod frames` (Inferred, R7c,
    /// C6). `None` for a sprite with no frames.
    pub fn subimage(&self, frames: u16) -> Option<u16> {
        unimplemented!("WP1: Image::subimage")
    }
}

/// The animation update: `image_index += image_speed`, wrap (`>=`, subtract once; Trace
/// t-anim-1). Returns true when Animation End fires. A sprite-less instance does not advance
/// (R7b).
pub fn advance(img: &mut Image, frames: u16, rules: &TickRules) -> Result<bool, SimError> {
    unimplemented!("WP1: anim::advance")
}
