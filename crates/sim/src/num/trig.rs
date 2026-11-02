//! Degree trigonometry for built-ins: GM8 motion, gravity and `lengthdir`.
//!
//! GM8 runs on the x87 FPU, whose FSIN and FCOS can differ from libm. The kid's gravity uses
//! `gravity_direction` trig, and the drift this causes is a known trick. So [`dsin`] and
//! [`dcos`] first look the angle up in [`TRIG_TABLE`], which holds exact bit patterns with a
//! named source, and use libm for every other angle (docs/architecture.md, "Trig fidelity").
//!
//! M6 (UNKNOWN, CLONE_SPEC numbers and motion U6): x87 vs libm for the trig bits. Every table
//! entry equals libm on the radian value. The trace recorder is binary64 (Confirmed (recorder):
//! `traces/little-runner/num-trig-a.csv`, ten aimed shots bit-identical to binary64 libm), and
//! 180 and 270 degrees are not special-cased there (`traces/showdown/t-mot-03-a.csv`,
//! `traces/kaenbyou/t-mot-04.csv`). What the x87 runner computes is OPEN; if it differs, only the
//! table changes.
//!
//! M6 also covers a gap measured against the recorder's own traces: at about 3% of whole-degree
//! angles our libm is one ulp off the recorded value (284 degrees in
//! `traces/eat-the-rori-girl/t-mot-05-b.csv` frame 4027, 45 degrees in `t-mot-05-b.csv` frame
//! 3802; the test `m6_recorder_gaps` pins them). Which bits to follow there is OPEN for the
//! CLONE_SPEC; the sim keeps libm.

use super::{Real, math};

/// Degrees to radians factor, `PI / 180` rounded to f64. Rust's `f64::to_radians` uses the same
/// constant. Multiplying by it differs from `x * PI / 180` for about a quarter of angles.
///
/// OPEN (M6): confirm in CLONE_SPEC which form GM8 uses. The traced conversions fit this form.
pub const DEG_TO_RAD: f64 = core::f64::consts::PI / 180.0;

/// Radians to degrees factor, `180 / PI` rounded to f64.
///
/// OPEN (M6): confirm in CLONE_SPEC which form GM8 uses (`x * (180 / PI)` or `x * 180 / PI`).
/// `traces/little-runner/num-trig-a.csv` fits this form for all ten shots.
pub const RAD_TO_DEG: f64 = 180.0 / core::f64::consts::PI;

/// Exact sine and cosine for one angle in degrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrigEntry {
    /// The angle, matched bit for bit (`-0.0` does not match `0.0`).
    pub degrees: Real,
    /// Sine of the angle.
    pub sin: Real,
    /// Cosine of the angle.
    pub cos: Real,
}

/// 0 degrees: sin `0.0`, cos `1.0`. Source: binary64 `sin`/`cos` of 0 radians, which are exact
/// (M6). Fits every traced instance that moves at direction 0.
pub const TRIG_0: TrigEntry = TrigEntry {
    degrees: Real::new(0.0),
    sin: Real::from_bits(0x0000_0000_0000_0000),
    cos: Real::from_bits(0x3FF0_0000_0000_0000),
};

/// 90 degrees: sin `1.0`, cos `6.123233995736766e-17`. Source: binary64 `sin`/`cos` of
/// `90 * DEG_TO_RAD` = `1.5707963267948966` (M6). Not yet checked against a trace.
pub const TRIG_90: TrigEntry = TrigEntry {
    degrees: Real::new(90.0),
    sin: Real::from_bits(0x3FF0_0000_0000_0000),
    cos: Real::from_bits(0x3C91_A626_3314_5C07),
};

/// 180 degrees: sin `1.2246467991473532e-16`, cos `-1.0`. Source: binary64 `sin`/`cos` of
/// `180 * DEG_TO_RAD` = `3.141592653589793` (M6). Trace: bullets with gravity 0.1 at direction
/// 180 change vspeed by at most `1.3877787807814457e-17` per tick, the sine residue
/// (`traces/kaenbyou/t-mot-04.csv`, CLONE_SPEC motion Confirmed (recorder)).
pub const TRIG_180: TrigEntry = TrigEntry {
    degrees: Real::new(180.0),
    sin: Real::from_bits(0x3CA1_A626_3314_5C07),
    cos: Real::from_bits(0xBFF0_0000_0000_0000),
};

