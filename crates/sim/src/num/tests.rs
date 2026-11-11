//! Unit tests for `num`: rounding, comparisons, GML `mod`/`div`, libm values, the trig table
//! and the trace value it exists for.

use super::trig::{DEG_TO_RAD, TRIG_TABLE};
use super::*;
use crate::rules::NumRules;
use crate::rules::num::{AlarmConv, ByteConv, CompareMode, MinMaxTies, NegZero, SignMode, StrRound, Truth};

fn r(x: f64) -> Real {
    Real::new(x)
}

/// Parses a decimal the way trace files write it.
fn parse(text: &str) -> Real {
    r(text.parse::<f64>().expect("test literal parses"))
}

// ---- Real ----

#[test]
fn real_equality_is_bitwise() {
    assert_ne!(r(0.0), r(-0.0));
    assert_eq!(r(f64::NAN), r(f64::NAN));
    assert_eq!(r(0.1) + r(0.2), r(0.30000000000000004));
}

#[test]
fn real_canonical_bits_unify_nans() {
    let payload_nan = Real::from_bits(0x7FF8_0000_0000_1234);
    assert!(payload_nan.is_nan());
    assert_eq!(payload_nan.canonical_bits(), Real::CANONICAL_NAN_BITS);
    assert_eq!(
        Real::from_bits(0xFFF8_0000_0000_0000).canonical_bits(),
        Real::CANONICAL_NAN_BITS
    );
    assert_eq!(r(-0.0).canonical_bits(), 0x8000_0000_0000_0000);
}

#[test]
fn real_display_is_shortest_round_trip() {
    for (value, text) in [
        (49.0, "49.0"),
        (0.4, "0.4"),
        (0.6000000000000001, "0.6000000000000001"),
        (-7.347880794884119e-17, "-7.347880794884119e-17"),
        (-0.0, "-0.0"),
        (1e16, "1e16"),
        (0.0001, "0.0001"),
    ] {
        assert_eq!(r(value).to_string(), text);
        assert_eq!(parse(text), r(value));
    }
}

#[test]
fn real_arithmetic_is_plain_f64() {
    let mut x = r(1.0);
    x += r(0.5);
    x *= r(4.0);
    x -= r(1.0);
    x /= r(2.0);
    assert_eq!(x, r(2.5));
    assert_eq!(-x, r(-2.5));
    assert_eq!(r(1.0) / r(3.0), r(0.3333333333333333));
}

#[test]
fn real_serializes_as_f64() {
    let bytes = postcard::to_allocvec(&r(-7.347880794884119e-17)).expect("serializes");
    assert_eq!(bytes, (-7.347880794884119e-17f64).to_le_bytes());
    let back: Real = postcard::from_bytes(&bytes).expect("deserializes");
    assert_eq!(back, r(-7.347880794884119e-17));
}

#[test]
fn real_serializes_nan_canonically() {
    let payload_nan = Real::from_bits(0xFFF8_0000_0000_1234);
    let bytes = postcard::to_allocvec(&payload_nan).expect("serializes");
    assert_eq!(bytes, Real::CANONICAL_NAN_BITS.to_le_bytes());
    // -0.0 is not a NaN and keeps its sign.
    let bytes = postcard::to_allocvec(&r(-0.0)).expect("serializes");
    assert_eq!(bytes, (-0.0f64).to_le_bytes());
}

#[test]
#[allow(
    clippy::disallowed_types,
    reason = "tests Real's Hash impl itself, which only test maps use; state hashing never does"
)]
fn real_hashes_nan_canonically() {
    use core::hash::{Hash, Hasher};
    let hash = |x: Real| {
        let mut h = std::hash::DefaultHasher::new();
        x.hash(&mut h);
        h.finish()
    };
    let canonical = Real::from_bits(Real::CANONICAL_NAN_BITS);
    assert_eq!(hash(Real::from_bits(0x7FF8_0000_0000_1234)), hash(canonical));
    assert_eq!(hash(Real::from_bits(0xFFF8_0000_0000_0000)), hash(canonical));
    assert_ne!(hash(r(0.0)), hash(r(-0.0)));
}

// ---- Rounding and conversion ----

/// The engine_rev 0 number rules.
fn d() -> NumRules {
    NumRules::default()
}

/// The engine_rev 0 compare epsilon.
const EPSILON: f64 = 1e-13;

#[test]
fn round_is_bankers() {
    let d = &d();
    assert_eq!(round(r(0.5), d), r(0.0));
    assert_eq!(round(r(1.5), d), r(2.0));
    assert_eq!(round(r(2.5), d), r(2.0));
    assert_eq!(round(r(3.5), d), r(4.0));
    // Zero results are +0.0 under the default N4 rule.
    assert_eq!(round(r(-0.5), d), r(0.0));
    assert_eq!(round(r(-0.3), d), r(0.0));
    assert_eq!(round(r(-0.0), d), r(0.0));
    assert_eq!(round(r(-1.5), d), r(-2.0));
    assert_eq!(round(r(-2.5), d), r(-2.0));
    assert_eq!(round(r(0.49999999999999994), d), r(0.0));
    assert_eq!(round(r(2.5000000000000004), d), r(3.0));
}

#[test]
fn round_neg_zero_rule() {
    let keep = NumRules {
        round_neg_zero: NegZero::Keep,
        ..d()
    };
    assert_eq!(round(r(-0.4), &keep), r(-0.0));
    assert_eq!(round(r(-0.0), &keep), r(-0.0));
    assert_eq!(round(r(0.4), &keep), r(0.0));
}

