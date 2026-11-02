//! WP2 RNG tests (docs/plans/sim-core.md 3.3, 12; N9). Values from
//! `traces/rely-the-needle-4-2/num-rng.md` (Confirmed (recorder), CLONE_SPEC numbers "RNG").

mod wp2 {
    use iwbtg_sim::SimError;
    use iwbtg_sim::num::Real;
    use iwbtg_sim::rng::Rng;

    fn r(x: f64) -> Real {
        Real::new(x)
    }

    /// A seed as `random_get_seed()` prints it (signed) to our unsigned state.
    fn seed(signed: i64) -> u32 {
        u32::try_from(signed.rem_euclid(1 << 32)).expect("fits")
    }

    /// LCG steps from `from` to `to`, searching up to `limit`.
    fn steps_between(from: u32, to: u32, limit: u32) -> Option<u32> {
        let mut rng = Rng::new(from);
        (1..=limit).find(|_| {
            rng.next_unit();
            rng.seed == to
        })
    }

    #[test]
    fn lcg_sequence_from_seed_0() {
        let mut rng = Rng::new(0);
        let u = rng.next_unit();
        assert_eq!(rng.seed, 1);
        assert_eq!(u, r(1.0 / 4_294_967_296.0));
        rng.next_unit();
        assert_eq!(rng.seed, 134_775_814);
        rng.next_unit();
        // 134775814 * 134775813 + 1 mod 2^32, with wrapping.
        let expected = 134_775_814u64 * 134_775_813 + 1;
        assert_eq!(u64::from(rng.seed), expected % (1 << 32));
    }

    #[test]
    fn lcg_wraps_at_2_pow_32() {
        let mut rng = Rng::new(u32::MAX);
        rng.next_unit();
        let expected = (u64::from(u32::MAX) * 134_775_813 + 1) % (1 << 32);
        assert_eq!(u64::from(rng.seed), expected);
        // u stays in [0, 1).
        let mut rng = Rng::new(u32::MAX);
        for _ in 0..1000 {
            let u = rng.next_unit().get();
            assert!((0.0..1.0).contains(&u));
        }
    }

    /// The recorded seeds (num-rng-a.csv, random_get_seed()) are whole LCG steps apart: 9 draws
    /// on frame 1, 13 on frame 2, 173 on frame 15 (40 blood x 4 + the runner's particle draws).
    #[test]
    fn recorded_seeds_are_lcg_steps_apart() {
        assert_eq!(steps_between(0, seed(1_598_751_577), 1000), Some(9));
        assert_eq!(
            steps_between(seed(1_598_751_577), seed(-669_280_590), 1000),
            Some(13)
        );
        assert_eq!(
            steps_between(seed(-1_390_856_633), seed(1_088_289_944), 1000),
            Some(173)
        );
    }

    /// num-rng-b.csv: the first blood of frame 15 draws, in order, choose(0,1,2), gravity
    /// 0.1 + random(0.2), direction irandom(35) * 10 and speed random(6), starting from the
    /// frame-14 seed.
    #[test]
    fn blood_draws_match_the_trace() {
        let mut rng = Rng::new(seed(-1_390_856_633));
        let blood = |rng: &mut Rng| {
            let image = rng.choose(3).expect("3 arguments");
            let gravity = r(0.1) + rng.random(r(0.2));
            let direction = rng.irandom(r(35.0)).expect("finite") * r(10.0);
            let speed = rng.random(r(6.0));
            (image, gravity, direction, speed)
        };
        assert_eq!(
            blood(&mut rng),
            (0, r(0.11717807003296912), r(270.0), r(2.971668702084571))
        );
        assert_eq!(
            blood(&mut rng),
            (2, r(0.20199325983412564), r(200.0), r(5.719158007297665))
        );
    }

    #[test]
    fn one_draw_per_call() {
        let calls: [fn(&mut Rng); 6] = [
            |g| {
                g.random(r(5.0));
            },
            |g| {
                g.random_range(r(-3.0), r(7.5));
            },
            |g| {
                g.irandom(r(35.0)).expect("finite");
            },
            |g| {
                g.irandom_range(r(2.0), r(9.0)).expect("finite");
            },
            |g| {
                g.choose(16).expect("16 arguments");
            },
            |g| {
                g.next_unit();
            },
        ];
        for call in calls {
            let mut a = Rng::new(12345);
            let mut b = Rng::new(12345);
            call(&mut a);
            b.next_unit();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn scaling_and_bounds() {
        let mut rng = Rng::new(777);
        for _ in 0..10_000 {
            let mut probe = rng.clone();
            let u = probe.next_unit();
            let x = rng.random(r(10.0));
            assert_eq!(x, r(10.0) * u);
            assert!((0.0..10.0).contains(&x.get()));
        }
        for n in 1..=16 {
            for _ in 0..500 {
                assert!(rng.choose(n).expect("arguments") < n);
            }
        }
        for _ in 0..2000 {
            let i = rng.irandom(r(3.0)).expect("finite").get();
            assert!([0.0f64, 1.0, 2.0, 3.0].iter().any(|v| v.to_bits() == i.to_bits()));
            let j = rng.irandom_range(r(-2.0), r(2.0)).expect("finite").get();
            assert!((-2.0..=2.0).contains(&j) && j.fract().to_bits() == 0.0f64.to_bits());
            let k = rng.random_range(r(5.0), r(6.0)).get();
            assert!((5.0..6.0).contains(&k));
        }
        // irandom floors a fractional bound (OPEN): 2.9 behaves as 2.
        for _ in 0..500 {
            assert!(rng.irandom(r(2.9)).expect("finite").get() <= 2.0);
        }
    }

    #[test]
    fn errors_draw_nothing() {
        let mut rng = Rng::new(5);
        assert!(matches!(rng.choose(0), Err(SimError::Invariant { .. })));
        assert!(matches!(
            rng.irandom(r(f64::NAN)),
            Err(SimError::NonFinite { .. })
        ));
        assert!(matches!(
            rng.irandom_range(r(0.0), r(f64::INFINITY)),
            Err(SimError::NonFinite { .. })
        ));
        assert_eq!(rng.seed, 5);
    }
}
