//! GM motion variables and automatic motion (docs/architecture/sim/motion.md, register M1-M7).
//!
//! The `gm8` representation ([`Motion`], [`MotionModel::Gm8Coupled`]) stores both forms:
//! `hspeed`/`vspeed` (cartesian) and `speed`/`direction` (polar). Writing one form recomputes the
//! other, in exactly one place each: [`Motion::recompute_polar`] and
//! [`Motion::recompute_cartesian`] (M3-M5). Automatic motion, tick slot 8, is the single function
//! [`apply_auto_motion`] (M1, M2).
//!
//! The `standard` representation ([`Body`], [`MotionModel::VelocityAcceleration`], B5) stores
//! position, velocity and gravity-as-acceleration; `speed` and `direction` are views, and the
//! polar-to-cartesian path has no integer snap. `MotionRules::model` selects one. A format-0
//! package (and `RunnerRules::for_runner`) gets `Gm8Coupled`, so no existing run changes, and no
//! package can select the standard value until the ruleset plumbing (#1160).
//!
//! Every setter in both representations rejects a non-finite value with `SimError::NonFinite` and
//! leaves the motion unchanged (N15). Spec: CLONE_SPEC "Automatic instance motion". The formulas
//! are Confirmed (recorder): the traces cited on each function are OpenGMK runs, so the real
//! runner is still unchecked. WP2 owns this file.

use serde::{Deserialize, Serialize};

use crate::def::GameDef;
use crate::error::SimError;
use crate::num::{self, Real};
use crate::path::{self, PathRun};
use crate::rules::MotionRules;
use crate::rules::motion::{FrictionRule, MotionModel, MotionOrder};

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

/// `Ok(v)` when `v` is finite, else `SimError::NonFinite` naming `what`.
fn finite(v: Real, what: &str) -> Result<Real, SimError> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(SimError::non_finite(what))
    }
}

/// `speed` and `direction` of the vector `(h, v)` (M4, M5): `speed = sqrt(h*h + v*v)` (not
/// `hypot`, not snapped) and `direction = radtodeg(atan2(-v, h))`, plus 360 when it is below 0,
/// then [`num::snap_to_int`] (so it can read 360.0: hspeed 5, vspeed 1e-6 gives 360.0; hspeed
/// 9.2e-16, vspeed -4.9 gives 90.0, not 89.99999999999999). Confirmed (recorder), research#199:
/// `traces/animus/motion-snap-hv.csv`, `motion-snap-add.csv` (all 5120 rows, 11 read 360.0).
/// The gravity add's read-back is not probed on its own; it goes through the same recompute.
///
/// Confirmed (recorder): `traces/little-runner/num-trig-a.csv` (ten shots bit for bit),
/// `traces/kaenbyou/stand.csv` rows 3-9 (hspeed `-7.347880794884119e-17`, vspeed 0 give
/// direction 180 and speed `7.347880794884119e-17`), `traces/kaenbyou/t-alarm-1.csv` (hspeed 16,
/// vspeed 0 give direction `-0.0`: only values below 0 wrap), `traces/kaenbyou/t-mot-06-b.csv`
/// rows 20-31 (both 0 give direction `-0.0`, M4).
fn polar_of(h: Real, v: Real) -> Result<(Real, Real), SimError> {
    let speed = finite(num::sqrt(h * h + v * v), "speed")?;
    Ok((speed, finite(num::snap_to_int(direction_of(h, v)?), "direction")?))
}

/// `direction` of the vector `(h, v)` before the integer snap (M5): `radtodeg(atan2(-v, h))`,
/// plus 360 when it is below 0. The gm8 representation stores the snapped value in [`polar_of`];
/// the standard representation reads this raw value as a view ([`Body::direction`]).
fn direction_of(h: Real, v: Real) -> Result<Real, SimError> {
    let mut direction = num::darctan2(-v, h);
    if direction.get() < 0.0 {
        direction += Real::new(360.0);
    }
    finite(direction, "direction")
}

