//! Degree trigonometry for built-ins: GM8 motion, gravity and `lengthdir`.
//!
//! GM8 runs on the x87 FPU, whose FSIN and FCOS can differ from libm. The kid's gravity uses
//! `gravity_direction` trig, and the drift this causes is a known trick. So [`dsin`] and
//! [`dcos`] first look the angle up in [`TRIG_TABLE`], which holds exact bit patterns with a
//! named source, and use libm for every other angle (docs/architecture.md, "Trig fidelity").
//!
//! Today every table entry equals libm on the radian value, because the traces were recorded
//! under OpenGMK, which uses plain f64. OPEN (docs/architecture.md): whether the real GM8
//! runner gives the same bits; settled by the Phase 2 cross-check. If it does not, only the
//! table changes.
//!
//! OPEN: OpenGMK takes sin and cos from the platform C runtime (Windows x64 UCRT for the trace
//! recorder), not from libm. The two agree on the four table angles but differ by one ulp on
//! about 3% of other angles, 45 degrees among them (`sin`: UCRT `0.7071067811865476`, libm
//! `0.7071067811865475`). The test `crt_golden_bits` pins measured UCRT values. Whether to follow
//! UCRT, libm or x87 for those angles is for the CLONE_SPEC and the Phase 2 cross-check.

use super::{Real, math};

/// Degrees to radians factor, `PI / 180` rounded to f64. Rust's `f64::to_radians` uses the same
/// constant. Multiplying by it differs from `x * PI / 180` for about a quarter of angles.
///
/// OPEN: confirm in CLONE_SPEC which form GM8 (and OpenGMK's motion code) uses.
pub const DEG_TO_RAD: f64 = core::f64::consts::PI / 180.0;

/// Radians to degrees factor, `180 / PI` rounded to f64.
///
/// OPEN: confirm in CLONE_SPEC which form GM8 uses (`x * (180 / PI)` or `x * 180 / PI`).
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

/// 0 degrees: sin `0.0`, cos `1.0`. Source: f64 `sin`/`cos` of 0 radians (exact; OpenGMK plain
/// f64 model; libm and Windows x64 UCRT agree). Not yet checked against a trace.
pub const TRIG_0: TrigEntry = TrigEntry {
    degrees: Real::new(0.0),
    sin: Real::from_bits(0x0000_0000_0000_0000),
    cos: Real::from_bits(0x3FF0_0000_0000_0000),
};

/// 90 degrees: sin `1.0`, cos `6.123233995736766e-17`. Source: f64 `sin`/`cos` of
/// `90 * DEG_TO_RAD` = `1.5707963267948966` (OpenGMK plain f64 model; libm and Windows x64
/// UCRT agree). Not yet checked against a trace.
pub const TRIG_90: TrigEntry = TrigEntry {
    degrees: Real::new(90.0),
    sin: Real::from_bits(0x3FF0_0000_0000_0000),
    cos: Real::from_bits(0x3C91_A626_3314_5C07),
};

/// 180 degrees: sin `1.2246467991473532e-16`, cos `-1.0`. Source: f64 `sin`/`cos` of
/// `180 * DEG_TO_RAD` = `3.141592653589793` (OpenGMK plain f64 model; libm and Windows x64
/// UCRT agree). Not yet checked against a trace.
pub const TRIG_180: TrigEntry = TrigEntry {
    degrees: Real::new(180.0),
    sin: Real::from_bits(0x3CA1_A626_3314_5C07),
    cos: Real::from_bits(0xBFF0_0000_0000_0000),
};

/// 270 degrees: sin `-1.0`, cos `-1.8369701987210297e-16`. Source: OpenGMK trace
/// `research/specs/engines/gm8/traces/rely-the-needle-4-2/djump.csv` (also `kaenbyou/`): with
/// gravity 0.4 and gravity_direction 270, frame 2 has hspeed `0.4 * cos = -7.347880794884119e-17`
/// and vspeed `0.4`. Both equal f64 `sin`/`cos` of `270 * DEG_TO_RAD` = `4.71238898038469`.
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
/// OPEN: confirm in CLONE_SPEC the formula and the normalization (when it happens and whether
/// it is done in degrees or radians).
pub fn point_direction(x1: Real, y1: Real, x2: Real, y2: Real) -> Real {
    let d = darctan2(y1 - y2, x2 - x1);
    if d.get() < 0.0 { d + Real::new(360.0) } else { d }
}

/// GML `point_distance`: `sqrt(dx * dx + dy * dy)`. Not `hypot`, which rounds differently.
///
/// OPEN: confirm in CLONE_SPEC that GM8 uses this plain formula.
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
