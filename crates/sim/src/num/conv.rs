//! Rule-dependent conversions: alarms, save bytes, `string()` (docs/plans/sim-core.md 3.2, R3b,
//! N5, N6).
//!
//! Each conversion reads its rule from [`NumRules`]. The engine_rev 0 defaults follow the
//! recorder's traces where the CLONE_SPEC numbers section records one (Confirmed (recorder)) and
//! name what is still OPEN.

use super::Real;
use crate::rules::NumRules;
use crate::rules::num::{AlarmConv, ByteConv, StrRound};

/// The value an alarm stores when GML assigns `v` (R3b, `NumRules::alarm_from_real`).
///
/// `AlarmConv::RoundHalfEven` (engine_rev 0 default): round to nearest, ties to even, so 1.5 is
/// 2, 2.5 is 2, 0.5 is 0 and 3.5 is 4 (Confirmed (recorder):
/// `traces/kaenbyou/num-alarm-half-b.csv` rows 10-31; 1.3 fires every tick in
/// `traces/go-the-jun-g-world/num-eps-alpha.csv`). A zero result is `+0.0`. `Truncate` rounds
/// toward zero; `Keep` stores `v` as it is.
///
/// A non-finite `v` comes back unchanged; the alarm setter rejects it (N15).
pub fn alarm_from_real(v: Real, r: &NumRules) -> Real {
    if !v.is_finite() {
        return v;
    }
    let x = v.get();
    match r.alarm_from_real {
        AlarmConv::RoundHalfEven => positive_zero(x.round_ties_even()),
        AlarmConv::Truncate => positive_zero(x.trunc()),
        AlarmConv::Keep => v,
    }
}

/// `-0.0` becomes `+0.0`; every other value is unchanged.
fn positive_zero(v: f64) -> Real {
    // Adding +0.0 turns -0.0 into +0.0 and leaves every other value (and NaN) as it is.
    Real::new(v + 0.0)
}

/// The byte `file_bin_write_byte(v)` writes (N5, `NumRules::byte_from_real`).
///
/// `ByteConv::RoundWrap` (engine_rev 0 default): round to nearest, ties to even, then wrap mod
/// 256. The rounding is Confirmed (recorder): 51.59999999999991 is written as 52, 67.4 as 67, and
/// the tie 48.5 as 48 (`traces/rely-the-needle-4-2/num-byte-save-a..d.csv`, `tr-room-05-e.csv`
/// row 63; `traces/go-the-spook-house/sh-save-restart-a.csv` row 190: 455.1 as 55). The wrap for
/// values outside 0..255 is OPEN (not traced). `ByteConv::TruncWrap` truncates toward zero
/// instead.
///
/// A non-finite `v` gives 0 (OPEN; built-ins reject non-finite values before they get here).
pub fn byte_from_real(v: Real, r: &NumRules) -> u8 {
    if !v.is_finite() {
        return 0;
    }
    let integral = match r.byte_from_real {
        ByteConv::RoundWrap => v.get().round_ties_even(),
        ByteConv::TruncWrap => v.get().trunc(),
    };
    // Exact for every finite integral value: the remainder is an integer in [0, 256).
    let wrapped = integral.rem_euclid(256.0);
    super::to_int_trunc(Real::new(wrapped))
        .ok()
        .and_then(|b| u8::try_from(b).ok())
        .unwrap_or(0)
}