/// `hspeed` and `vspeed` of `speed` at `direction` degrees (M3): `h = speed * dcos(direction)`,
/// `v = -(speed * dsin(direction))`. Also for a negative speed, which turns the vector around
/// (Confirmed (recorder): speed -2.5 at direction 224.9999999999999 gives hspeed
/// `1.7677669529663722`, vspeed `-1.7677669529663653`,
/// `traces/eat-the-rori-girl/t-mot-05-b.csv` row 3802).
///
/// No snap here: motion_add adds these raw products (research#199);
/// [`Motion::recompute_cartesian`] snaps. Off-table angles can differ from the recorder by one
/// ulp (M6, see `num::trig`).
fn cartesian_of(speed: Real, direction: Real) -> Result<(Real, Real), SimError> {
    let h = finite(num::lengthdir_x(speed, direction), "hspeed")?;
    let v = finite(num::lengthdir_y(speed, direction), "vspeed")?;
    Ok((h, v))
}

impl Motion {
    /// At `(x, y)`, also the start and previous position; speed 0, direction 0, gravity 0,
    /// gravity_direction 270, friction 0 (Confirmed (recorder):
    /// `traces/poronkusema/t-col-09-e.csv`, `traces/kaenbyou/t-mot-06.csv` row 2).
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
        self.x = finite(v, "x")?;
        Ok(())
    }
    /// Sets `y`.
    pub fn set_y(&mut self, v: Real) -> Result<(), SimError> {
        self.y = finite(v, "y")?;
        Ok(())
    }
    /// Sets `xstart`.
    pub fn set_xstart(&mut self, v: Real) -> Result<(), SimError> {
        self.xstart = finite(v, "xstart")?;
        Ok(())
    }
    /// Sets `ystart`.
    pub fn set_ystart(&mut self, v: Real) -> Result<(), SimError> {
        self.ystart = finite(v, "ystart")?;
        Ok(())
    }
    /// Sets `xprevious`.
    pub fn set_xprevious(&mut self, v: Real) -> Result<(), SimError> {
        self.xprevious = finite(v, "xprevious")?;
        Ok(())
    }
    /// Sets `yprevious`.
    pub fn set_yprevious(&mut self, v: Real) -> Result<(), SimError> {
        self.yprevious = finite(v, "yprevious")?;
        Ok(())
    }

    /// Sets `hspeed`; recomputes speed and direction (vspeed is kept, Confirmed manual).
    ///
    /// Writing the value hspeed already holds (same bits) changes nothing: `hspeed = hspeed` on a
    /// cherry with speed -2.5 keeps speed -2.5 and its direction (Confirmed (recorder),
    /// `traces/eat-the-rori-girl/t-mot-05-c.csv` row 3806). Whether a different but
    /// compare-equal value also skips the recompute is OPEN (M3).
    pub fn set_hspeed(&mut self, v: Real) -> Result<(), SimError> {
        let v = finite(v, "hspeed")?;
        if v == self.hspeed {
            return Ok(());
        }
        self.recompute_polar(v, self.vspeed)
    }

    /// Sets `vspeed`; recomputes speed and direction. Same-bits writes change nothing, as for
    /// [`Motion::set_hspeed`].
    pub fn set_vspeed(&mut self, v: Real) -> Result<(), SimError> {
        let v = finite(v, "vspeed")?;
        if v == self.vspeed {
            return Ok(());
        }
        self.recompute_polar(self.hspeed, v)
    }

    /// Sets `speed` as written (a negative speed stays negative, M3); recomputes hspeed and
    /// vspeed from it and the current direction, with the integer snap.
    pub fn set_speed(&mut self, v: Real) -> Result<(), SimError> {
        let v = finite(v, "speed")?;
        self.recompute_cartesian(v, self.direction)
    }

    /// Sets `direction` in degrees as written (no wrap into `[0, 360)`); recomputes hspeed and
    /// vspeed. A negative speed keeps its sign and turns with the direction (Confirmed
    /// (recorder): direction += 30 gives speed -2.5, direction 254.9999999999999, hspeed
    /// `0.6470476127563058`, vspeed `-2.41481456572267`, `t-mot-05-c.csv` row 3805).
    ///
    /// hspeed and vspeed get the integer snap: speed 8 at direction 120 gives
    /// hspeed -4.0, not -3.9999999999999982.
    pub fn set_direction(&mut self, deg: Real) -> Result<(), SimError> {
        let deg = finite(deg, "direction")?;
        self.recompute_cartesian(self.speed, deg)
    }

    /// GML `motion_set(dir, speed)`: direction `dir` and speed `speed`, then one cartesian
    /// recompute with the integer snap (Confirmed (recorder), research#199: motion_set and the D&D
    /// Move Free action give the same bits as speed and direction writes,
    /// `traces/animus/motion-snap-set.csv`; and `t-mot-05-a.csv` frame 492: speed -4 at 180
    /// reads vspeed 0.0).
    pub fn motion_set(&mut self, dir: Real, speed: Real) -> Result<(), SimError> {
        let dir = finite(dir, "direction")?;
        let speed = finite(speed, "speed")?;
        self.recompute_cartesian(speed, dir)
    }

    /// GML `motion_add(dir, speed)`: adds `(speed * dcos(dir), -(speed * dsin(dir)))` to the
    /// vector without the integer snap, then recomputes speed and direction; when `(h, v)` stays
    /// bit-identical (a speed of 1e-16), speed and direction keep their old values (Confirmed
    /// (recorder), research#199: `traces/animus/motion-snap-add.csv`; speed -4.9 at 90 added to
    /// a still instance leaves hspeed `-3.0003846579110154e-16`).
    pub fn motion_add(&mut self, dir: Real, speed: Real) -> Result<(), SimError> {
        let dir = finite(dir, "direction")?;
        let speed = finite(speed, "speed")?;
        let (dh, dv) = cartesian_of(speed, dir)?;
        let h = finite(self.hspeed + dh, "hspeed")?;
        let v = finite(self.vspeed + dv, "vspeed")?;
        if h == self.hspeed && v == self.vspeed {
            return Ok(());
        }
        self.recompute_polar(h, v)
    }

    /// Sets `gravity`.
    pub fn set_gravity(&mut self, g: Real) -> Result<(), SimError> {
        self.gravity = finite(g, "gravity")?;
        Ok(())
    }
    /// Sets `gravity_direction` in degrees, as written.
    pub fn set_gravity_direction(&mut self, d: Real) -> Result<(), SimError> {
        self.gravity_direction = finite(d, "gravity_direction")?;
        Ok(())
    }
    /// Sets `friction`.
    pub fn set_friction(&mut self, f: Real) -> Result<(), SimError> {
        self.friction = finite(f, "friction")?;
        Ok(())
    }

    /// `xprevious = x`, `yprevious = y` (R1 decides when).
    pub fn capture_previous(&mut self) {
        self.xprevious = self.x;
        self.yprevious = self.y;
    }

    /// Stores `(h, v)` and the speed and direction computed from them. The only place that turns
    /// the cartesian form into the polar one (M4, M5; formula in [`polar_of`]).
    fn recompute_polar(&mut self, h: Real, v: Real) -> Result<(), SimError> {
        let (speed, direction) = polar_of(h, v)?;
        self.hspeed = h;
        self.vspeed = v;
        self.speed = speed;
        self.direction = direction;
        Ok(())
    }

    /// Stores `speed` and `direction` as given and the hspeed and vspeed computed from them. The
    /// only place that turns the polar form into the cartesian one (M3; formula in
    /// [`cartesian_of`]), then [`num::snap_to_int`] on each: speed and direction writes,
    /// motion_set and friction (research#193, research#199).
    fn recompute_cartesian(&mut self, speed: Real, direction: Real) -> Result<(), SimError> {
        let (h, v) = cartesian_of(speed, direction)?;
        let (h, v) = (num::snap_to_int(h), num::snap_to_int(v));
        self.speed = speed;
        self.direction = direction;
        self.hspeed = h;
        self.vspeed = v;
        Ok(())
    }

    /// Moves to `(x, y)` (paths). Both must be finite.
    pub(crate) fn place(&mut self, x: Real, y: Real) -> Result<(), SimError> {
        let x = finite(x, "x")?;
        let y = finite(y, "y")?;
        self.x = x;
        self.y = y;
        Ok(())
    }
}

