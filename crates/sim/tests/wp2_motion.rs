//! WP2 motion tests (docs/plans/sim-core.md 3.1, 12): coupling bits, friction, gravity vector,
//! the fall table, non-finite setters. Values come from the CLONE_SPEC motion section and the
//! recorder traces it cites; trace-reading checks are in `wp2_traces.rs`.

mod wp2 {
    use iwbtg_sim::SimError;
    use iwbtg_sim::def::RoomDef;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::motion::{Motion, apply_auto_motion};
    use iwbtg_sim::num::{self, Real};
    use iwbtg_sim::rules::{MotionRules, NumRules};

    fn r(x: f64) -> Real {
        Real::new(x)
    }

    fn def() -> iwbtg_sim::GameDef {
        let mut b = GameDefBuilder::new("wp2");
        b.room(RoomDef::new("room", 800, 608));
        b.build().expect("one-room game builds")
    }

    /// One tick of automatic motion with the engine_rev 0 rules and no path.
    fn tick(m: &mut Motion) {
        let events = apply_auto_motion(m, None, &def(), &MotionRules::default()).expect("motion");
        assert!(!events.end_of_path);
    }

    /// The residue gravity 0.4 at direction 270 leaves in hspeed.
    const RESIDUE: Real = Real::from_bits(0xBC95_2DC7_707E_D4D5);

    #[test]
    fn new_instance_defaults() {
        let m = Motion::new_at(r(32.0), r(544.0));
        assert_eq!((m.x(), m.y()), (r(32.0), r(544.0)));
        assert_eq!((m.xstart(), m.ystart()), (r(32.0), r(544.0)));
        assert_eq!((m.xprevious(), m.yprevious()), (r(32.0), r(544.0)));
        assert_eq!((m.hspeed(), m.vspeed(), m.speed()), (r(0.0), r(0.0), r(0.0)));
        assert_eq!(m.direction(), r(0.0));
        assert_eq!(
            (m.gravity(), m.gravity_direction(), m.friction()),
            (r(0.0), r(270.0), r(0.0))
        );
    }

    /// M5: kaenbyou/stand.csv rows 3-9 and t-alarm-1.csv.
    #[test]
    fn coupling_polar_from_vector() {
        let mut m = Motion::new_at(r(0.0), r(0.0));
        m.set_hspeed(RESIDUE).unwrap();
        assert_eq!(m.direction(), r(180.0));
        assert_eq!(m.speed(), r(7.347880794884119e-17));
        m.set_hspeed(r(16.0)).unwrap();
        // atan2(-0.0, 16) is -0.0, which is not below 0 and so does not wrap.
        assert_eq!(m.direction(), r(-0.0));
        assert_eq!(m.speed(), r(16.0));
        // t-mot-06-b.csv rows 20-31: hspeed -3, then 0, gives direction -0.0; speed 2 then moves
        // right.
        m.set_hspeed(r(-3.0)).unwrap();
        assert_eq!(m.direction(), r(180.0));
        m.set_hspeed(r(0.0)).unwrap();
        assert_eq!((m.speed(), m.direction()), (r(0.0), r(-0.0)));
        m.set_speed(r(2.0)).unwrap();
        // -(2 * sin(-0.0)) is +0.0.
        assert_eq!((m.hspeed(), m.vspeed()), (r(2.0), r(0.0)));
        let mut m = Motion::new_at(r(410.0), r(0.0));
        m.set_hspeed(r(-3.0)).unwrap();
        m.set_hspeed(r(0.0)).unwrap();
        m.set_speed(r(2.0)).unwrap();
        tick(&mut m);
        assert_eq!(m.x(), r(412.0));
    }