/// 270 degrees: sin `-1.0`, cos `-1.8369701987210297e-16`. Source: the recorder trace
/// `research/specs/engines/gm8/traces/rely-the-needle-4-2/djump.csv` (also `kaenbyou/` and
/// `showdown/t-mot-03-a.csv`): with gravity 0.4 and gravity_direction 270, frame 2 has hspeed
/// `0.4 * cos = -7.347880794884119e-17` and vspeed `0.4`. Both equal binary64 `sin`/`cos` of
/// `270 * DEG_TO_RAD` = `4.71238898038469` (M6).
pub const TRIG_270: TrigEntry = TrigEntry {
    degrees: Real::new(270.0),
    sin: Real::from_bits(0xBFF0_0000_0000_0000),
    cos: Real::from_bits(0xBCAA_7939_4C9E_8A0A),
};

/// Angles whose sine and cosine are fixed bit patterns rather than computed by libm.
pub const TRIG_TABLE: [TrigEntry; 4] = [TRIG_0, TRIG_90, TRIG_180, TRIG_270];

/// The table entry for exactly this angle, if any.
fn lookup(degrees: Real) -> Option<&'static TrigEntry> {
    TRIG_TABLE.iter().find(|e| e.degrees == degrees)
}

/// GML `degtorad(x)`: `x * DEG_TO_RAD`.
pub fn degtorad(degrees: Real) -> Real {
    Real::new(degrees.get() * DEG_TO_RAD)
}

/// GML `radtodeg(x)`: `x * RAD_TO_DEG`.
pub fn radtodeg(radians: Real) -> Real {
    Real::new(radians.get() * RAD_TO_DEG)
}

/// Sine of an angle in degrees: the table value for table angles, else `sin(degtorad(x))`.
pub fn dsin(degrees: Real) -> Real {
    match lookup(degrees) {
        Some(entry) => entry.sin,
        None => math::sin(degtorad(degrees)),
    }
}

/// Cosine of an angle in degrees: the table value for table angles, else `cos(degtorad(x))`.
pub fn dcos(degrees: Real) -> Real {
    match lookup(degrees) {
        Some(entry) => entry.cos,
        None => math::cos(degtorad(degrees)),
    }
}

/// Angle of the vector `(x, y)` in degrees, in `[-180, 180]`: `radtodeg(atan2(y, x))`.
pub fn darctan2(y: Real, x: Real) -> Real {
    radtodeg(math::atan2(y, x))
}

/// GML `point_direction`: direction from `(x1, y1)` to `(x2, y2)` in degrees, in `[0, 360)`.
/// GM's y axis points down, hence `y1 - y2`. A negative angle gets 360 added.
///
/// The formula and the `< 0` normalization match the recorder's direction readback
/// (`degrees(atan2(-v, h)) mod 360`, `traces/little-runner/num-trig-a.csv`); for the real runner
/// they stay OPEN (M6).
pub fn point_direction(x1: Real, y1: Real, x2: Real, y2: Real) -> Real {
    let d = darctan2(y1 - y2, x2 - x1);
    if d.get() < 0.0 { d + Real::new(360.0) } else { d }
}

/// GML `point_distance`: `sqrt(dx * dx + dy * dy)`. Not `hypot`, which rounds differently.
///
/// The recorder's speed readback uses this formula, not `hypot` (`traces/little-runner/num-trig-a.csv`
/// shot 148718: `2.9999999999999996` where `hypot` gives `3.0`); the real runner is OPEN (M6).
pub fn point_distance(x1: Real, y1: Real, x2: Real, y2: Real) -> Real {
    let dx = x2 - x1;
    let dy = y2 - y1;
    math::sqrt(dx * dx + dy * dy)
}

/// GML `lengthdir_x(len, dir)`: `len * dcos(dir)`.
pub fn lengthdir_x(len: Real, dir: Real) -> Real {
    len * dcos(dir)
}

/// GML `lengthdir_y(len, dir)`: `-(len * dsin(dir))`, negative because GM's y axis points down.
/// Negating before or after the product gives the same bits.
pub fn lengthdir_y(len: Real, dir: Real) -> Real {
    -(len * dsin(dir))
}