/// The `standard` ruleset's motion representation (B5, [`MotionModel::VelocityAcceleration`]).
///
/// Position, velocity and gravity-as-acceleration are the state. `speed` and `direction` are
/// views computed from the velocity, so there is no polar value to keep in step and no integer
/// snap: writing one form does not round the other, and the direction view keeps the raw angle.
/// Every setter rejects a non-finite value with `SimError::NonFinite` and leaves the state
/// unchanged (N15).
///
/// A zero velocity has no direction: [`Body::direction`] reads `+0.0` and writing a direction at
/// rest leaves the body at rest. A caller that wants to set both should use
/// [`Body::motion_set`]. The gm8 representation keeps its stored, possibly negative, speed and
/// its stored direction ([`Motion`]); that is why a port stays on `Gm8Coupled`.
///
/// This type is not a caller's state yet: the world stores a [`Motion`] and the default ruleset
/// selects `Gm8Coupled`. A ruleset-selected caller adopts `Body` after #1160, and path motion
/// stays on `Motion` until `path::advance` takes a shared motion interface.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Body {
    x: Real,
    y: Real,
    xstart: Real,
    ystart: Real,
    xprevious: Real,
    yprevious: Real,
    hspeed: Real,
    vspeed: Real,
    gravity: Real,
    gravity_direction: Real,
    friction: Real,
}