#[test]
fn to_int_round_is_bankers() {
    assert_eq!(to_int_round(r(0.5)), Ok(0));
    assert_eq!(to_int_round(r(1.5)), Ok(2));
    assert_eq!(to_int_round(r(2.5)), Ok(2));
    assert_eq!(to_int_round(r(-0.5)), Ok(0));
    assert_eq!(to_int_round(r(-1.5)), Ok(-2));
}

#[test]
fn to_int_trunc_goes_toward_zero() {
    assert_eq!(to_int_trunc(r(1.7)), Ok(1));
    assert_eq!(to_int_trunc(r(-1.7)), Ok(-1));
    assert_eq!(to_int_trunc(-TWO_POW_63), Ok(i64::MIN));
}

/// 2^63, the first f64 above i64::MAX.
const TWO_POW_63: Real = Real::from_bits(0x43E0_0000_0000_0000);

#[test]
fn int_conversion_errors_instead_of_saturating() {
    assert_eq!(to_int_trunc(r(f64::NAN)), Err(NumError::NotFinite(r(f64::NAN))));
    assert_eq!(
        to_int_round(r(f64::INFINITY)),
        Err(NumError::NotFinite(r(f64::INFINITY)))
    );
    assert_eq!(to_int_trunc(TWO_POW_63), Err(NumError::OutOfRange(TWO_POW_63)));
    assert_eq!(to_int_round(r(-1e19)), Err(NumError::OutOfRange(r(-1e19))));
}

#[test]
fn floor_ceil_frac_sign_abs() {
    let d = &d();
    assert_eq!(floor(r(-1.5), d), r(-2.0));
    assert_eq!(ceil(r(-1.5), d), r(-1.0));
    assert_eq!(frac(r(2.75)), r(0.75));
    assert_eq!(frac(r(-1.25)), r(-0.25));
    // floor and ceil keep IEEE signed zeros under the default N13 rule.
    assert_eq!(ceil(r(-0.5), d), r(-0.0));
    assert_eq!(floor(r(-0.0), d), r(-0.0));
    assert_eq!(sign(r(3.2), d), r(1.0));
    assert_eq!(sign(r(-0.1), d), r(-1.0));
    assert_eq!(sign(r(0.0), d), r(0.0));
    assert_eq!(sign(r(-0.0), d), r(0.0));
    assert_eq!(sign(r(f64::NAN), d), r(0.0));
    assert_eq!(abs(r(-2.0)), r(2.0));
}

#[test]
fn floor_ceil_zero_rule() {
    let positive = NumRules {
        floor_ceil_zero: NegZero::Positive,
        ..d()
    };
    assert_eq!(ceil(r(-0.5), &positive), r(0.0));
    assert_eq!(floor(r(-0.0), &positive), r(0.0));
    assert_eq!(floor(r(-1.5), &positive), r(-2.0));
}

/// The default sign uses the comparison tolerance, so the gravity drift on trace frames 2-4 has
/// sign 0.
#[test]
fn sign_uses_epsilon() {
    let d = &d();
    assert_eq!(sign(parse("-7.347880794884119e-17"), d), r(0.0));
    assert_eq!(sign(parse("7.347880794884119e-17"), d), r(0.0));
    assert_eq!(sign(r(2e-13), d), r(1.0));
    assert_eq!(sign(r(-2e-13), d), r(-1.0));
    assert_eq!(sign(r(EPSILON), d), r(1.0));
    assert_eq!(sign(r(-EPSILON), d), r(-1.0));
    assert_eq!(sign(r(0.99 * EPSILON), d), r(0.0));
}

/// Exact sign, or an exact compare mode, gives the drift its sign (N11 follows N1).
#[test]
fn sign_exact_rules() {
    let exact_sign = NumRules {
        sign: SignMode::Exact,
        ..d()
    };
    let exact_compare = NumRules {
        compare: CompareMode::Exact,
        ..d()
    };
    for rules in [&exact_sign, &exact_compare] {
        assert_eq!(sign(parse("-7.347880794884119e-17"), rules), r(-1.0));
        assert_eq!(sign(parse("5.551115123125783e-17"), rules), r(1.0));
        assert_eq!(sign(r(-0.0), rules), r(0.0));
        assert_eq!(sign(r(f64::NAN), rules), r(0.0));
    }
}

#[test]
fn min_max_clamp_lerp() {
    let d = &d();
    assert_eq!(gm_min(r(1.0), r(2.0), d), r(1.0));
    assert_eq!(gm_max(r(1.0), r(2.0), d), r(2.0));
    // min keeps `a` unless `a > b`; max takes `b` unless `a > b`.
    assert_eq!(gm_min(r(0.0), r(-0.0), d), r(0.0));
    assert_eq!(gm_min(r(-0.0), r(0.0), d), r(-0.0));
    assert_eq!(gm_max(r(-0.0), r(0.0), d), r(0.0));
    assert_eq!(gm_max(r(0.0), r(-0.0), d), r(-0.0));
    assert_eq!(gm_max(r(f64::NAN), r(1.0), d), r(1.0));
    assert!(gm_max(r(1.0), r(f64::NAN), d).is_nan());
    assert!(gm_min(r(f64::NAN), r(1.0), d).is_nan());
    assert_eq!(gm_min(r(1.0), r(f64::NAN), d), r(1.0));
    assert_eq!(clamp(r(5.0), r(0.0), r(3.0)), r(3.0));
    assert_eq!(clamp(r(-1.0), r(0.0), r(3.0)), r(0.0));
    assert_eq!(clamp(r(2.0), r(0.0), r(3.0)), r(2.0));
    assert_eq!(lerp(r(0.0), r(10.0), r(0.25)), r(2.5));
    assert_eq!(lerp(r(0.1), r(0.3), r(1.0)), r(0.1) + (r(0.3) - r(0.1)));
}

