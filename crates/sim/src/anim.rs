//! Sprite and image variables and the per-tick animation update (docs/plans/sim-core.md 2.1
//! rows 8b and 13, R7, R7b-R7d).
//!
//! WP0 declared; WP1 owns. CLONE_SPEC "sprites and animation", Confirmed (recorder): once per
//! step, after End Step and Draw, `image_index += image_speed`; at least `image_number` wraps by
//! subtracting it once and fires Animation End, whose changes stand
//! (traces/kaenbyou/t-anim-1.csv rows 18 and 45, traces/rely-the-needle-4-2/anim-save-end.csv
//! row 80). `image_index` is a double (anim-idle-float-a.csv). A sprite change resets an index
//! that is too large for the new sprite to 0 (anim-switch-a..f.csv), which
//! [`Image::change_sprite`] does.

use serde::{Deserialize, Serialize};

use crate::def::{MaskId, SpriteId};
use crate::error::SimError;
use crate::num::{Real, to_int_trunc};
use crate::rules::TickRules;
use crate::rules::tick::{BelowZeroWrap, NoSpriteAnim};

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

/// `Ok(v)` when `v` is finite, else `SimError::NonFinite` naming `what` (N15).
fn finite(v: Real, what: &str) -> Result<Real, SimError> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(SimError::non_finite(what))
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

    /// Sets `sprite_index` and nothing else. Use it where the index is known to fit (spawning,
    /// restoring); GML `sprite_index = s` ports use [`Image::change_sprite`], which applies the
    /// recorder's index reset.
    pub fn set_sprite(&mut self, s: Option<SpriteId>) {
        self.sprite = s;
    }

    /// GML `sprite_index = s` with the new sprite's `image_number`: when `image_index` is at
    /// least `frames`, it becomes 0 (Confirmed (recorder): 2.0, 2.5, 3.0 and 3.5 on the 4-image
    /// running sprite all restart at 0 on the 2-image jump sprite,
    /// traces/rely-the-needle-4-2/anim-switch-a..f.csv); a smaller index is kept (manual,
    /// traces/kaenbyou/t-anim-1.csv row 10). Clearing the sprite (`None`) keeps the index.
    pub fn change_sprite(&mut self, s: Option<SpriteId>, frames: u16) {
        self.sprite = s;
        if s.is_some() && self.index.get() >= f64::from(frames) {
            self.index = Real::ZERO;
        }
    }

    /// Sets `mask_index`.
    pub fn set_mask(&mut self, m: Option<MaskId>) {
        self.mask = m;
    }
    /// Sets `image_index`.
    pub fn set_index(&mut self, v: Real) -> Result<(), SimError> {
        self.index = finite(v, "image_index")?;
        Ok(())
    }
    /// Sets `image_speed`.
    pub fn set_speed(&mut self, v: Real) -> Result<(), SimError> {
        self.speed = finite(v, "image_speed")?;
        Ok(())
    }
    /// Sets `image_xscale`.
    pub fn set_xscale(&mut self, v: Real) -> Result<(), SimError> {
        self.xscale = finite(v, "image_xscale")?;
        Ok(())
    }
    /// Sets `image_yscale`.
    pub fn set_yscale(&mut self, v: Real) -> Result<(), SimError> {
        self.yscale = finite(v, "image_yscale")?;
        Ok(())
    }
    /// Sets `image_angle`.
    pub fn set_angle(&mut self, v: Real) -> Result<(), SimError> {
        self.angle = finite(v, "image_angle")?;
        Ok(())
    }
    /// Sets `image_alpha`.
    pub fn set_alpha(&mut self, v: Real) -> Result<(), SimError> {
        self.alpha = finite(v, "image_alpha")?;
        Ok(())
    }

    /// The subimage drawn and used for separate masks: `floor(index) mod frames` (Inferred, R7c,
    /// C6), the modulo taken non-negative. `None` for a sprite with no frames, or an index too
    /// large to convert.
    pub fn subimage(&self, frames: u16) -> Option<u16> {
        if frames == 0 {
            return None;
        }
        let whole = to_int_trunc(Real::new(self.index.get().floor())).ok()?;
        u16::try_from(whole.rem_euclid(i64::from(frames))).ok()
    }
}

