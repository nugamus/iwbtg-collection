//! Unit tests for `num`: rounding, comparisons, GML `mod`/`div`, libm values, the trig table
//! and the trace value it exists for.

use super::trig::{DEG_TO_RAD, TRIG_TABLE};
use super::*;

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

#[test]
fn round_is_bankers() {
    assert_eq!(round(r(0.5)), r(0.0));
    assert_eq!(round(r(1.5)), r(2.0));
    assert_eq!(round(r(2.5)), r(2.0));
    assert_eq!(round(r(3.5)), r(4.0));
    // Zero results are +0.0, never -0.0.
    assert_eq!(round(r(-0.5)), r(0.0));
    assert_eq!(round(r(-0.3)), r(0.0));
    assert_eq!(round(r(-0.0)), r(0.0));
    assert_eq!(round(r(-1.5)), r(-2.0));
    assert_eq!(round(r(-2.5)), r(-2.0));
    assert_eq!(round(r(0.49999999999999994)), r(0.0));
    assert_eq!(round(r(2.5000000000000004)), r(3.0));
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
    assert_eq!(floor(r(-1.5)), r(-2.0));
    assert_eq!(ceil(r(-1.5)), r(-1.0));
    assert_eq!(frac(r(2.75)), r(0.75));
    assert_eq!(frac(r(-1.25)), r(-0.25));
    // floor and ceil keep IEEE signed zeros (OPEN in their docs).
    assert_eq!(ceil(r(-0.5)), r(-0.0));
    assert_eq!(floor(r(-0.0)), r(-0.0));
    assert_eq!(sign(r(3.2)), r(1.0));
    assert_eq!(sign(r(-0.1)), r(-1.0));
    assert_eq!(sign(r(0.0)), r(0.0));
    assert_eq!(sign(r(-0.0)), r(0.0));
    assert_eq!(sign(r(f64::NAN)), r(0.0));
    assert_eq!(abs(r(-2.0)), r(2.0));
}

/// sign uses the comparison tolerance, so the gravity drift on trace frames 2-4 has sign 0.
#[test]
fn sign_uses_epsilon() {
    assert_eq!(sign(parse("-7.347880794884119e-17")), r(0.0));
    assert_eq!(sign(parse("7.347880794884119e-17")), r(0.0));
    assert_eq!(sign(r(2e-13)), r(1.0));
    assert_eq!(sign(r(-2e-13)), r(-1.0));
    assert_eq!(sign(r(EPSILON)), r(1.0));
    assert_eq!(sign(r(-EPSILON)), r(-1.0));
    assert_eq!(sign(r(0.99 * EPSILON)), r(0.0));
}