    /// M3: eat-the-rori-girl t-mot-05-a/b/c (negative speed keeps its sign and turns).
    #[test]
    fn coupling_vector_from_polar_negative_speed() {
        let mut m = Motion::new_at(r(0.0), r(0.0));
        m.set_direction(r(224.9999999999999)).unwrap();
        m.set_speed(r(-2.5)).unwrap();
        assert_eq!(m.hspeed(), r(1.7677669529663722));
        assert_eq!(m.vspeed(), r(-1.7677669529663653));
        assert_eq!(m.speed(), r(-2.5));
        // Run c: direction += 30.
        m.set_direction(m.direction() + r(30.0)).unwrap();
        assert_eq!(m.direction(), r(254.9999999999999));
        assert_eq!(m.speed(), r(-2.5));
        assert_eq!(m.hspeed(), r(0.6470476127563058));
        assert_eq!(m.vspeed(), r(-2.41481456572267));
        // Run c: hspeed = hspeed changes nothing (row 3806).
        m.set_hspeed(m.hspeed()).unwrap();
        assert_eq!((m.speed(), m.direction()), (r(-2.5), r(254.9999999999999)));
        // Run a: speed -4 at direction 323.99999999999994.
        let mut m = Motion::new_at(r(0.0), r(0.0));
        m.motion_set(r(323.99999999999994), r(-4.0)).unwrap();
        assert_eq!(m.hspeed(), r(-3.236067977499787));
        assert_eq!(m.vspeed(), r(-2.351141009169896));
        // Run a: speed -4 at direction 180 gives hspeed 4.0. The recorder reads vspeed 0.0 here;
        // the formula gives the sine residue (OPEN, reported to research).
        m.motion_set(r(180.0), r(-4.0)).unwrap();
        assert_eq!(m.hspeed(), r(4.0));
        assert_eq!(m.vspeed(), r(4.898587196589413e-16));
        // A speed write keeps the direction when the speed is 0 (U5): roll cherries read
        // hspeed -0.0 at direction 111.16511394266496.
        m.set_direction(r(111.16511394266496)).unwrap();
        m.set_speed(r(0.0)).unwrap();
        assert_eq!(m.direction(), r(111.16511394266496));
        assert_eq!(m.hspeed(), r(-0.0));
        // t-mot-05-b frame 4027: speed 16 then direction 284 gives vspeed 15.524731620415945.
        m.set_speed(r(16.0)).unwrap();
        m.set_direction(r(284.0)).unwrap();
        assert_eq!(m.vspeed(), r(15.524731620415945));
    }

    #[test]
    fn motion_add_adds_a_vector() {
        let mut m = Motion::new_at(r(0.0), r(0.0));
        m.motion_set(r(0.0), r(3.0)).unwrap();
        m.motion_add(r(90.0), r(4.0)).unwrap();
        assert_eq!(m.hspeed(), r(3.0) + r(4.0) * num::dcos(r(90.0)));
        assert_eq!(m.vspeed(), r(-4.0));
        assert_eq!(m.speed(), num::sqrt(m.hspeed() * m.hspeed() + r(16.0)));
    }

    /// M2: kaenbyou t-mot-04 id 117874 (speed 6, friction 0.2) and the friction -0.1 bullet.
    #[test]
    fn friction_sign_cases() {
        let mut m = Motion::new_at(r(640.0), r(287.0));
        m.motion_set(r(195.75988710857928), r(6.0)).unwrap();
        m.set_friction(r(0.2)).unwrap();
        tick(&mut m);
        assert_eq!(m.speed(), r(5.8));
        let mut last = m.speed();
        let mut ticks = 1;
        while m.speed().get() > 0.0 {
            last = m.speed();
            tick(&mut m);
            ticks += 1;
            assert!(ticks < 100, "friction never stopped");
        }
        // Stops at exactly 0 without reversing; the direction is kept; hspeed reads -0.0.
        assert_eq!(last, r(0.19999999999999696));
        assert_eq!(m.speed(), r(0.0));
        assert_eq!(m.direction(), r(195.75988710857928));
        assert_eq!(m.hspeed(), r(-0.0));
        tick(&mut m);
        assert_eq!(m.speed(), r(0.0));

        // Negative friction grows the size from the first tick.
        let mut m = Motion::new_at(r(0.0), r(0.0));
        m.motion_set(r(10.0), r(7.0)).unwrap();
        m.set_friction(r(-0.1)).unwrap();
        let mut speeds = Vec::new();
        for _ in 0..3 {
            tick(&mut m);
            speeds.push(m.speed());
        }
        assert_eq!(speeds, [r(7.1), r(7.199999999999999), r(7.299999999999999)]);

        // A stopped instance does not start under negative friction (U3 default).
        let mut m = Motion::new_at(r(0.0), r(0.0));
        m.set_friction(r(-0.1)).unwrap();
        tick(&mut m);
        assert_eq!((m.speed(), m.x()), (r(0.0), r(0.0)));

        // A negative speed shrinks toward 0 and stops there (U3 default).
        let mut m = Motion::new_at(r(0.0), r(0.0));
        m.motion_set(r(0.0), r(-0.3)).unwrap();
        m.set_friction(r(0.2)).unwrap();
        tick(&mut m);
        assert_eq!(m.speed(), r(-0.3) + r(0.2));
        tick(&mut m);
        assert_eq!(m.speed(), r(0.0));
    }

