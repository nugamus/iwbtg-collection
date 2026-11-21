//! Number rule switches (docs/architecture/sim/numbers.md, rng.md, register N1-N6, N10-N13, R3b).
//!
//! Every rule with OpenGMK provenance or an open spec status is a field here, never a `const`,
//! so a rule settled later becomes a new `engine_rev`. [`NumRules`] is a **ruleset value**: the
//! two values are [`NumRules::gm8`] (the recorder's rules, the engine_rev 0 defaults) and
//! [`NumRules::standard`] (exact IEEE comparisons, the modern default). A package selects the
//! ruleset; it cannot supply a tolerance or any other field (decision 0005). The engine_rev 0
//! defaults keep the earlier `num` behavior where the spec has no answer yet, and follow the
//! recorder where the CLONE_SPEC numbers section records a trace result (Confirmed (recorder)):
//! compare and sign (num-eps-slip, num-eps-alpha), alarms (num-alarm-half) and save bytes
//! (num-byte-save, which moved N5 from truncation to round-half-even). The real GM8 runner is
//! unchecked for all of them. WP2 owns this file.

use crate::num::Real;

/// Rules for number handling.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NumRules {
    /// N1 (the real runner uses a tolerance too, its exact `t` is OPEN; code value from OpenGMK): `==`, `<` exact or with an
    /// epsilon. The recorder uses a tolerance `9e-14 < t <= 1e-13` (CLONE_SPEC numbers,
    /// `traces/little-trap/num-eps-slip-b.csv`, `-c.csv`; `traces/go-the-jun-g-world/num-eps-alpha.csv`),
    /// which the default `Epsilon(1e-13)` fits.
    pub compare: CompareMode,
    /// N2 (spec infers `> 0.5`; code value from OpenGMK): truthiness.
    pub truth: Truth,
    /// N11 (OpenGMK provenance): `sign` exact or with the compare epsilon. Follows N1. The
    /// recorder gives sign 0 for `-9e-14` and -1 for `-1e-13` (num-eps-slip-b, -c), which the
    /// default `Epsilon` fits.
    pub sign: SignMode,
    /// N12 (OpenGMK provenance): `min`/`max` ties and signed zeros.
    pub min_max_ties: MinMaxTies,
    /// N4 (−0 rule from OpenGMK): `round(-0.4)` gives `+0` or `-0`.
    pub round_neg_zero: NegZero,
    /// N13 (OpenGMK provenance): `floor`/`ceil` keep a signed zero or give `+0`.
    pub floor_ceil_zero: NegZero,
    /// R3b: fractional alarm values. Recorder: round half to even
    /// (`traces/kaenbyou/num-alarm-half-b.csv`); the real runner is unchecked.
    pub alarm_from_real: AlarmConv,
    /// N5: `file_bin_write_byte` of a fraction or an out-of-range value. Recorder: fractions
    /// round half to even (`traces/rely-the-needle-4-2/num-byte-save-a..d.csv`, `tr-room-05-e.csv`
    /// row 63: 48.5 gives 48). Out-of-range values and the real runner are UNKNOWN.
    pub byte_from_real: ByteConv,
    /// N6 (UNKNOWN): `string()` two-decimal rounding. Recorder: exact binary halves go to even,
    /// other values fit no simple rule (`traces/eat-the-new-year/num-string-save-b.csv`,
    /// `traces/machinary/tr-room-07-b.csv`, `-c.csv`). The default stays until research#100
    /// finds the rule.
    pub string_decimals: StrRound,
    /// N3 (Confirmed, real runner: x87 at 53-bit precision, so binary64; no fallback needed):
    /// FPU precision; per game from `game.toml [engine]`.
    pub float_model: FloatModel,
    /// N10: particle systems draw from the gameplay RNG. Confirmed (recorder): CLONE_SPEC numbers
    /// "Particle draw rules" (research#158), modelled in [`crate::particles`]; the real runner is
    /// unchecked (research#164). engine_rev 0: true (#766). No package is verified yet, so the
    /// default changed in place; it only matters to packages with `particle_system` instances or
    /// a `blood_emitter` in `particles` mode.
    pub particles_draw_rng: bool,
}

impl NumRules {
    /// The `gm8` ruleset: the recorder's rules and the engine_rev 0 defaults.
    ///
    /// This is also [`Default`], so a package that selects no ruleset keeps every existing
    /// outcome and hash. A package selects a ruleset; it cannot supply a tolerance or any other
    /// value here (decision 0005).
    pub const fn gm8() -> NumRules {
        NumRules {
            compare: CompareMode::Epsilon(Real::new(1e-13)),
            truth: Truth::GeHalf,
            sign: SignMode::Epsilon,
            min_max_ties: MinMaxTies::StrictGreater,
            round_neg_zero: NegZero::Positive,
            floor_ceil_zero: NegZero::Keep,
            alarm_from_real: AlarmConv::RoundHalfEven,
            byte_from_real: ByteConv::RoundWrap,
            string_decimals: StrRound::HalfEven,
            float_model: FloatModel::F64,
            particles_draw_rng: true,
        }
    }

