//! GM motion variables and automatic motion (docs/plans/sim-core.md 3.1, register M1-M7).
//!
//! An instance's motion is stored in both forms: `hspeed`/`vspeed` (cartesian) and
//! `speed`/`direction` (polar). Writing one form recomputes the other, in exactly one place each:
//! [`Motion::recompute_polar`] and [`Motion::recompute_cartesian`] (M3-M5). Automatic motion, tick
//! slot 8, is the single function [`apply_auto_motion`] (M1, M2). Every setter rejects a
//! non-finite value with `SimError::NonFinite` and leaves the motion unchanged (N15).
//!
//! Spec: CLONE_SPEC "Automatic instance motion". The formulas are Confirmed (recorder): the
//! traces cited on each function are OpenGMK runs, so the real runner is still unchecked.
//! WP2 owns this file.

use serde::{Deserialize, Serialize};

use crate::def::GameDef;
use crate::error::SimError;
use crate::num::{self, Real};
use crate::path::{self, PathRun};
use crate::rules::MotionRules;
use crate::rules::motion::{FrictionRule, MotionOrder};

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
/// `hypot`) and `direction = radtodeg(atan2(-v, h))`, plus 360 when it is below 0.
///
/// Confirmed (recorder): `traces/little-runner/num-trig-a.csv` (ten shots bit for bit),
/// `traces/kaenbyou/stand.csv` rows 3-9 (hspeed `-7.347880794884119e-17`, vspeed 0 give
/// direction 180 and speed `7.347880794884119e-17`), `traces/kaenbyou/t-alarm-1.csv` (hspeed 16,
/// vspeed 0 give direction `-0.0`: only values below 0 wrap), `traces/kaenbyou/t-mot-06-b.csv`
/// rows 20-31 (both 0 give direction `-0.0`, M4).
fn polar_of(h: Real, v: Real) -> Result<(Real, Real), SimError> {
    let speed = finite(num::sqrt(h * h + v * v), "speed")?;
    let mut direction = num::darctan2(-v, h);
    if direction.get() < 0.0 {
        direction += Real::new(360.0);
    }
    Ok((speed, finite(direction, "direction")?))
}

/// `hspeed` and `vspeed` of `speed` at `direction` degrees (M3): `h = speed * dcos(direction)`,
/// `v = -(speed * dsin(direction))`. Also for a negative speed, which turns the vector around
/// (Confirmed (recorder): speed -2.5 at direction 224.9999999999999 gives hspeed
/// `1.7677669529663722`, vspeed `-1.7677669529663653`,
/// `traces/eat-the-rori-girl/t-mot-05-b.csv` row 3802).
///
/// OPEN, reported to research: in `t-mot-05-a.csv` (frame 492) cherries at speed -4, direction
/// 180 read vspeed `0.0`, where this formula gives `4.898587196589413e-16`
/// (`-(-4 * dsin(180))`). Off-table angles can also differ from the recorder by one ulp (M6, see
/// `num::trig`).
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
    /// vspeed from it and the current direction.
    pub fn set_speed(&mut self, v: Real) -> Result<(), SimError> {
        let v = finite(v, "speed")?;
        self.recompute_cartesian(v, self.direction)
    }

    /// Sets `direction` in degrees as written (no wrap into `[0, 360)`); recomputes hspeed and
    /// vspeed. A negative speed keeps its sign and turns with the direction (Confirmed
    /// (recorder): direction += 30 gives speed -2.5, direction 254.9999999999999, hspeed
    /// `0.6470476127563058`, vspeed `-2.41481456572267`, `t-mot-05-c.csv` row 3805).
    pub fn set_direction(&mut self, deg: Real) -> Result<(), SimError> {
        let deg = finite(deg, "direction")?;
        self.recompute_cartesian(self.speed, deg)
    }

    /// GML `motion_set(dir, speed)`: direction `dir` and speed `speed`, then one cartesian
    /// recompute (Confirmed manual: "sets the motion").
    pub fn motion_set(&mut self, dir: Real, speed: Real) -> Result<(), SimError> {
        let dir = finite(dir, "direction")?;
        let speed = finite(speed, "speed")?;
        self.recompute_cartesian(speed, dir)
    }

    /// GML `motion_add(dir, speed)`: adds `(speed * dcos(dir), -(speed * dsin(dir)))` to the
    /// vector, then recomputes speed and direction (Confirmed manual: "adds a motion as a
    /// vector"; the operation order is Inferred, no game calls it).
    pub fn motion_add(&mut self, dir: Real, speed: Real) -> Result<(), SimError> {
        let dir = finite(dir, "direction")?;
        let speed = finite(speed, "speed")?;
        let (dh, dv) = cartesian_of(speed, dir)?;
        let h = finite(self.hspeed + dh, "hspeed")?;
        let v = finite(self.vspeed + dv, "vspeed")?;
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
    /// [`cartesian_of`]).
    fn recompute_cartesian(&mut self, speed: Real, direction: Real) -> Result<(), SimError> {
        let (h, v) = cartesian_of(speed, direction)?;
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

/// Friction for one tick (M2, `MotionRules::friction`).
///
/// `FrictionRule::TowardZeroClamp`: the size of speed shrinks by `friction` and stops at exactly
/// 0 without reversing; the direction is kept; a negative friction grows the size. Then hspeed
/// and vspeed are recomputed from speed and direction (Confirmed (recorder),
/// `traces/kaenbyou/t-mot-04.csv` id 117874: speed 6 with friction 0.2 reads 5.8 on its first row,
/// then `0.19999999999999696` and `0.0`; friction -0.1 gives 7.1, 7.199999999999999; the
/// cartesian recompute fits 2593 of 2651 friction rows, the rest are M6 one-ulp gaps).
///
/// Nothing happens when friction or speed is 0, so a stopped instance does not start under a
/// negative friction (OPEN, U3; kaenbyou sets speed to minus the friction before relying on it,
/// which hints the same). A negative speed shrinks toward 0 the same way (OPEN, U3).
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
