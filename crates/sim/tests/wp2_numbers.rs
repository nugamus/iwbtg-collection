//! WP2 number sequences from the recorder traces that decide the NumRules defaults
//! (docs/plans/sim-core.md 3.2; CLONE_SPEC numbers, Confirmed (recorder)).

mod wp2 {
    use iwbtg_sim::num::{self, Real};
    use iwbtg_sim::rules::NumRules;
    use iwbtg_sim::rules::num::CompareMode;

    fn r(x: f64) -> Real {
        Real::new(x)
    }

    /// What go-the-jun-g-world's object1048 does (CLONE_SPEC numbers, "Equality after repeated
    /// adds"): Alarm 0 sets alpha to g and adds 0.02, re-arms while `g < 0.8`, and arms the
    /// fade (Alarm 1) when `g == 0.8`; Alarm 1 sets alpha to g, subtracts 0.02 and re-arms while
    /// `g > 0`. Returns (g at the top, whether the fade started, final g, final alpha).
    fn object1048(n: &NumRules) -> (Real, bool, Real, Real) {
        let step = r(0.02);
        let mut g = r(0.0);
        let mut alpha;
        loop {
            alpha = g;
            g += step;
            if !num::gm_lt(g, r(0.8), n) {
                break;
            }
        }
        let top = g;
        if !num::gm_eq(g, r(0.8), n) {
            return (top, false, g, alpha);
        }
        loop {
            alpha = g;
            g -= step;
            if !num::gm_gt(g, r(0.0), n) {
                break;
            }
        }
        (top, true, g, alpha)
    }

    /// `traces/go-the-jun-g-world/num-eps-alpha.csv`: g reaches 0.8000000000000004 on the 40th
    /// add, "g equals 0.8" passes (alarm 1 set on frame 45), and the fade stops at
    /// g = -6.245004513516506e-17 with alpha 0.019999999999999938 (rows 44-45, 247-249). The
    /// default compare reproduces it; an exact compare would never start the fade.
    #[test]
    fn eps_alpha_fade() {
        let (top, faded, g, alpha) = object1048(&NumRules::default());
        assert_eq!(top, r(0.8000000000000004));
        assert_eq!(top.to_bits(), 0x3FE9_9999_9999_999D);
        assert!(faded);
        assert_eq!(g, r(-6.245004513516506e-17));
        assert_eq!(alpha, r(0.019999999999999938));

        let exact = NumRules {
            compare: CompareMode::Exact,
            ..NumRules::default()
        };
        let (top, faded, _, alpha) = object1048(&exact);
        assert_eq!(top, r(0.8000000000000004));
        assert!(!faded);
        assert_eq!(alpha, r(0.7800000000000004));
    }

    /// The tolerance bounds the recorder shows: values from -1.4e-14 to -9e-14 compare equal to
    /// 0 and have sign 0, -1e-13 does not (num-eps-slip-b.csv rows 50-51, -c.csv rows 50-100).
    #[test]
    fn eps_slip_tolerance_bounds() {
        let n = NumRules::default();
        for v in [-1.4e-14, -5e-14, -9e-14] {
            assert!(num::gm_eq(r(v), r(0.0), &n), "{v} == 0");
            assert!(!num::gm_ne(r(v), r(0.0), &n), "{v} != 0");
            assert_eq!(num::sign(r(v), &n), r(0.0), "sign({v})");
        }
        assert!(num::gm_ne(r(-1e-13), r(0.0), &n));
        assert!(num::gm_lt(r(-1e-13), r(0.0), &n));
        assert_eq!(num::sign(r(-1e-13), &n), r(-1.0));
    }

    /// machinary t-mot-02-a.csv row 84: the Step test "vspeed above 9" did not fire for
    /// 9.000000000000005.
    #[test]
    fn fall_cap_tolerance() {
        let n = NumRules::default();
        assert!(!num::gm_gt(r(9.000000000000005), r(9.0), &n));
        let exact = NumRules {
            compare: CompareMode::Exact,
            ..NumRules::default()
        };
        assert!(num::gm_gt(r(9.000000000000005), r(9.0), &exact));
    }

    /// The alarm and byte conversions the recorder showed (num-alarm-half-b, num-byte-save).
    #[test]
    fn integer_conversions_round_half_even() {
        let n = NumRules::default();
        assert_eq!(num::alarm_from_real(r(1.3), &n), r(1.0));
        assert_eq!(num::alarm_from_real(r(1.5), &n), r(2.0));
        assert_eq!(num::alarm_from_real(r(2.0) - r(0.5), &n), r(2.0));
        assert_eq!(num::byte_from_real(r(67.4), &n), 67);
        assert_eq!(num::byte_from_real(r(48.5), &n), 48);
        // rely's three-byte y save of 567.4: floor(y / 10000), floor(rest / 100), rest.
        let y = r(567.4);
        let rest = y - num::floor(y / r(10000.0), &n) * r(10000.0);
        let rest = rest - num::floor(rest / r(100.0), &n) * r(100.0);
        assert_eq!(num::byte_from_real(rest, &n), 67);
    }
}