    /// The `standard` ruleset: exact IEEE comparisons (N1) and exact `sign` (N11).
    ///
    /// The standard ruleset has no comparison epsilon and no comparison mode. The values that
    /// decision 0005 does not settle stay the `gm8` ones: truthiness keeps `>= 0.5` until a trace
    /// settles the boundary, and the conversions, the `min`/`max` tie rule, the signed-zero rules
    /// and the float model have no standard contract yet. Where a feel contract needs a forgiving
    /// boundary, the call site declares a [`crate::num::Tolerance`]; it is not a field here and a
    /// package cannot set it.
    pub const fn standard() -> NumRules {
        NumRules {
            compare: CompareMode::Exact,
            truth: Truth::GeHalf,
            sign: SignMode::Exact,
            min_max_ties: MinMaxTies::StrictGreater,
            round_neg_zero: NegZero::Positive,
            floor_ceil_zero: NegZero::Keep,
            alarm_from_real: AlarmConv::RoundHalfEven,
            byte_from_real: ByteConv::RoundWrap,
            string_decimals: StrRound::HalfEven,
            float_model: FloatModel::F64,
            particles_draw_rng: true,
        }
    }
}

impl Default for NumRules {
    /// The absent ruleset is `gm8` (decision 0005, migration).
    fn default() -> NumRules {
        NumRules::gm8()
    }
}

/// N1: how reals compare.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompareMode {
    /// Exact IEEE comparison (what the spec infers).
    Exact,
    /// Equal when `|a - b| < epsilon` (engine_rev 0 default, `1e-13`; fits the recorder).
    Epsilon(Real),
}

/// N2: truthiness of a real.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Truth {
    /// True when `x > 0.5` (what the spec infers).
    GtHalf,
    /// True when `x >= 0.5` (engine_rev 0 default).
    GeHalf,
}

/// N11: `sign`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SignMode {
    /// Exact sign: any nonzero value gives ±1.
    Exact,
    /// Values inside the compare epsilon give 0 (engine_rev 0 default).
    Epsilon,
}

/// N12: `min`/`max` on ties and signed zeros.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MinMaxTies {
    /// Both compare with a strict `a > b`: `min` keeps `a` and `max` takes `b` on ties
    /// (engine_rev 0 default, the current `num` code).
    StrictGreater,
    /// Both compare with `a >= b`: `min` takes `b` and `max` keeps `a` on ties.
    GreaterOrEqual,
}

/// N4, N13: what happens to a zero result's sign.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NegZero {
    /// A zero result is `+0`.
    Positive,
    /// IEEE: the sign is kept.
    Keep,
}

/// R3b: fractional alarm values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlarmConv {
    /// Round half to even (engine_rev 0 default; Confirmed (recorder)).
    RoundHalfEven,
    /// Truncate toward zero.
    Truncate,
    /// Keep the fraction.
    Keep,
}

/// N5: `file_bin_write_byte` of a fraction or an out-of-range value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ByteConv {
    /// Truncate toward 0, then wrap mod 256.
    TruncWrap,
    /// Round half to even, then wrap mod 256 (engine_rev 0 default; the rounding is Confirmed
    /// (recorder), the wrap is UNKNOWN).
    RoundWrap,
}

/// N6: `string()` two-decimal rounding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StrRound {
    /// Round the exact decimal value, ties to even (engine_rev 0 default; matches the recorder's
    /// exact binary halves, not all of its other values).
    HalfEven,
    /// Round half away from zero.
    HalfAway,
}

/// N3: the runner's float model.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FloatModel {
    /// IEEE binary64, the only model for now.
    #[default]
    F64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A package with no ruleset gets exactly the pre-ruleset rules, so every existing outcome
    /// and hash is unchanged (decision 0005, migration).
    #[test]
    fn gm8_is_the_default_ruleset() {
        assert_eq!(NumRules::default(), NumRules::gm8());
        let gm8 = NumRules::gm8();
        assert_eq!(gm8.compare, CompareMode::Epsilon(Real::new(1e-13)));
        assert_eq!(gm8.truth, Truth::GeHalf);
        assert_eq!(gm8.sign, SignMode::Epsilon);
        assert_eq!(gm8.min_max_ties, MinMaxTies::StrictGreater);
        assert_eq!(gm8.round_neg_zero, NegZero::Positive);
        assert_eq!(gm8.floor_ceil_zero, NegZero::Keep);
        assert_eq!(gm8.alarm_from_real, AlarmConv::RoundHalfEven);
        assert_eq!(gm8.byte_from_real, ByteConv::RoundWrap);
        assert_eq!(gm8.string_decimals, StrRound::HalfEven);
        assert_eq!(gm8.float_model, FloatModel::F64);
        assert!(gm8.particles_draw_rng);
    }

    /// The standard ruleset changes the comparison policy only. Every other rule has no standard
    /// contract yet, so it stays the gm8 value (decision 0005; the user kept `>= 0.5`).
    #[test]
    fn standard_is_exact_and_keeps_the_unsettled_rules() {
        let standard = NumRules::standard();
        let gm8 = NumRules::gm8();
        assert_eq!(standard.compare, CompareMode::Exact);
        assert_eq!(standard.sign, SignMode::Exact);
        assert_eq!(standard.truth, gm8.truth);
        assert_eq!(standard.min_max_ties, gm8.min_max_ties);
        assert_eq!(standard.round_neg_zero, gm8.round_neg_zero);
        assert_eq!(standard.floor_ceil_zero, gm8.floor_ceil_zero);
        assert_eq!(standard.alarm_from_real, gm8.alarm_from_real);
        assert_eq!(standard.byte_from_real, gm8.byte_from_real);
        assert_eq!(standard.string_decimals, gm8.string_decimals);
        assert_eq!(standard.float_model, gm8.float_model);
        assert_eq!(standard.particles_draw_rng, gm8.particles_draw_rng);
        assert_ne!(standard, gm8);
    }
}