#[test]
fn min_max_ties_rule() {
    let ge = NumRules {
        min_max_ties: MinMaxTies::GreaterOrEqual,
        ..d()
    };
    assert_eq!(gm_min(r(0.0), r(-0.0), &ge), r(-0.0));
    assert_eq!(gm_max(r(0.0), r(-0.0), &ge), r(0.0));
    assert_eq!(gm_min(r(1.0), r(2.0), &ge), r(1.0));
    assert_eq!(gm_max(r(1.0), r(2.0), &ge), r(2.0));
}

#[test]
fn repeat_count_rounds_and_stops_at_zero() {
    let d = &d();
    assert_eq!(gm_repeat_count(r(2.5), d), Ok(2));
    assert_eq!(gm_repeat_count(r(3.5), d), Ok(4));
    assert_eq!(gm_repeat_count(r(0.5), d), Ok(0));
    assert_eq!(gm_repeat_count(r(-3.0), d), Ok(0));
    assert_eq!(
        gm_repeat_count(r(f64::NAN), d),
        Err(NumError::NotFinite(r(f64::NAN)))
    );
}

#[test]
fn gm_mod_keeps_dividend_sign() {
    assert_eq!(gm_mod(r(7.0), r(3.0)), Ok(r(1.0)));
    assert_eq!(gm_mod(r(-7.0), r(3.0)), Ok(r(-1.0)));
    assert_eq!(gm_mod(r(7.0), r(-3.0)), Ok(r(1.0)));
    assert_eq!(gm_mod(r(-7.0), r(-3.0)), Ok(r(-1.0)));
    assert_eq!(gm_mod(r(5.5), r(2.0)), Ok(r(1.5)));
    // An exact multiple gives +0.0, not fmod's -0.0.
    assert_eq!(gm_mod(r(-6.0), r(3.0)), Ok(r(0.0)));
    assert_eq!(gm_mod(r(-32.0), r(32.0)), Ok(r(0.0)));
    assert_eq!(gm_mod(r(-0.0), r(3.0)), Ok(r(0.0)));
    assert_eq!(gm_mod(r(1.0), r(0.0)), Err(NumError::DivisionByZero));
    assert_eq!(gm_mod(r(1.0), r(-0.0)), Err(NumError::DivisionByZero));
}

/// `a - trunc(a / b) * b` with each step rounded, not exact fmod.
#[test]
fn gm_mod_is_the_rounded_formula_not_fmod() {
    assert_eq!(gm_mod(r(1.0), r(0.1)), Ok(r(0.0)));
    assert_eq!(libm::fmod(1.0, 0.1), 0.09999999999999995);
    let big = r(1e17 + 16.0);
    assert_eq!(gm_mod(big, r(3.0)), Ok(r(0.0)));
    assert_eq!(libm::fmod(big.get(), 3.0), 2.0);
}

/// N8: div truncates toward zero (Confirmed, CLONE_SPEC numbers: `-7 div 2 = -3`).
#[test]
fn gm_div_truncates_toward_zero() {
    for (a, b, q) in [
        (7.0, 2.0, 3.0),
        (-7.0, 2.0, -3.0),
        (7.0, -2.0, -3.0),
        (-7.0, -2.0, 3.0),
        (-33.0, 32.0, -1.0),
        (7.5, 2.0, 3.0),
        (-7.5, 2.0, -3.0),
        (1.0, 3.0, 0.0),
        (18059.0, 3600.0, 5.0),
        // Recorder, positive operands (traces/little-runner/num-divmod-save-a.csv row 60).
        (24.200000000000045, 1.0, 24.0),
        (7.7000000000000455, 1.0, 7.0),
        (168.60000000000002, 1.0, 168.0),
    ] {
        assert_eq!(gm_div(r(a), r(b)), Ok(r(q)), "{a} div {b}");
    }
    // A quotient between -1 and 0 truncates to -0.0, the IEEE result of trunc.
    assert_eq!(gm_div(r(-1.0), r(3.0)), Ok(r(-0.0)));
    assert_eq!(gm_div(r(0.0), r(0.0)), Err(NumError::DivisionByZero));
    assert_eq!(gm_div(r(1.0), r(-0.0)), Err(NumError::DivisionByZero));
}

/// N8: mod is `a - (a div b) * b`, so it keeps the dividend's sign (Confirmed formula).
#[test]
fn gm_mod_table_follows_truncated_div() {
    for (a, b, m) in [
        (-7.0, 2.0, -1.0),
        (7.0, -2.0, 1.0),
        (-7.0, -2.0, -1.0),
        (-33.0, 32.0, -1.0),
        (-7.5, 2.0, -1.5),
        // Recorder, positive operands (num-divmod-save-a.csv, -b.csv row 60).
        (280.20000000000005, 256.0, 24.200000000000045),
        (263.70000000000005, 256.0, 7.7000000000000455),
        (936.6, 256.0, 168.60000000000002),
    ] {
        assert_eq!(gm_mod(r(a), r(b)), Ok(r(m)), "{a} mod {b}");
        let q = gm_div(r(a), r(b)).expect("nonzero divisor");
        assert_eq!(gm_mod(r(a), r(b)), Ok(r(a) - q * r(b)), "{a} mod {b} formula");
    }
}

// ---- Comparisons ----