    /// Gravity as a vector at the table angles (Confirmed (recorder) for 270 and 180).
    #[test]
    fn gravity_vector_at_right_angles() {
        let cases = [
            // (direction, hspeed, vspeed) after one tick of gravity 0.4 from rest.
            (0.0, r(0.4), r(0.0)),
            (90.0, r(0.4) * num::dcos(r(90.0)), r(-0.4)),
            (180.0, r(-0.4), r(0.0) - r(0.4) * num::dsin(r(180.0))),
            (270.0, RESIDUE, r(0.4)),
            (-270.0, RESIDUE, r(-0.4)),
        ];
        for (dir, h, v) in cases {
            let mut m = Motion::new_at(r(0.0), r(0.0));
            m.set_gravity(r(0.4)).unwrap();
            m.set_gravity_direction(r(dir)).unwrap();
            tick(&mut m);
            assert_eq!((m.hspeed(), m.vspeed()), (h, v), "gravity_direction {dir}");
            assert_eq!(
                (m.x(), m.y()),
                (r(0.0) + h, r(0.0) + v),
                "gravity_direction {dir}"
            );
        }
        // 180 leaves the 1.2e-17 sine residue in vspeed (kaenbyou t-mot-04 windy bullets).
        assert_eq!(r(0.0) - r(0.4) * num::dsin(r(180.0)), r(-4.898587196589413e-17));
        // Negative gravity at 270 gives the positive residue (Inferred, player2).
        let mut m = Motion::new_at(r(0.0), r(0.0));
        m.set_gravity(r(-0.4)).unwrap();
        tick(&mut m);
        assert_eq!(m.hspeed(), -RESIDUE);
    }

    /// The standing kid: gravity leaves the residue in hspeed and the block event zeroes vspeed,
    /// so direction reads 180 (kaenbyou/t-mot-06.csv rows 15-34).
    #[test]
    fn standing_kid_reads_direction_180() {
        let mut m = Motion::new_at(r(401.0), r(311.0));
        m.set_gravity(r(0.4)).unwrap();
        tick(&mut m);
        assert_eq!((m.hspeed(), m.vspeed(), m.speed()), (RESIDUE, r(0.4), r(0.4)));
        assert_eq!(m.direction(), r(270.0));
        assert_eq!(m.y(), r(311.4));
        m.set_vspeed(r(0.0)).unwrap();
        assert_eq!((m.speed(), m.direction()), (r(7.347880794884119e-17), r(180.0)));
    }

