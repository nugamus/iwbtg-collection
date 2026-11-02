//! WP2 slip-block sequence (docs/plans/sim-core.md 3.2, 12): why N11 (`sign`) must follow N1
//! (compare). Slip blocks (CLONE_SPEC numbers, "Slip blocks", little-trap and rely player Step)
//! add or subtract `slip = 0.2` while a direction is held and `hspeed` is inside `maxSpeed = 3`,
//! and with no direction held subtract `slip * sign(hspeed)` while `hspeed != 0`.

mod wp2 {
    use iwbtg_sim::def::RoomDef;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::motion::{Motion, apply_auto_motion};
    use iwbtg_sim::num::{self, Real};
    use iwbtg_sim::rules::num::{CompareMode, SignMode};
    use iwbtg_sim::rules::{MotionRules, NumRules};

    fn r(x: f64) -> Real {
        Real::new(x)
    }

    const SLIP: Real = Real::new(0.2);
    const MAX_SPEED: Real = Real::new(3.0);

    /// The kid's slip-block Step for held direction `d` (+1, -1 or 0).
    fn slip_step(m: &mut Motion, d: i8, n: &NumRules) {
        let h = m.hspeed();
        let next = match d {
            1 if num::gm_lt(h, MAX_SPEED, n) => h + SLIP,
            -1 if num::gm_gt(h, -MAX_SPEED, n) => h - SLIP,
            0 if num::gm_ne(h, r(0.0), n) => h - SLIP * num::sign(h, n),
            _ => h,
        };
        m.set_hspeed(next).expect("finite");
    }

    /// Exact compare and exact sign, the rules the spec's earlier prediction assumed.
    fn exact() -> NumRules {
        NumRules {
            compare: CompareMode::Exact,
            sign: SignMode::Exact,
            ..NumRules::default()
        }
    }

    /// hspeed after each tick of `ticks` ticks of the bare number sequence (no gravity): Right
    /// for the first 15, then nothing.
    fn bare_sequence(n: &NumRules, ticks: usize) -> Vec<Real> {
        let mut m = Motion::new_at(r(0.0), r(0.0));
        (0..ticks)
            .map(|t| {
                slip_step(&mut m, if t < 15 { 1 } else { 0 }, n);
                m.hspeed()
            })
            .collect()
    }

    #[test]
    fn slip_peak_is_3_0000000000000004() {
        for n in [NumRules::default(), exact()] {
            let hs = bare_sequence(&n, 16);
            assert_eq!(hs[13], r(2.8000000000000003));
            assert_eq!(hs[14], r(3.0000000000000004));
            // Release: the first slowdown.
            assert_eq!(hs[15], r(2.8000000000000003));
        }
    }

    /// Under exact compare and sign the slowdown never stops: hspeed ends at a value about
    /// 5.55e-17 from 0 and from then on flips between it and a value near -0.2, so x would creep
    /// by about 0.1 px per tick. Under the default epsilon rules, 5.55e-17 counts as 0 and the
    /// slowdown stops there.
    #[test]
    fn slip_creep_under_exact_stop_under_epsilon() {
        let tiny = r(5.551115123125783e-17);
        let exact_hs = bare_sequence(&exact(), 80);
        let eps_hs = bare_sequence(&NumRules::default(), 80);
        // Both reach the tiny remainder on the same tick (15 adds, 15 subtractions).
        let end = exact_hs
            .iter()
            .position(|h| num::abs(*h) == tiny)
            .expect("reaches the 5.55e-17 remainder");
        assert_eq!(eps_hs[end], exact_hs[end]);
        // Exact: it keeps alternating.
        let tail = &exact_hs[end..];
        assert!(tail.len() > 20);
        for pair in tail.windows(2) {
            assert!(num::abs(pair[0]) == tiny || num::abs(pair[1]) == tiny);
            assert_ne!(pair[0], pair[1]);
        }
        assert!(tail.iter().any(|h| h.get() < -0.19));
        // Epsilon: it stops at the remainder for good.
        assert!(eps_hs[end..].iter().all(|h| *h == eps_hs[end]));
    }

    /// The recorded run, `traces/little-trap/num-eps-slip.csv` (Confirmed (recorder), CLONE_SPEC
    /// numbers): Left held on frames 12-26 from x 145. Gravity leaves its 270-degree residue in
    /// hspeed every tick, the block event zeroes vspeed. With the default rules: hspeed ends at
    /// -3.0000000000000018 on row 26, the slowdown stops at -1.4890131643459157e-15 on row 41,
    /// and x drifts to 99.99999999999882 on row 201.
    fn recorded_run(n: &NumRules) -> Vec<(Real, Real)> {
        let mut b = GameDefBuilder::new("wp2");
        b.room(RoomDef::new("rStage02", 800, 608));
        let def = b.build().expect("one-room game builds");
        let rules = MotionRules::default();
        let mut m = Motion::new_at(r(145.0), r(567.4));
        let mut rows = vec![(r(0.0), r(0.0)); 2];
        for frame in 2..=201 {
            let d = if (12..=26).contains(&frame) { -1 } else { 0 };
            if frame > 2 {
                slip_step(&mut m, d, n);
            }
            m.set_gravity(r(0.4)).expect("finite");
            apply_auto_motion(&mut m, None, &def, &rules).expect("motion");
            // Collision with block: back onto the floor, vspeed 0.
            m.set_y(r(567.4)).expect("finite");
            m.set_vspeed(r(0.0)).expect("finite");
            rows.push((m.x(), m.hspeed()));
        }
        rows
    }

    #[test]
    fn slip_recorded_run_matches_trace_rows() {
        let rows = recorded_run(&NumRules::default());
        let residue = Real::from_bits(0xBC95_2DC7_707E_D4D5);
        assert_eq!(rows[2], (r(145.0), residue));
        assert_eq!(rows[11].1, r(-7.34788079488412e-16));
        assert_eq!(rows[12], (r(144.8), r(-0.20000000000000082)));
        assert_eq!(rows[26], (r(121.00000000000001), r(-3.0000000000000018)));
        assert_eq!(rows[27], (r(118.20000000000002), r(-2.8000000000000016)));
        assert_eq!(rows[40], (r(100.00000000000001), r(-0.20000000000000143)));
        assert_eq!(rows[41], (r(100.00000000000001), r(-1.4890131643459157e-15)));
        assert_eq!(rows[42].1, r(-1.5624919722947568e-15));
        assert_eq!(rows[199].0, r(99.99999999999885));
        assert_eq!(rows[201], (r(99.99999999999882), r(-1.3245622436160541e-14)));
    }

    /// The same run under exact compare and sign never settles: the residue alone already
    /// triggers the slowdown while the kid stands (rows 3-11), and after the release hspeed swings
    /// by 0.2 every tick, which the recorder never shows.
    #[test]
    fn slip_recorded_run_under_exact_rules_keeps_firing() {
        let rows = recorded_run(&exact());
        assert!(rows[3..12].iter().any(|(_, h)| num::abs(*h).get() > 0.1));
        let swings = rows[42..]
            .iter()
            .filter(|(_, h)| num::abs(*h).get() > 0.1)
            .count();
        assert!(swings > 50, "only {swings} swings");
    }
}