#[test]
fn gm_comparisons_use_epsilon() {
    let d = &d();
    let sum = r(0.1) + r(0.2);
    assert!(gm_eq(sum, r(0.3), d));
    assert!(!gm_ne(sum, r(0.3), d));
    assert!(!gm_lt(r(0.3), sum, d));
    assert!(gm_le(r(0.3), sum, d));
    assert!(!gm_gt(sum, r(0.3), d));
    assert!(gm_ge(r(0.3), sum, d));

    // The bound is exclusive: a difference of exactly EPSILON is not equal.
    let (zero, eps) = (r(0.0), r(EPSILON));
    assert!(!gm_eq(zero, eps, d) && gm_ne(zero, eps, d));
    assert!(gm_lt(zero, eps, d) && gm_le(zero, eps, d));
    assert!(!gm_gt(zero, eps, d) && !gm_ge(zero, eps, d));
    assert!(gm_gt(eps, zero, d) && gm_ge(eps, zero, d));
    assert!(!gm_lt(eps, zero, d) && !gm_le(eps, zero, d));
    // Just inside the bound is equal.
    let inside = r(0.99 * EPSILON);
    assert!(gm_eq(zero, inside, d) && !gm_ne(zero, inside, d));
    assert!(!gm_lt(zero, inside, d) && gm_le(zero, inside, d));
    assert!(!gm_gt(inside, zero, d) && gm_ge(inside, zero, d));

    // NaN: every comparison is false, != included.
    let nan = r(f64::NAN);
    assert!(!gm_eq(nan, nan, d));
    assert!(!gm_ne(nan, nan, d));
    assert!(!gm_ne(nan, r(0.0), d));
    assert!(!gm_lt(nan, r(0.0), d) && !gm_le(nan, r(0.0), d));
    assert!(!gm_gt(nan, r(0.0), d) && !gm_ge(nan, r(0.0), d));
}

#[test]
fn gm_comparisons_exact_mode() {
    let x = &NumRules {
        compare: CompareMode::Exact,
        ..d()
    };
    let sum = r(0.1) + r(0.2);
    assert!(!gm_eq(sum, r(0.3), x) && gm_ne(sum, r(0.3), x));
    assert!(!gm_lt(sum, r(0.3), x) && !gm_le(sum, r(0.3), x));
    assert!(gm_gt(sum, r(0.3), x) && gm_ge(sum, r(0.3), x));
    assert!(gm_eq(r(0.0), r(-0.0), x));
    let nan = r(f64::NAN);
    assert!(!gm_eq(nan, nan, x) && !gm_ne(nan, r(0.0), x));
    assert!(!gm_lt(nan, r(0.0), x) && !gm_ge(nan, r(0.0), x));
}

#[test]
fn gm_bool_threshold() {
    let d = &d();
    assert!(!gm_bool(r(0.0), d));
    assert!(!gm_bool(r(-1.0), d));
    assert!(!gm_bool(r(0.49999999999999994), d));
    assert!(gm_bool(r(0.5), d));
    assert!(gm_bool(r(1.0), d));
    assert!(!gm_bool(r(f64::NAN), d));
    let gt = NumRules {
        truth: Truth::GtHalf,
        ..NumRules::default()
    };
    assert!(!gm_bool(r(0.5), &gt));
    assert!(gm_bool(r(0.5000000000000001), &gt));
}

// ---- Conversions (R3b, N5, N6) ----

/// R3b default: round half to even, as the recorder (traces/kaenbyou/num-alarm-half-b.csv).
#[test]
fn alarm_from_real_rounds_half_even() {
    let d = &d();
    for (v, a) in [
        (1.5, 2.0),
        (2.5, 2.0),
        (0.5, 0.0),
        (3.5, 4.0),
        (1.3, 1.0),
        (2.0, 2.0),
        (-0.4, 0.0),
        (-1.5, -2.0),
    ] {
        assert_eq!(alarm_from_real(r(v), d), r(a), "alarm {v}");
    }
    assert!(alarm_from_real(r(f64::NAN), d).is_nan());
}

#[test]
fn alarm_from_real_other_rules() {
    let trunc = NumRules {
        alarm_from_real: AlarmConv::Truncate,
        ..d()
    };
    let keep = NumRules {
        alarm_from_real: AlarmConv::Keep,
        ..d()
    };
    assert_eq!(alarm_from_real(r(1.5), &trunc), r(1.0));
    assert_eq!(alarm_from_real(r(-0.5), &trunc), r(0.0));
    assert_eq!(alarm_from_real(r(1.3), &keep), r(1.3));
}

/// N5 default: round half to even, then wrap (rounding Confirmed (recorder): num-byte-save-a..d,
/// tr-room-05-a/b/e, sh-save-restart-a/b).
#[test]
fn byte_from_real_rounds_half_even() {
    let d = &d();
    assert_eq!(d.byte_from_real, ByteConv::RoundWrap);
    for (v, b) in [
        (51.59999999999991, 52),
        (59.3, 59),
        (44.3, 44),
        (67.4, 67),
        (38.93, 39),
        (36.845, 37),
        (48.5, 48),
        (49.5, 50),
        (0.0, 0),
        (255.0, 255),
        // Out of range wraps mod 256 (OPEN).
        (256.0, 0),
        (455.1 - 256.0 * 1.0, 199),
        (-1.0, 255),
        (-0.4, 0),
    ] {
        assert_eq!(byte_from_real(r(v), d), b, "byte {v}");
    }
    assert_eq!(byte_from_real(r(f64::NAN), d), 0);
    assert_eq!(byte_from_real(r(f64::INFINITY), d), 0);
    assert_eq!(byte_from_real(r(1e300), d), 0);
}

#[test]
fn byte_from_real_trunc_rule() {
    let t = NumRules {
        byte_from_real: ByteConv::TruncWrap,
        ..d()
    };
    assert_eq!(byte_from_real(r(51.59999999999991), &t), 51);
    assert_eq!(byte_from_real(r(67.4), &t), 67);
    assert_eq!(byte_from_real(r(48.5), &t), 48);
    assert_eq!(byte_from_real(r(-1.5), &t), 255);
    assert_eq!(byte_from_real(r(257.9), &t), 1);
}