/// The animation update: `image_index += image_speed`, wrap (`>=`, subtract once; Confirmed
/// (recorder), traces/kaenbyou/t-anim-1.csv). Returns true when Animation End fires.
///
/// `frames` is the sprite's `image_number`. With no sprite, or a sprite of 0 frames, the index
/// advances and never wraps under `TickRules::anim_nosprite = Advance` (Confirmed (recorder),
/// traces/little-trap/anim-nosprite.csv) and stays under `Frozen`. An index that goes below 0
/// follows `TickRules::anim_below_zero` (sprites Inferred step 3, not traced). A non-finite
/// result halts (N15).
pub fn advance(img: &mut Image, frames: u16, rules: &TickRules) -> Result<bool, SimError> {
    if img.sprite.is_none() || frames == 0 {
        if rules.anim_nosprite == NoSpriteAnim::Advance {
            img.index = finite(img.index + img.speed, "image_index")?;
        }
        return Ok(false);
    }
    let n = Real::new(f64::from(frames));
    let mut index = finite(img.index + img.speed, "image_index")?;
    let mut fired = false;
    if index.get() >= n.get() {
        index -= n;
        fired = true;
    } else if index.get() < 0.0 && rules.anim_below_zero == BelowZeroWrap::AddFramesAndFire {
        index += n;
        fired = true;
    }
    img.index = index;
    Ok(fired)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite_img(index: f64, speed: f64) -> Image {
        let mut img = Image::default();
        img.set_sprite(Some(SpriteId(0)));
        img.set_index(Real::new(index)).unwrap();
        img.set_speed(Real::new(speed)).unwrap();
        img
    }

    #[test]
    fn wraps_at_image_number_by_subtracting_once() {
        let rules = TickRules::default();
        let mut img = sprite_img(3.5, 0.5);
        assert!(advance(&mut img, 4, &rules).unwrap());
        assert_eq!(img.index(), Real::ZERO);
        let mut img = sprite_img(3.8000000000000007, 0.2);
        assert!(advance(&mut img, 4, &rules).unwrap());
        assert_eq!(img.index(), Real::new(8.881784197001252e-16));
        let mut img = sprite_img(1.5999999999999999, 0.5);
        assert!(!advance(&mut img, 4, &rules).unwrap());
        assert_eq!(img.index(), Real::new(2.0999999999999996));
        // Subtract once, even far past the end.
        let mut img = sprite_img(9.0, 0.0);
        assert!(advance(&mut img, 4, &rules).unwrap());
        assert_eq!(img.index(), Real::new(5.0));
    }

    #[test]
    fn idle_speed_reaches_subimages_as_doubles() {
        let rules = TickRules::default();
        let mut img = sprite_img(0.0, 0.2);
        let mut subs = Vec::new();
        for _ in 0..20 {
            advance(&mut img, 4, &rules).unwrap();
            subs.push(img.subimage(4).unwrap());
        }
        // Subimage 2 after 11 additions, not 10 (anim-idle-float-a.csv rows 21-22).
        assert_eq!(subs[9], 1);
        assert_eq!(subs[10], 2);
        assert_eq!(img.index(), Real::new(8.881784197001252e-16));
    }

    #[test]
    fn negative_speed_and_no_sprite_rules() {
        let mut rules = TickRules::default();
        let mut img = sprite_img(0.0, -0.5);
        assert!(advance(&mut img, 4, &rules).unwrap());
        assert_eq!(img.index(), Real::new(3.5));
        rules.anim_below_zero = BelowZeroWrap::None;
        let mut img = sprite_img(0.0, -0.5);
        assert!(!advance(&mut img, 4, &rules).unwrap());
        assert_eq!(img.index(), Real::new(-0.5));

        let mut rules = TickRules::default();
        let mut img = Image::default();
        for _ in 0..60 {
            assert!(!advance(&mut img, 0, &rules).unwrap());
        }
        assert_eq!(img.index(), Real::new(60.0), "anim-nosprite: never wraps");
        rules.anim_nosprite = NoSpriteAnim::Frozen;
        assert!(!advance(&mut img, 0, &rules).unwrap());
        assert_eq!(img.index(), Real::new(60.0));
    }

    #[test]
    fn change_sprite_resets_only_out_of_range_indices() {
        let mut img = sprite_img(2.0, 0.5);
        img.change_sprite(Some(SpriteId(1)), 2);
        assert_eq!(img.index(), Real::ZERO);
        let mut img = sprite_img(1.5999999999999999, 0.5);
        img.change_sprite(Some(SpriteId(1)), 2);
        assert_eq!(img.index(), Real::new(1.5999999999999999));
        let mut img = sprite_img(3.0, 0.5);
        img.set_sprite(Some(SpriteId(1)));
        assert_eq!(img.index(), Real::new(3.0), "set_sprite keeps the index");
    }

    #[test]
    fn subimage_floors_and_wraps() {
        let img = sprite_img(5.7, 0.0);
        assert_eq!(img.subimage(4), Some(1));
        assert_eq!(img.subimage(0), None);
        let img = sprite_img(-0.5, 0.0);
        assert_eq!(img.subimage(4), Some(3));
    }

    #[test]
    fn setters_reject_non_finite() {
        let mut img = Image::default();
        assert!(img.set_index(Real::new(f64::INFINITY)).is_err());
        assert!(img.set_speed(Real::new(f64::NAN)).is_err());
        assert!(img.set_xscale(Real::new(f64::NAN)).is_err());
        assert!(img.set_yscale(Real::new(f64::NAN)).is_err());
        assert!(img.set_angle(Real::new(f64::NAN)).is_err());
        assert!(img.set_alpha(Real::new(f64::NAN)).is_err());
        assert_eq!(img, Image::default(), "a rejected value changes nothing");
    }
}