impl Body {
    /// At `(x, y)`, also the start and previous position; velocity 0, gravity 0,
    /// gravity_direction 270, friction 0 (the same defaults as [`Motion::new_at`]).
    pub fn new_at(x: Real, y: Real) -> Body {
        Body {
            x,
            y,
            xstart: x,
            ystart: y,
            xprevious: x,
            yprevious: y,
            hspeed: Real::ZERO,
            vspeed: Real::ZERO,
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
    /// The `x` component of the velocity (GM `hspeed`).
    pub fn hspeed(&self) -> Real {
        self.hspeed
    }
    /// The `y` component of the velocity (GM `vspeed`).
    pub fn vspeed(&self) -> Real {
        self.vspeed
    }
    /// The gravity magnitude (GM `gravity`).
    pub fn gravity(&self) -> Real {
        self.gravity
    }
    /// The gravity direction in degrees (GM `gravity_direction`).
    pub fn gravity_direction(&self) -> Real {
        self.gravity_direction
    }
    /// The friction magnitude (GM `friction`).
    pub fn friction(&self) -> Real {
        self.friction
    }

    /// GM `speed` as a view of the velocity (M4): `sqrt(h*h + v*v)`, non-negative and unsnapped.
    ///
    /// A negative speed is not a state here: writing one moves the body backwards and this view
    /// reads the magnitude. `gm8` keeps the negative value as written ([`Motion::speed`]).
    pub fn speed(&self) -> Result<Real, SimError> {
        finite(
            num::sqrt(self.hspeed * self.hspeed + self.vspeed * self.vspeed),
            "speed",
        )
    }

    /// GM `direction` as a view of the velocity (M5): `radtodeg(atan2(-v, h))`, plus 360 when it is
    /// below 0, with no integer snap. A zero velocity reads `+0.0`; the view has no vector to
    /// derive a direction from.
    pub fn direction(&self) -> Result<Real, SimError> {
        if self.hspeed.get() == 0.0 && self.vspeed.get() == 0.0 {
            return Ok(Real::ZERO);
        }
        direction_of(self.hspeed, self.vspeed)
    }

    /// The gravity acceleration vector `(g * dcos(gd), -(g * dsin(gd)))` (M6), the formula the
    /// gm8 gravity step applies to the velocity each tick. The vector is a view of the stored
    /// magnitude and direction; it is not integrated here.
    pub fn acceleration(&self) -> (Real, Real) {
        (
            self.gravity * num::dcos(self.gravity_direction),
            -(self.gravity * num::dsin(self.gravity_direction)),
        )
    }

    /// Sets `x`.
    pub fn set_x(&mut self, v: Real) -> Result<(), SimError> {
        self.x = finite(v, "x")?;
        Ok(())
    }
    /// Sets `y`.
    pub fn set_y(&mut self, v: Real) -> Result<(), SimError> {
        self.y = finite(v, "y")?;
        Ok(())
    }
    /// Sets `xstart`.
    pub fn set_xstart(&mut self, v: Real) -> Result<(), SimError> {
        self.xstart = finite(v, "xstart")?;
        Ok(())
    }
    /// Sets `ystart`.
    pub fn set_ystart(&mut self, v: Real) -> Result<(), SimError> {
        self.ystart = finite(v, "ystart")?;
        Ok(())
    }
    /// Sets `xprevious`.
    pub fn set_xprevious(&mut self, v: Real) -> Result<(), SimError> {
        self.xprevious = finite(v, "xprevious")?;
        Ok(())
    }
    /// Sets `yprevious`.
    pub fn set_yprevious(&mut self, v: Real) -> Result<(), SimError> {
        self.yprevious = finite(v, "yprevious")?;
        Ok(())
    }

    /// Sets the `x` component of the velocity (GM `hspeed`). The views follow from it; there is no
    /// cache to recompute.
    pub fn set_hspeed(&mut self, v: Real) -> Result<(), SimError> {
        self.hspeed = finite(v, "hspeed")?;
        Ok(())
    }
    /// Sets the `y` component of the velocity (GM `vspeed`). The views follow from it.
    pub fn set_vspeed(&mut self, v: Real) -> Result<(), SimError> {
        self.vspeed = finite(v, "vspeed")?;
        Ok(())
    }

    /// Writes `speed` by deriving the velocity from it and the current direction view, without
    /// the integer snap. A non-negative speed moves along the direction view; a negative one
    /// moves against it.
    pub fn set_speed(&mut self, v: Real) -> Result<(), SimError> {
        let speed = finite(v, "speed")?;
        let direction = self.direction()?;
        let (h, v) = cartesian_of(speed, direction)?;
        self.hspeed = h;
        self.vspeed = v;
        Ok(())
    }

    /// Writes `direction` by deriving the velocity from it and the current speed view, without
    /// the integer snap. At rest the speed is 0, so this leaves the body at rest.
    pub fn set_direction(&mut self, deg: Real) -> Result<(), SimError> {
        let direction = finite(deg, "direction")?;
        let speed = self.speed()?;
        let (h, v) = cartesian_of(speed, direction)?;
        self.hspeed = h;
        self.vspeed = v;
        Ok(())
    }

    /// GML `motion_set(dir, speed)`: the velocity from both values, without the integer snap.
    pub fn motion_set(&mut self, dir: Real, speed: Real) -> Result<(), SimError> {
        let dir = finite(dir, "direction")?;
        let speed = finite(speed, "speed")?;
        let (h, v) = cartesian_of(speed, dir)?;
        self.hspeed = h;
        self.vspeed = v;
        Ok(())
    }

    /// GML `motion_add(dir, speed)`: adds the vector to the velocity, without the integer snap.
    pub fn motion_add(&mut self, dir: Real, speed: Real) -> Result<(), SimError> {
        let dir = finite(dir, "direction")?;
        let speed = finite(speed, "speed")?;
        let (dh, dv) = cartesian_of(speed, dir)?;
        self.hspeed = finite(self.hspeed + dh, "hspeed")?;
        self.vspeed = finite(self.vspeed + dv, "vspeed")?;
        Ok(())
    }

    /// Sets `gravity`.
    pub fn set_gravity(&mut self, g: Real) -> Result<(), SimError> {
        self.gravity = finite(g, "gravity")?;
        Ok(())
    }
    /// Sets `gravity_direction` in degrees, as written.
    pub fn set_gravity_direction(&mut self, d: Real) -> Result<(), SimError> {
        self.gravity_direction = finite(d, "gravity_direction")?;
        Ok(())
    }
    /// Sets `friction`.
    pub fn set_friction(&mut self, f: Real) -> Result<(), SimError> {
        self.friction = finite(f, "friction")?;
        Ok(())
    }

    /// `xprevious = x`, `yprevious = y` (R1 decides when).
    pub fn capture_previous(&mut self) {
        self.xprevious = self.x;
        self.yprevious = self.y;
    }

    /// One tick of automatic motion in the standard representation: friction, gravity as an
    /// acceleration, then `x += hspeed`, `y += vspeed`, with no integer snap.
    ///
    /// The order is `MotionRules::order` (M1). `MotionModel::VelocityAcceleration` is the model
    /// this representation implements; [`apply_auto_motion`] implements `Gm8Coupled` and rejects
    /// this one. Paths are not integrated: `path::advance` takes a [`Motion`], so a path stays on
    /// the gm8 representation until the adoption slice. The `def` and path arguments of the gm8
    /// function therefore have no counterpart here.
    pub fn apply_auto_motion(&mut self, rules: &MotionRules) -> Result<MotionEvents, SimError> {
        if rules.model != MotionModel::VelocityAcceleration {
            return Err(SimError::unsupported(
                "the gm8 motion representation on Body; use Motion",
            ));
        }
        match rules.order {
            MotionOrder::FrictionGravityPathMove => {
                self.apply_friction(rules.friction)?;
                self.apply_gravity()?;
                let x = finite(self.x + self.hspeed, "x")?;
                let y = finite(self.y + self.vspeed, "y")?;
                self.x = x;
                self.y = y;
            }
        }
        Ok(MotionEvents::default())
    }

    /// Friction for one tick (M2) on the velocity, without the integer snap.
    ///
    /// `FrictionRule::TowardZeroClamp` shrinks the speed view by `friction` and stops at exactly
    /// 0; the direction view is kept, so the velocity keeps its heading. A negative friction grows
    /// the speed. The gm8 rule works on the stored (possibly negative) speed and snaps the
    /// recomputed components; this one has no negative speed and does not snap (M3).
    fn apply_friction(&mut self, rule: FrictionRule) -> Result<(), SimError> {
        let f = self.friction;
        let s = self.speed()?;
        if f.get() == 0.0 || s.get() == 0.0 {
            return Ok(());
        }
        let next = match rule {
            FrictionRule::TowardZeroClamp => {
                let n = s - f;
                if n.get() < 0.0 { Real::ZERO } else { n }
            }
        };
        let direction = self.direction()?;
        let (h, v) = cartesian_of(finite(next, "speed")?, direction)?;
        self.hspeed = h;
        self.vspeed = v;
        Ok(())
    }

    /// Adds the gravity acceleration to the velocity for one tick, without the integer snap.
    ///
    /// Skipped when gravity is 0, as in the gm8 step: adding a zero vector would turn `-0.0` into
    /// `+0.0` and change a resting body's signed zeros (M3, M4).
    fn apply_gravity(&mut self) -> Result<(), SimError> {
        if self.gravity.get() == 0.0 {
            return Ok(());
        }
        let (ax, ay) = self.acceleration();
        self.hspeed = finite(self.hspeed + ax, "hspeed")?;
        self.vspeed = finite(self.vspeed + ay, "vspeed")?;
        Ok(())
    }
}

/// Friction for one tick (M2, `MotionRules::friction`).
///
/// `FrictionRule::TowardZeroClamp`: the size of speed shrinks by `friction` and stops at exactly
/// 0 without reversing; the direction is kept; a negative friction grows the size. Then hspeed
/// and vspeed are recomputed from speed and direction (Confirmed (recorder),
/// `traces/kaenbyou/t-mot-04.csv` id 117874: speed 6 with friction 0.2 reads 5.8 on its first row,
/// then `0.19999999999999696` and `0.0`; friction -0.1 gives 7.1, 7.199999999999999; the
/// cartesian recompute fits 2593 of 2651 friction rows, the rest are M6 one-ulp gaps). The
/// recompute has the integer snap, and a negative speed gets speed + friction (Confirmed
/// (recorder), research#199, `traces/animus/motion-snap-fric.csv`: speed 8.099999 at 60 with
/// friction 0.1 reads speed 7.999999000000001 and hspeed 4.0).
///
/// Nothing happens when friction or speed is 0, so a stopped instance does not start under a
/// negative friction (OPEN, U3; kaenbyou sets speed to minus the friction before relying on it,
/// which hints the same). A negative speed crossing 0 is OPEN (research#201).
fn apply_friction(m: &mut Motion, rule: FrictionRule) -> Result<(), SimError> {
    let f = m.friction;
    let s = m.speed;
    if f.get() == 0.0 || s.get() == 0.0 {
        return Ok(());
    }
    let next = match rule {
        FrictionRule::TowardZeroClamp => {
            if s.get() > 0.0 {
                let n = s - f;
                if n.get() < 0.0 { Real::ZERO } else { n }
            } else {
                let n = s + f;
                if n.get() > 0.0 { Real::ZERO } else { n }
            }
        }
    };
    let next = finite(next, "speed")?;
    m.recompute_cartesian(next, m.direction)
}

/// Gravity for one tick: `hspeed += gravity * dcos(gravity_direction)`,
/// `vspeed -= gravity * dsin(gravity_direction)`, then speed and direction are recomputed
/// (Confirmed (recorder): the kid's `-7.347880794884119e-17` hspeed residue at direction 270,
/// `traces/showdown/t-mot-03-a.csv`; gravity_direction -270 gives the same residue, `-b.csv`;
/// bullets with gravity 0.1 at 180 lose 0.1 hspeed per tick, `traces/kaenbyou/t-mot-04.csv`).
///
/// Skipped when gravity is 0, so instances without gravity keep a negative speed and signed-zero
/// speeds (Design: adding a zero vector would turn `-0.0` into `+0.0` and the polar recompute
/// would make a negative speed positive; the traced cherries keep speed -4).
fn apply_gravity(m: &mut Motion) -> Result<(), SimError> {
    let g = m.gravity;
    if g.get() == 0.0 {
        return Ok(());
    }
    let gd = m.gravity_direction;
    let h = finite(m.hspeed + g * num::dcos(gd), "hspeed")?;
    let v = finite(m.vspeed - g * num::dsin(gd), "vspeed")?;
    m.recompute_polar(h, v)
}

/// Automatic motion for one instance (tick row 8, M1): friction, gravity as a vector, path,
/// then `x += hspeed`, `y += vspeed`, no rounding.
///
/// The order is `MotionRules::order` (M1). Friction and gravity before the move are Confirmed
/// (recorder) (`traces/rely-the-needle-4-2/t-evo-1.csv` row 3, `num-fpu-jump-a.csv`); friction
/// before gravity is Inferred (U2: no traced instance had both). Gravity written in Step is used
/// the same tick because Step runs before this (Confirmed (recorder), num-fpu-jump-a rows 11-13).
/// The position keeps the full double sum (Confirmed (recorder): `560.3999999999994`,
/// `traces/showdown/t-mot-01.csv` row 49).
///
/// `path` is the instance's running path; [`path::advance`] moves it and reports its end. On
/// `PathEnd::Stop` the caller may drop the run with [`path::end`]; a stopped run that stays
/// raises no further End of Path.
pub fn apply_auto_motion(
    m: &mut Motion,
    path: Option<&mut PathRun>,
    def: &GameDef,
    rules: &MotionRules,
) -> Result<MotionEvents, SimError> {
    if rules.model != MotionModel::Gm8Coupled {
        return Err(SimError::unsupported(
            "the standard motion representation on Motion; use motion::Body",
        ));
    }
    let mut events = MotionEvents::default();
    match rules.order {
        MotionOrder::FrictionGravityPathMove => {
            apply_friction(m, rules.friction)?;
            apply_gravity(m)?;
            if let Some(run) = path {
                events.end_of_path = path::advance(run, m, def)?;
            }
            let x = m.x + m.hspeed;
            let y = m.y + m.vspeed;
            m.place(x, y)?;
        }
    }
    Ok(events)
}