/// N6: integers get no decimals, everything else two. Recorder values the default matches
/// (num-string-save-a/b, tr-room-07-b/c), plus the two it does not (N6 OPEN).
#[test]
fn gm_string_real_table() {
    let d = &d();
    for (v, s) in [
        (1370.0, "1370"),
        (551.7, "551.70"),
        (551.6999999999999, "551.70"),
        (0.125, "0.12"),
        (0.375, "0.38"),
        (0.625, "0.62"),
        (0.875, "0.88"),
        (183.125, "183.12"),
        (183.005, "183.00"),
        (0.015, "0.01"),
        (-1.5, "-1.50"),
        (-0.125, "-0.12"),
        (9.995, "9.99"),
        (9.999, "10.00"),
        (0.0, "0"),
        (-0.0, "0"),
        (-3.0, "-3"),
        (1e20, "100000000000000000000"),
    ] {
        assert_eq!(gm_string_real(r(v), d), s, "string({v})");
    }
    // N6 OPEN: the recorder writes 2.68 and 1.01 for these.
    assert_eq!(gm_string_real(r(2.675), d), "2.67");
    assert_eq!(gm_string_real(r(1.005), d), "1.00");
}

#[test]
fn gm_string_real_half_away_rule() {
    let away = NumRules {
        string_decimals: StrRound::HalfAway,
        ..d()
    };
    assert_eq!(gm_string_real(r(0.125), &away), "0.13");
    assert_eq!(gm_string_real(r(0.375), &away), "0.38");
    assert_eq!(gm_string_real(r(-0.125), &away), "-0.13");
    assert_eq!(gm_string_real(r(9.875), &away), "9.88");
    assert_eq!(gm_string_real(r(99.995), &away), gm_string_real(r(99.995), &d()));
    assert_eq!(gm_string_real(r(551.7), &away), "551.70");
}

// ---- GML literals (N14) ----

#[test]
fn gml_literal_uses_the_recorder_model() {
    // One ulp above the nearest double (research#104 scan; CLONE_SPEC N14).
    for (text, bits) in [
        ("0.95", 0.950_000_000_000_000_1),
        ("0.3", 0.300_000_000_000_000_04),
        ("0.7", 0.700_000_000_000_000_1),
        ("0.6", 0.600_000_000_000_000_1),
        ("1.7", 1.700_000_000_000_000_2),
        ("474.4", 474.400_000_000_000_03),
    ] {
        let v = gml_literal(text).expect("literal");
        assert_eq!(v.get().to_bits(), f64::to_bits(bits), "{text}");
        assert_ne!(
            v.get().to_bits(),
            parse(text).get().to_bits(),
            "{text} differs from the nearest double"
        );
    }
    // The nearest double (CLONE_SPEC N14 checked constants).
    for text in [
        "0.4", "0.45", "0.017", "0.2", "2.5001", "183.125", "3", "3.", ".5",
    ] {
        let v = gml_literal(text).expect("literal");
        assert_eq!(v.get().to_bits(), parse(text).get().to_bits(), "{text}");
    }
    for text in ["", ".", "-0.3", "1e3", "1.2.3", "0x10"] {
        assert!(gml_literal(text).is_none(), "{text:?}");
    }
}

// ---- libm ----

#[test]
fn libm_exact_values() {
    use core::f64::consts::{FRAC_PI_4, LN_2, PI, SQRT_2};
    assert_eq!(sin(r(0.0)), r(0.0));
    assert_eq!(cos(r(0.0)), r(1.0));
    assert_eq!(tan(r(0.0)), r(0.0));
    assert_eq!(atan2(r(1.0), r(1.0)), r(FRAC_PI_4));
    assert_eq!(atan2(r(0.0), r(-1.0)), r(PI));
    assert_eq!(sqrt(r(4.0)), r(2.0));
    assert_eq!(sqrt(r(2.0)), r(SQRT_2));
    assert_eq!(pow(r(2.0), r(10.0)), r(1024.0));
    assert_eq!(pow(r(2.0), r(0.5)), r(SQRT_2));
    assert_eq!(pow(r(10.0), r(-2.0)), r(0.01));
    assert_eq!(exp(r(0.0)), r(1.0));
    assert_eq!(ln(r(1.0)), r(0.0));
    assert_eq!(ln(r(2.0)), r(LN_2));
    assert_eq!(ln(r(0.0)), r(f64::NEG_INFINITY));
}

/// Pins libm 0.2.16 output bits. A libm upgrade that changes one of these changes outcomes and
/// needs a SIM_VERSION bump.
#[test]
fn libm_golden_bits() {
    assert_eq!(sin(r(1.0)), r(0.8414709848078965));
    assert_eq!(cos(r(1.0)), r(0.5403023058681398));
    assert_eq!(tan(r(1.0)), r(1.5574077246549023));
    // One ulp above the correctly rounded e (2.718281828459045): libm is not correctly rounded.
    assert_eq!(exp(r(1.0)), r(2.7182818284590455));
    assert_eq!(dsin(r(30.0)), r(0.49999999999999994));
    assert_eq!(dcos(r(60.0)), r(0.5000000000000001));
    assert_eq!(dsin(r(45.0)), r(0.7071067811865475));
    assert_eq!(dcos(r(45.0)), r(core::f64::consts::FRAC_1_SQRT_2));
}

// ---- Degree trig ----

