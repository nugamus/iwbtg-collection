//! Number handling for the sim (docs/architecture.md, "Number handling").
//!
//! Every real in built-ins is a [`Real`]: an IEEE binary64 double, like a GM8 real. Only exact
//! IEEE operations (`+ - * /`, `sqrt`, `floor`, `ceil`, `trunc`) run on hardware. Everything
//! transcendental goes through `libm` in software, so results are bit-identical on Windows,
//! WASM and Linux.
//!
//! Submodules:
//! - [`round`](mod@round): banker's rounding, integer conversion, `frac`, `sign`, `gm_min`/`gm_max`,
//!   `repeat` counts, GML `mod`/`div`.
//! - [`compare`]: GM8 comparisons and truthiness under `NumRules`.
//! - [`conv`]: rule-dependent conversions (alarms, save bytes, `string()`).
//! - [`math`]: software `sin`, `cos`, `tan`, `atan2`, `sqrt`, `pow`, `exp`, `ln`.
//! - [`trig`]: degree helpers, the exact-bits trig table, `point_direction` and friends.

pub mod compare;
pub mod conv;
pub mod math;
pub mod round;
pub mod trig;

#[cfg(test)]
mod tests;

use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize, Serializer};

pub use compare::{gm_bool, gm_eq, gm_ge, gm_gt, gm_le, gm_lt, gm_ne};
pub use conv::{alarm_from_real, byte_from_real, gm_string_real};
pub use math::{atan2, cos, exp, ln, pow, sin, sqrt, tan};
pub use round::{
    abs, ceil, clamp, floor, frac, gm_div, gm_max, gm_min, gm_mod, gm_repeat_count, lerp, round, sign,
    to_int_round, to_int_trunc,
};
pub use trig::{
    darctan2, dcos, degtorad, dsin, lengthdir_x, lengthdir_y, point_direction, point_distance, radtodeg,
};

/// A GM8 real: one IEEE binary64 double.
///
/// Arithmetic is plain f64 arithmetic. Equality and `Eq` compare the bit pattern, so `Real` can
/// key ordered maps; `0.0` and `-0.0` differ, and a NaN equals itself. For GML-style value
/// comparison use [`gm_eq`] and friends.
///
/// `Hash` and `Serialize` use [`Real::canonical_bits`], so every NaN hashes and serializes as
/// [`Real::CANONICAL_NAN_BITS`] whatever payload the platform produced. The state hash is BLAKE3
/// over the postcard encoding (plan 10.1); `Hash` is only for test maps. Deserializing keeps the
/// bits it reads.
#[derive(Clone, Copy, Default, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct Real(f64);

impl Real {
    /// `0.0`.
    pub const ZERO: Real = Real(0.0);
    /// `1.0`.
    pub const ONE: Real = Real(1.0);

    /// Bit pattern every NaN maps to in [`Real::canonical_bits`], `Hash` and `Serialize`. WASM
    /// NaN payloads are not deterministic, so the state hash must not see them.
    pub const CANONICAL_NAN_BITS: u64 = 0x7FF8_0000_0000_0000;

    /// Wraps an f64.
    pub const fn new(value: f64) -> Real {
        Real(value)
    }

    /// The f64 value.
    pub const fn get(self) -> f64 {
        self.0
    }

    /// Builds a real from its IEEE bit pattern, for constants a trace says must match bit for bit.
    pub const fn from_bits(bits: u64) -> Real {
        Real(f64::from_bits(bits))
    }

    /// The IEEE bit pattern.
    pub const fn to_bits(self) -> u64 {
        self.0.to_bits()
    }

    /// The bit pattern for the state hash: like [`Real::to_bits`], but every NaN gives
    /// [`Real::CANONICAL_NAN_BITS`].
    pub const fn canonical_bits(self) -> u64 {
        if self.0.is_nan() {
            Real::CANONICAL_NAN_BITS
        } else {
            self.0.to_bits()
        }
    }

    /// True unless NaN or infinite. Host setters reject non-finite values.
    pub const fn is_finite(self) -> bool {
        self.0.is_finite()
    }

    /// True for NaN.
    pub const fn is_nan(self) -> bool {
        self.0.is_nan()
    }
}

impl PartialEq for Real {
    fn eq(&self, other: &Real) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for Real {}

/// Hashes [`Real::canonical_bits`]. Consistent with bitwise `Eq`: equal bits give equal
/// canonical bits.
impl Hash for Real {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.canonical_bits().hash(state);
    }
}

/// Writes the f64 of [`Real::canonical_bits`], so NaN payloads never reach a serialized state.
impl Serialize for Real {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(f64::from_bits(self.canonical_bits()))
    }
}

impl From<f64> for Real {
    fn from(value: f64) -> Real {
        Real(value)
    }
}

impl From<Real> for f64 {
    fn from(value: Real) -> f64 {
        value.0
    }
}

/// Shortest round-trip decimal, the same text the trace recorder writes (`49.0`, `0.4`,
/// `-7.347880794884119e-17`). Parsing it back gives the same bits.
impl fmt::Display for Real {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Rust's f64 Debug is shortest round-trip, with an exponent below 1e-4 and from 1e16.
        write!(f, "{:?}", self.0)
    }
}

/// Like [`Display`](fmt::Display), plus the hex bits, so a test failure shows exact values.
impl fmt::Debug for Real {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Real({:?} = {:#018X})", self.0, self.0.to_bits())
    }
}

macro_rules! real_binop {
    ($trait:ident, $method:ident, $assign_trait:ident, $assign_method:ident, $op:tt) => {
        impl $trait for Real {
            type Output = Real;
            fn $method(self, rhs: Real) -> Real {
                Real(self.0 $op rhs.0)
            }
        }

        impl $assign_trait for Real {
            fn $assign_method(&mut self, rhs: Real) {
                self.0 = self.0 $op rhs.0;
            }
        }
    };
}

real_binop!(Add, add, AddAssign, add_assign, +);
real_binop!(Sub, sub, SubAssign, sub_assign, -);
real_binop!(Mul, mul, MulAssign, mul_assign, *);
real_binop!(Div, div, DivAssign, div_assign, /);

impl Neg for Real {
    type Output = Real;
    fn neg(self) -> Real {
        Real(-self.0)
    }
}

/// A number operation that has no valid result. The caller turns it into a script error or a
/// sim error; nothing in `num` panics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NumError {
    /// `div` or `mod` with a zero divisor. GM8 stops the game with an error here.
    #[error("division by zero")]
    DivisionByZero,
    /// A NaN or infinite real where an integer was needed.
    #[error("cannot convert {0} to an integer: not a finite number")]
    NotFinite(Real),
    /// A real outside the i64 range where an integer was needed.
    #[error("cannot convert {0} to an integer: out of range")]
    OutOfRange(Real),
}
