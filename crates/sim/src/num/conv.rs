//! Rule-dependent conversions: alarms, save bytes, `string()` (docs/plans/sim-core.md 3.2, R3b,
//! N5, N6). WP0 declares the signatures; WP2 owns the file and the bodies.
#![allow(unused_variables, reason = "WP0 stubs: WP2 fills the bodies")]

use super::Real;
use crate::rules::NumRules;

/// The value an alarm stores when GML assigns `v` (R3b, `NumRules::alarm_from_real`).
pub fn alarm_from_real(v: Real, r: &NumRules) -> Real {
    unimplemented!("WP2: num::alarm_from_real")
}

/// The byte `file_bin_write_byte(v)` writes (N5, `NumRules::byte_from_real`).
pub fn byte_from_real(v: Real, r: &NumRules) -> u8 {
    unimplemented!("WP2: num::byte_from_real")
}

/// GML `string(v)` for a real: two decimals, trailing zeros dropped (N6,
/// `NumRules::string_decimals`).
pub fn gm_string_real(v: Real, r: &NumRules) -> String {
    unimplemented!("WP2: num::gm_string_real")
}