/// The value docs/architecture.md names: gravity 0.4 at gravity_direction 270 gives this hspeed
/// in the recorder traces, bit for bit (M6; `traces/showdown/t-mot-03-a.csv`, `kaenbyou/stand.csv`).
#[test]
fn gravity_270_matches_trace_value() {
    let hspeed = r(0.4) * dcos(r(270.0));
    assert_eq!(hspeed, parse("-7.347880794884119e-17"));
    assert_eq!(hspeed.to_bits(), 0xBC95_2DC7_707E_D4D5);
    assert_eq!(lengthdir_x(r(0.4), r(270.0)), hspeed);
    assert_eq!(r(0.0) - r(0.4) * dsin(r(270.0)), r(0.4));
    assert_eq!(lengthdir_y(r(0.4), r(270.0)), r(0.4));
    // gravity_direction -270 leaves the same residue (t-mot-03-b.csv).
    assert_eq!(r(0.4) * dcos(r(-270.0)), hspeed);
}

/// Regression guard: the table entries equal libm on the radian value, and dsin/dcos return
/// them. Not a fidelity check (libm against libm); `m6_recorder_bits` and the trace test are.
#[test]
fn table_entries_reproduce_f64_trig_of_the_radian() {
    for entry in TRIG_TABLE {
        let radians = entry.degrees.get() * DEG_TO_RAD;
        assert_eq!(entry.sin, r(libm::sin(radians)), "sin {}", entry.degrees);
        assert_eq!(entry.cos, r(libm::cos(radians)), "cos {}", entry.degrees);
        assert_eq!(dsin(entry.degrees), entry.sin);
        assert_eq!(dcos(entry.degrees), entry.cos);
    }
}

/// Property-style regression guard for `lookup`: for many angles not in the table, dsin/dcos
/// are exactly libm on degtorad. Off the table that is the implementation itself, so this only
/// fails if `lookup` wrongly matches a non-table angle. It says nothing about fidelity; see
/// `m6_recorder_bits`.
#[test]
fn dsin_dcos_match_libm_off_table() {
    let in_table = |d: Real| TRIG_TABLE.iter().any(|e| e.degrees == d);
    let check = |d: Real| {
        if in_table(d) {
            return;
        }
        let radians = d.get() * DEG_TO_RAD;
        assert_eq!(dsin(d), r(libm::sin(radians)), "dsin {d}");
        assert_eq!(dcos(d), r(libm::cos(radians)), "dcos {d}");
    };

    // Every quarter degree in [-1080, 1080], including table angles plus whole turns.
    for i in -4320..=4320 {
        check(r(f64::from(i) * 0.25));
    }
    // Negative zero is not the table's 0.
    check(r(-0.0));
    assert_eq!(dsin(r(-0.0)), r(-0.0));

    // Pseudo-random angles in [-10000, 10000) from a fixed xorshift64 seed.
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..100_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
        check(r(unit * 20_000.0 - 10_000.0));
    }
}

/// Ten aimed shots from `traces/little-runner/num-trig-a.csv` (CLONE_SPEC numbers, Confirmed
/// (recorder); hex values from `num-trig.md`): `(dx, dy, agl, hspeed, vspeed, speed,
/// direction)`. Each shot has `agl = atan2(dy, dx)`, `hspeed = 3 cos(agl)`, `vspeed = 3 sin(agl)`,
/// and speed and direction as read back from the vector.
#[rustfmt::skip]
const NUM_TRIG_SHOTS: [(f64, f64, u64, u64, u64, f64, f64); 10] = [
    (858.3879532441333, 117.46834325340092, 0x3FC1_688A_63C8_4953, 0x4007_C743_F516_0A48, 0x3FDA_0838_D6D7_FD9B, 3.0, 352.20761209861473),
    (950.7759064882666, 155.7366865068018, 0x3FC4_C823_C8B2_D961, 0x4007_AF33_1C94_6EE8, 0x3FDF_0932_3152_10B4, 3.0000000000000004, 350.6975843542303),
    (1043.1638597324, 194.00502976020266, 0x3FC7_8946_2DE9_06C9, 0x4007_986D_0AE1_1832, 0x3FE1_8D88_A71D_E559, 3.0, 349.4646368860251),
    (1129.5518129765333, 232.27337301360353, 0x3FC9_F58D_B6F4_E80D, 0x4007_8214_7D0E_2B6D, 0x3FE3_5611_91A0_8634, 3.0, 348.38006066107306),
    (1071.9397662206666, 270.5417162670044, 0x3FCF_A4FA_CC42_50C9, 0x4007_4532_7D09_994C, 0x3FE7_7E08_85CB_F39D, 2.9999999999999996, 345.83520674005064),
    (873.8980305337172, 308.8100595204053, 0x3FD5_BD3A_945A_D002, 0x4006_A0F3_78D4_89E8, 0x3FEF_FC3D_46A6_2A9B, 3.0, 340.5380783833871),
    (631.5100772895839, 347.07840277380615, 0x3FE0_14C7_AE8E_EEE8, 0x4005_0860_C94F_EA80, 0x3FF7_1E84_2BD5_66CE, 3.0, 331.2067725095146),
    (389.1221240454506, 333.30179920258183, 0x3FE6_AA41_B5EF_D5D8, 0x4002_3A3E_6382_5B52, 0x3FFF_39B9_5547_9FC6, 3.0, 319.41835698824417),
    (380.7341708013173, 295.03345594918096, 0x3FE5_1897_2ABE_C22E, 0x4002_F887_8C87_295B, 0x3FFD_66B6_E179_42FC, 3.0, 322.22765596940343),
    (438.346217557184, 256.7651126957801, 0x3FE0_F4CB_64C7_77B8, 0x4004_B573_E243_E790, 0x3FF8_42BE_7D72_9CAA, 3.0, 329.6399896292651),
];

/// Our `atan2(dy, dx)` minus the recorded `agl`, in ulps, per shot (M6 gaps).
const ATAN2_GAPS: [i128; 10] = [0; 10];