/// GML `string(v)` for a real: no decimals for an integer, else exactly two (Confirmed, manual;
/// Confirmed (recorder): `551.70`, `traces/eat-the-new-year/num-string-save-a.csv` row 40). The
/// rounding of the second decimal is N6, `NumRules::string_decimals`:
///
/// - `StrRound::HalfEven` (engine_rev 0 default): the exact decimal value of the double, rounded
///   to two decimals, ties to even. Matches the recorder for exact binary halves (0.125 gives
///   0.12, 0.375 gives 0.38, 0.625 gives 0.62, 183.125 gives 183.12) and for 183.005 (183.00) and
///   0.015 (0.01) (`num-string-save-b.csv` row 2, `traces/machinary/tr-room-07-b.csv` and `-c.csv`
///   row 55). It does not match 2.675 (recorder 2.68) or 1.005 (recorder 1.01); the spec says no
///   simple rule fits all of them, so N6 stays OPEN.
/// - `StrRound::HalfAway`: the same, with ties away from zero.
///
/// Only `x.125`, `x.375`, `x.625` and `x.875` are exact ties, because a two-decimal half must be
/// a multiple of 1/8 to be a double. OPEN as well: negative zero (written `0` here), values that
/// round to a negative zero (`-0.001` gives `-0.00`), very large integers (written in full) and
/// non-finite values (Rust's `NaN`, `inf`, `-inf`; setters reject them, N15).
pub fn gm_string_real(v: Real, r: &NumRules) -> String {
    let x = v.get();
    if !x.is_finite() {
        return format!("{x}");
    }
    if x.trunc().to_bits() == x.to_bits() {
        // Integral (signed zero included): no decimals. `+ 0.0` turns -0.0 into 0.
        return format!("{:.0}", x + 0.0);
    }
    let eighths = x * 8.0;
    let is_tie = eighths.trunc().to_bits() == eighths.to_bits() && (x * 4.0).fract() != 0.0;
    if !is_tie {
        // Not a tie: the correctly rounded two-decimal text of the exact value.
        return format!("{x:.2}");
    }
    // A tie: three exact decimals ending in 5. Drop the 5 and round the digit string.
    let exact = format!("{x:.3}");
    let kept = &exact[..exact.len() - 1];
    let last_odd = kept
        .bytes()
        .last()
        .is_some_and(|d| d.is_ascii_digit() && (d - b'0') % 2 == 1);
    let round_up = match r.string_decimals {
        StrRound::HalfEven => last_odd,
        StrRound::HalfAway => true,
    };
    if round_up {
        increment_decimal(kept)
    } else {
        kept.to_string()
    }
}

/// Adds one unit in the last place of a decimal string such as `-12.34` (giving `-12.35`), with
/// carries (`9.99` gives `10.00`). The sign is kept, so this rounds away from zero.
fn increment_decimal(text: &str) -> String {
    let mut digits: Vec<u8> = text.bytes().collect();
    let mut carry = true;
    for d in digits.iter_mut().rev() {
        if !carry {
            break;
        }
        match *d {
            b'9' => *d = b'0',
            b'0'..=b'8' => {
                *d += 1;
                carry = false;
            }
            _ => {}
        }
    }
    let mut out = String::with_capacity(digits.len() + 1);
    let body = String::from_utf8(digits).unwrap_or_default();
    if carry {
        // All digits were 9: insert a 1 after the sign.
        match body.strip_prefix('-') {
            Some(rest) => {
                out.push_str("-1");
                out.push_str(rest);
            }
            None => {
                out.push('1');
                out.push_str(&body);
            }
        }
    } else {
        out.push_str(&body);
    }
    out
}

/// The double the recorder reads for the GML decimal literal `text` (N14, research#104): all its
/// digits as one integer, times the double nearest 10^-k, where k is the number of digits after
/// the dot. `"0.95"` is 0.9500000000000001 and `"0.3"` is 0.30000000000000004 (one ulp above the
/// nearest double); `"0.4"`, `"0.45"` and `"0.017"` are the nearest double. `None` unless `text`
/// is digits with at most one dot (GM8 has no exponent syntax; a sign is an operator).
///
/// Packages store the result (the converter writes it in shortest round-trip form), so the sim
/// never parses GML at run time. This function documents the rule, pins it in tests and gives
/// built-in Rust constants that port a GML literal their value. The real runner's parser is
/// OPEN (research#8).
pub fn gml_literal(text: &str) -> Option<Real> {
    let (int, frac) = text.split_once('.').unwrap_or((text, ""));
    let digits = format!("{int}{frac}");
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let m = digits.parse::<f64>().ok()?;
    if frac.is_empty() {
        return Some(Real::new(m));
    }
    let scale = format!("1e-{}", frac.len()).parse::<f64>().ok()?;
    Some(Real::new(m * scale))
}