#[test]
fn min_max_clamp_lerp() {
    assert_eq!(min(r(1.0), r(2.0)), r(1.0));
    assert_eq!(max(r(1.0), r(2.0)), r(2.0));
    // min keeps `a` unless `a > b`; max takes `b` unless `a > b`.
    assert_eq!(min(r(0.0), r(-0.0)), r(0.0));
    assert_eq!(min(r(-0.0), r(0.0)), r(-0.0));
    assert_eq!(max(r(-0.0), r(0.0)), r(0.0));
    assert_eq!(max(r(0.0), r(-0.0)), r(-0.0));
    assert_eq!(max(r(f64::NAN), r(1.0)), r(1.0));
    assert!(max(r(1.0), r(f64::NAN)).is_nan());
    assert!(min(r(f64::NAN), r(1.0)).is_nan());
    assert_eq!(min(r(1.0), r(f64::NAN)), r(1.0));
    assert_eq!(clamp(r(5.0), r(0.0), r(3.0)), r(3.0));
    assert_eq!(clamp(r(-1.0), r(0.0), r(3.0)), r(0.0));
    assert_eq!(clamp(r(2.0), r(0.0), r(3.0)), r(2.0));
    assert_eq!(lerp(r(0.0), r(10.0), r(0.25)), r(2.5));
    assert_eq!(lerp(r(0.1), r(0.3), r(1.0)), r(0.1) + (r(0.3) - r(0.1)));
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

#[test]
fn gm_div_floors() {
    assert_eq!(gm_div(r(7.0), r(2.0)), Ok(r(3.0)));
    assert_eq!(gm_div(r(-7.0), r(2.0)), Ok(r(-4.0)));
    assert_eq!(gm_div(r(7.0), r(-2.0)), Ok(r(-4.0)));
    assert_eq!(gm_div(r(-33.0), r(32.0)), Ok(r(-2.0)));
    assert_eq!(gm_div(r(-7.0), r(-2.0)), Ok(r(3.0)));
    assert_eq!(gm_div(r(7.5), r(2.0)), Ok(r(3.0)));
    assert_eq!(gm_div(r(0.0), r(0.0)), Err(NumError::DivisionByZero));
}

// ---- Comparisons ----

#[test]
fn gm_comparisons_use_epsilon() {
    let sum = r(0.1) + r(0.2);
    assert!(gm_eq(sum, r(0.3)));
    assert!(!gm_ne(sum, r(0.3)));
    assert!(!gm_lt(r(0.3), sum));
    assert!(gm_le(r(0.3), sum));
    assert!(!gm_gt(sum, r(0.3)));
    assert!(gm_ge(r(0.3), sum));

    // The bound is exclusive: a difference of exactly EPSILON is not equal.
    let (zero, eps) = (r(0.0), r(EPSILON));
    assert!(!gm_eq(zero, eps) && gm_ne(zero, eps));
    assert!(gm_lt(zero, eps) && gm_le(zero, eps));
    assert!(!gm_gt(zero, eps) && !gm_ge(zero, eps));
    assert!(gm_gt(eps, zero) && gm_ge(eps, zero));
    assert!(!gm_lt(eps, zero) && !gm_le(eps, zero));
    // Just inside the bound is equal.
    let inside = r(0.99 * EPSILON);
    assert!(gm_eq(zero, inside) && !gm_ne(zero, inside));
    assert!(!gm_lt(zero, inside) && gm_le(zero, inside));
    assert!(!gm_gt(inside, zero) && gm_ge(inside, zero));

    // NaN: every comparison is false, != included.
    let nan = r(f64::NAN);
    assert!(!gm_eq(nan, nan));
    assert!(!gm_ne(nan, nan));
    assert!(!gm_ne(nan, r(0.0)));
    assert!(!gm_lt(nan, r(0.0)) && !gm_le(nan, r(0.0)));
    assert!(!gm_gt(nan, r(0.0)) && !gm_ge(nan, r(0.0)));
}

#[test]
fn gm_bool_threshold() {
    assert!(!gm_bool(r(0.0)));
    assert!(!gm_bool(r(-1.0)));
    assert!(!gm_bool(r(0.49999999999999994)));
    assert!(gm_bool(r(TRUTH_THRESHOLD)));
    assert!(gm_bool(r(0.5)));
    assert!(gm_bool(r(1.0)));
    assert!(!gm_bool(r(f64::NAN)));
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
/// in the OpenGMK traces, bit for bit.
#[test]
fn gravity_270_matches_trace_value() {
    let hspeed = r(0.4) * dcos(r(270.0));
    assert_eq!(hspeed, parse("-7.347880794884119e-17"));
    assert_eq!(lengthdir_x(r(0.4), r(270.0)), hspeed);
    assert_eq!(r(0.0) - r(0.4) * dsin(r(270.0)), r(0.4));
    assert_eq!(lengthdir_y(r(0.4), r(270.0)), r(0.4));
}

/// Regression guard: the table entries equal libm on the radian value, and dsin/dcos return
/// them. Not a fidelity check (libm against libm); `crt_golden_bits` and the trace test are.
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
/// `crt_golden_bits`.
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

/// Independent oracle for off-table angles: `sin` and `cos` of `degrees * DEG_TO_RAD` from the
/// Windows x64 UCRT, which the trace recorder (OpenGMK) calls. Measured 2026-10-24 on the
/// research machine with a black-boxed Rust std `f64::sin`/`f64::cos` (x86_64-pc-windows-msvc,
/// rustc 1.99.1), bit patterns below. UCRT may pick a different code path on another CPU.
///
/// Over every quarter degree in `[0, 360]`, UCRT and libm 0.2.16 differ by one ulp at 58 angles
/// (whole degrees: 34, 45, 53, 63, 113, 137, 207, 220, 224, 239, 284, 342, 351).
const CRT_AGREES_WITH_LIBM: [(f64, u64, u64); 7] = [
    (1.0, 0x3F91_DF0B_2B89_DD1E, 0x3FEF_FEC0_97F5_AF8A),
    (10.0, 0x3FC6_3A1A_7E0B_7389, 0x3FEF_838B_8C81_1C17),
    (30.0, 0x3FDF_FFFF_FFFF_FFFF, 0x3FEB_B67A_E858_4CAB),
    (60.0, 0x3FEB_B67A_E858_4CAA, 0x3FE0_0000_0000_0001),
    (135.0, 0x3FE6_A09E_667F_3BCD, 0xBFE6_A09E_667F_3BCC),
    (200.75, 0xBFD6_ACB4_515D_D44D, 0xBFED_ECA0_ACDE_1092),
    (1000.0, 0xBFEF_838B_8C81_1C16, 0x3FC6_3A1A_7E0B_73AA),
];

/// UCRT values (same source as [`CRT_AGREES_WITH_LIBM`]) where libm is one ulp off.
const CRT_DIFFERS_FROM_LIBM: [(f64, u64, u64); 4] = [
    (34.0, 0x3FE1_E4E8_8411_FD13, 0x3FEA_8779_CDA8_EEA5),
    (45.0, 0x3FE6_A09E_667F_3BCD, 0x3FE6_A09E_667F_3BCD),
    (53.0, 0x3FE9_8E6C_0EA2_7A14, 0x3FE3_4211_9455_BEB7),
    (63.0, 0x3FEC_8320_1D3D_2C6C, 0x3FDD_0E2E_2B44_DE01),
];

/// dsin/dcos against the UCRT oracle. Where they agree the bits must match. Where they differ
/// the gap is pinned at one ulp in one of sin or cos.
///
/// OPEN (docs/architecture.md, "Trig fidelity"): the sim uses libm here, so on these angles it
/// differs from the trace recorder by one ulp. Whether to follow UCRT, libm or x87 is for the
/// CLONE_SPEC and the Phase 2 cross-check; if the answer is UCRT, these angles move into the
/// table and this test moves them to the first list.
#[test]
fn crt_golden_bits() {
    for (degrees, sin_bits, cos_bits) in CRT_AGREES_WITH_LIBM {
        assert_eq!(dsin(r(degrees)), Real::from_bits(sin_bits), "dsin {degrees}");
        assert_eq!(dcos(r(degrees)), Real::from_bits(cos_bits), "dcos {degrees}");
    }
    let ulps = |a: Real, b: u64| a.to_bits().abs_diff(b);
    for (degrees, sin_bits, cos_bits) in CRT_DIFFERS_FROM_LIBM {
        let gaps = (ulps(dsin(r(degrees)), sin_bits), ulps(dcos(r(degrees)), cos_bits));
        assert!(
            gaps == (1, 0) || gaps == (0, 1),
            "{degrees} degrees: expected a one-ulp gap to UCRT, got {gaps:?}"
        );
    }
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
/// wherever gravity is 0.4 at direction 270 and hspeed is tiny but nonzero, it is our dcos
/// product.
#[test]
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
            let (hs, gr, gd) = (col("hspeed"), col("gravity"), col("gravity_direction"));
            for line in lines {
                let fields: Vec<&str> = line.split(',').collect();
                for &c in &real_cols {
                    let field = fields[c];
                    assert_eq!(parse(field).to_string(), field, "{}", path.display());
                }
                if fields[gr] == "0.4" && fields[gd] == "270.0" {
                    // A tiny nonzero hspeed is gravity's x part added to a zero hspeed.
                    let hspeed = parse(fields[hs]);
                    if hspeed != r(0.0) && hspeed.get().abs() < 1e-10 {
                        gravity_rows += 1;
                        assert_eq!(hspeed, r(0.4) * dcos(r(270.0)), "{}", path.display());
                    }
                }
            }
        }
    }
    assert!(files > 0, "no trace CSVs under {}", dir.display());
    assert!(gravity_rows > 0, "no gravity-270 rows found");
}