/// M6: the ten num-trig shots reproduce bit for bit with our libm: `atan2` (except shot 3, see
/// below), `cos`, `sin`, `sqrt(h*h + v*v)` (not `hypot`: shot 5 reads `2.9999999999999996`) and
/// `radtodeg(atan2(-v, h))` plus 360. This says nothing about the x87 runner (M6 OPEN).
///
/// Shots 3 and 10 are M6 gaps: our `atan2` is one ulp below the recorded `agl` (shot 3:
/// `0x3FC789462DE906C8` against `...C9`). The rest of each shot is checked from the recorded
/// `agl`.
#[test]
fn m6_recorder_bits() {
    let (o, three) = (r(0.0), r(3.0));
    for (i, (dx, dy, agl, h, v, speed, direction)) in NUM_TRIG_SHOTS.into_iter().enumerate() {
        let ours = atan2(r(dy), r(dx));
        let a = Real::from_bits(agl);
        let gap = i128::from(ours.to_bits()) - i128::from(agl);
        assert_eq!(gap, ATAN2_GAPS[i], "atan2 gap of shot {}", i + 1);
        let (h, v) = (Real::from_bits(h), Real::from_bits(v));
        assert_eq!(three * cos(a), h, "hspeed of ({dx}, {dy})");
        assert_eq!(three * sin(a), v, "vspeed of ({dx}, {dy})");
        assert_eq!(point_distance(o, o, h, v), r(speed), "speed of ({dx}, {dy})");
        assert_eq!(
            point_direction(o, o, h, v),
            r(direction),
            "direction of ({dx}, {dy})"
        );
    }
}

/// M6: off-table angles where our libm is one ulp away from the recorder. Pinned so a libm or
/// table change shows up; which bits are right for GM8 is OPEN (x87 vs libm). From
/// `traces/eat-the-rori-girl/t-mot-05-b.csv`: frame 4027 id 144489 has speed 16 at direction 284
/// and hspeed `3.870750329594679`; frame 3802 id 144474 has speed -2.5 at direction 45 and vspeed
/// `1.7677669529663689`.
#[test]
fn m6_recorder_gaps() {
    let ulps = |a: Real, b: f64| a.to_bits().abs_diff(b.to_bits());
    let h_284 = lengthdir_x(r(16.0), r(284.0));
    assert_eq!(h_284, r(3.8707503295946792));
    assert_eq!(ulps(h_284, 3.870750329594679), 1);
    assert_eq!(lengthdir_y(r(16.0), r(284.0)), r(15.524731620415945));
    let v_45 = lengthdir_y(r(-2.5), r(45.0));
    assert_eq!(v_45, r(1.7677669529663687));
    assert_eq!(ulps(v_45, 1.7677669529663689), 1);
    assert_eq!(lengthdir_x(r(-2.5), r(45.0)), r(-1.7677669529663689));
}

#[test]
fn degree_conversions() {
    use core::f64::consts::{FRAC_PI_2, PI};
    assert_eq!(degtorad(r(180.0)), r(PI));
    assert_eq!(degtorad(r(90.0)), r(FRAC_PI_2));
    assert_eq!(radtodeg(r(PI)), r(180.0));
    assert_eq!(darctan2(r(1.0), r(-1.0)), r(135.0));
    assert_eq!(darctan2(r(-1.0), r(0.0)), r(-90.0));
}

#[test]
fn point_direction_and_distance() {
    let o = r(0.0);
    assert_eq!(point_direction(o, o, r(1.0), o), r(0.0));
    assert_eq!(point_direction(o, o, o, r(-1.0)), r(90.0));
    assert_eq!(point_direction(o, o, r(-1.0), o), r(180.0));
    assert_eq!(point_direction(o, o, o, r(1.0)), r(270.0));
    assert_eq!(point_direction(o, o, r(-1.0), r(1.0)), r(225.0));
    assert_eq!(point_distance(o, o, r(3.0), r(4.0)), r(5.0));
    assert_eq!(point_distance(r(1.0), r(1.0), r(1.0), r(1.0)), r(0.0));

    // Trace frame 3: hspeed -7.347880794884119e-17, vspeed 0 gives speed and direction 180.
    let hspeed = parse("-7.347880794884119e-17");
    assert_eq!(point_direction(o, o, hspeed, o), r(180.0));
    assert_eq!(point_distance(o, o, hspeed, o), parse("7.347880794884119e-17"));
}

// ---- Traces (need IWBTG_RESEARCH) ----

/// Path to the GM8 traces, or None (and a note) when IWBTG_RESEARCH is unset.
#[allow(
    clippy::disallowed_methods,
    reason = "tests may read the research path; the sim itself never reads the environment"
)]
fn traces_dir() -> Option<std::path::PathBuf> {
    match std::env::var_os("IWBTG_RESEARCH") {
        Some(root) => Some(std::path::PathBuf::from(root).join("specs/engines/gm8/traces")),
        None => {
            eprintln!("IWBTG_RESEARCH is unset; skipping trace test");
            None
        }
    }
}

/// Columns that hold GM8 reals in every trace CSV.
const REAL_COLUMNS: [&str; 12] = [
    "x",
    "y",
    "xprevious",
    "yprevious",
    "hspeed",
    "vspeed",
    "speed",
    "direction",
    "gravity",
    "gravity_direction",
    "friction",
    "image_index",
];