    /// The fall table, variant (a) (CLONE_SPEC motion, showdown/t-mot-01.csv): vspeed 0.4k for k
    /// = 1..22 (8.800000000000002 at k = 22), 9.200000000000003 at k = 23, 9.4 from then on;
    /// y moves by vspeed. Step sets hspeed 0 and clamps `abs(vspeed) > 9` to `sign(vspeed) * 9`.
    #[test]
    fn fall_table_variant_a() {
        let n = NumRules::default();
        let mut m = Motion::new_at(r(113.0), r(215.0));
        let mut vs = Vec::new();
        let mut ys = Vec::new();
        for _ in 0..30 {
            // Step.
            m.set_gravity(r(0.4)).unwrap();
            m.set_hspeed(r(0.0)).unwrap();
            let v = m.vspeed();
            if num::gm_gt(num::abs(v), r(9.0), &n) {
                m.set_vspeed(num::sign(v, &n) * r(9.0)).unwrap();
            }
            let y0 = m.y();
            tick(&mut m);
            assert_eq!(m.y(), y0 + m.vspeed());
            assert_eq!(m.hspeed(), RESIDUE);
            vs.push(m.vspeed());
            ys.push(m.y());
        }
        let mut sum = r(0.0);
        for (k, v) in vs.iter().take(22).enumerate() {
            sum += r(0.4);
            assert_eq!(*v, sum, "k = {}", k + 1);
        }
        assert_eq!(vs[0], r(0.4));
        assert_eq!(vs[2], r(1.2000000000000002));
        assert_eq!(vs[21], r(8.800000000000002));
        assert_eq!(vs[22], r(9.200000000000003));
        assert!(vs[23..].iter().all(|v| *v == r(9.4)));
        // t-mot-01.csv rows 2, 3, 23, 25 and 30.
        assert_eq!(ys[0], r(215.4));
        assert_eq!(ys[1], r(216.20000000000002));
        assert_eq!(ys[21], r(316.2));
        assert_eq!(ys[23], r(334.79999999999995));
        assert_eq!(ys[28], r(381.79999999999984));
    }

    #[test]
    fn non_finite_setters_error_and_keep_state() {
        let mut m = Motion::new_at(r(1.0), r(2.0));
        m.set_hspeed(r(3.0)).unwrap();
        let before = m.clone();
        let bad = [r(f64::NAN), r(f64::INFINITY), r(f64::NEG_INFINITY)];
        for v in bad {
            type Setter = fn(&mut Motion, Real) -> Result<(), SimError>;
            let setters: [(&str, Setter); 13] = [
                ("x", Motion::set_x),
                ("y", Motion::set_y),
                ("xstart", Motion::set_xstart),
                ("ystart", Motion::set_ystart),
                ("xprevious", Motion::set_xprevious),
                ("yprevious", Motion::set_yprevious),
                ("hspeed", Motion::set_hspeed),
                ("vspeed", Motion::set_vspeed),
                ("speed", Motion::set_speed),
                ("direction", Motion::set_direction),
                ("gravity", Motion::set_gravity),
                ("gravity_direction", Motion::set_gravity_direction),
                ("friction", Motion::set_friction),
            ];
            for (name, set) in setters {
                let err = set(&mut m, v).expect_err(name);
                assert!(matches!(err, SimError::NonFinite { .. }), "{name}: {err:?}");
                assert_eq!(m, before, "{name} changed state");
            }
            assert!(matches!(m.motion_set(v, r(1.0)), Err(SimError::NonFinite { .. })));
            assert!(matches!(m.motion_set(r(1.0), v), Err(SimError::NonFinite { .. })));
            assert!(matches!(m.motion_add(v, r(1.0)), Err(SimError::NonFinite { .. })));
            assert_eq!(m, before);
        }
        // A finite value whose recompute overflows is rejected too.
        assert!(matches!(m.set_hspeed(r(1e300)), Err(SimError::NonFinite { .. })));
        assert_eq!(m, before);
        // Automatic motion that would leave the finite range halts.
        let mut m = Motion::new_at(r(f64::MAX), r(0.0));
        m.set_hspeed(r(f64::MAX)).expect_err("speed overflows");
        m.set_speed(r(f64::MAX)).unwrap();
        let err = apply_auto_motion(&mut m, None, &def(), &MotionRules::default());
        assert!(matches!(err, Err(SimError::NonFinite { .. })));
    }

    #[test]
    fn capture_previous_copies_position() {
        let mut m = Motion::new_at(r(1.0), r(2.0));
        m.set_hspeed(r(3.0)).unwrap();
        tick(&mut m);
        assert_eq!((m.xprevious(), m.x()), (r(1.0), r(4.0)));
        m.capture_previous();
        assert_eq!((m.xprevious(), m.yprevious()), (r(4.0), r(2.0)));
    }
}