/// Reads every trace CSV. For each real field: Display reproduces the recorder's text, and
/// wherever gravity is 0.4 at direction 270 and a zero hspeed became tiny but nonzero, it is our
/// dcos product (or that product added to the previous tick's residue).
#[test]
#[allow(clippy::disallowed_methods, reason = "test-only: reads research trace files")]
fn traces_match_display_and_gravity_trig() {
    let Some(dir) = traces_dir() else { return };
    let mut files = 0;
    let mut gravity_rows = 0;
    let games = std::fs::read_dir(&dir).expect("IWBTG_RESEARCH traces folder exists");
    for game in games {
        let game = game.expect("readable dir entry").path();
        let Ok(cases) = std::fs::read_dir(&game) else {
            continue;
        };
        for case in cases {
            let path = case.expect("readable dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("csv") {
                continue;
            }
            files += 1;
            let text = std::fs::read_to_string(&path).expect("readable trace");
            let mut lines = text.lines();
            let header: Vec<&str> = lines.next().expect("header").split(',').collect();
            let col = |name: &str| {
                header
                    .iter()
                    .position(|h| *h == name)
                    .unwrap_or_else(|| panic!("{} has column {name}", path.display()))
            };
            let real_cols: Vec<usize> = REAL_COLUMNS.iter().map(|c| col(c)).collect();
            let (id, hs, gr, gd) = (col("id"), col("hspeed"), col("gravity"), col("gravity_direction"));
            let residue = r(0.4) * dcos(r(270.0));
            // The previous row's hspeed per instance id.
            let mut previous: std::collections::BTreeMap<&str, Real> = Default::default();
            for line in lines {
                let fields: Vec<&str> = line.split(',').collect();
                for &c in &real_cols {
                    let field = fields[c];
                    assert_eq!(parse(field).to_string(), field, "{}", path.display());
                }
                let hspeed = parse(fields[hs]);
                let before = previous.insert(fields[id], hspeed);
                if fields[gr] == "0.4" && fields[gd] == "270.0" {
                    // A tiny nonzero hspeed is gravity's x part. When the previous row of the
                    // instance had hspeed 0 (or there is none), it must be exactly the residue.
                    // Otherwise the residue may sit on top of what game code left: nothing resets
                    // hspeed on slip blocks (traces/little-trap/num-eps-slip-b.csv), and probes
                    // inject values (num-eps-slip-c.csv), so those rows only count when they
                    // match `previous + residue`.
                    // -0.0 is zero too: a frozen kid reads hspeed -0 with gravity 0.4
                    // (kaenbyou/famb-gravflip.csv frames 40-54), so only a nonzero value counts.
                    let is_zero = |x: Real| x == r(0.0) || x == r(-0.0);
                    if !is_zero(hspeed) && hspeed.get().abs() < 1e-10 {
                        let from_zero = before.is_none_or(is_zero);
                        let accumulated = before.is_some_and(|b| hspeed == b + residue);
                        if hspeed == residue || accumulated {
                            gravity_rows += 1;
                        } else {
                            assert!(
                                !from_zero,
                                "{} id {}: hspeed {hspeed:?} after a zero hspeed",
                                path.display(),
                                fields[id]
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(files > 0, "no trace CSVs under {}", dir.display());
    assert!(gravity_rows > 0, "no gravity-270 rows found");
}

/// `atan2` with a tiny `x` is the double below pi/2 (not the angle `libm::atan2` gives):
/// `traces/rely-the-needle-4-2/djump.csv` frames 10, 24 and 26 read direction 90.0 with hspeed
/// `-7.347880794884119e-17` and vspeed -8.1, -2.4999999999999964 and -1.6999999999999964.
#[test]
fn atan2_with_a_tiny_x_is_the_double_nearest_pi_over_two() {
    use core::f64::consts::{FRAC_PI_2, FRAC_PI_4};
    let h = r(-7.347_880_794_884_119e-17);
    for v in [8.1, 2.499_999_999_999_996_4, 1.699_999_999_999_996_4] {
        assert_eq!(atan2(r(v), h), r(FRAC_PI_2), "v = {v}");
        assert_eq!(darctan2(r(v), h), r(90.0), "v = {v}");
        assert_eq!(atan2(r(-v), h), r(-FRAC_PI_2), "v = -{v}");
    }
    // Not tiny: the plain libm value.
    assert_eq!(atan2(r(1.0), r(1.0)), r(FRAC_PI_4));
}

/// `atan2` is correctly rounded for ordinary vectors: kaenbyou land-moving frame 10 (hspeed 3,
/// vspeed -8.1, direction `0x40516B51B9E71DBC`) and vines frame 22 (`29.827084654474746` is the
/// recorder's value; the pre-fix error was 2 ulp).
#[test]
fn atan2_is_correctly_rounded_for_ordinary_vectors() {
    let d = point_direction(r(0.0), r(0.0), r(3.0), r(-8.1));
    assert_eq!(d.to_bits(), 0x4051_6B51_B9E7_1DBC);
    assert_eq!(atan2(r(1.0), r(1.0)), r(core::f64::consts::FRAC_PI_4));
    assert_eq!(atan2(r(1.0), r(-1.0)), r(2.356_194_490_192_345));
    assert_eq!(atan2(r(-1.0), r(-1.0)), r(-2.356_194_490_192_345));
}

/// The recorder reads exactly 90 and 270 for a kid with the hspeed residue even at a ratio of
/// 2e-15 (kaenbyou famb-platform-a frame 50, rely num-rng-b up to 1.9e-14); an ordinary vector is
/// not snapped (#428).
#[test]
fn near_axis_directions_snap_to_the_axis() {
    let h = parse("-7.347880794884119e-17");
    let o = r(0.0);
    assert_eq!(point_direction(o, o, h, r(-0.03749999999999998)), r(90.0));
    assert_eq!(point_direction(o, o, h, r(0.3625)), r(270.0));
    assert_ne!(point_direction(o, o, r(1e-5), r(-1.0)), r(90.0));
}
