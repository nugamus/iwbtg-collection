//! WP2 path tests (docs/plans/sim-core.md 3.1, M8). The traced case is
//! `traces/rely-the-needle-4-2/t-evo-1.csv` (CLONE_SPEC motion U9, Confirmed (recorder)); the end
//! actions are the M8 Design defaults.

mod wp2 {
    use iwbtg_sim::SimError;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{PathDef, PathId, RoomDef};
    use iwbtg_sim::motion::{Motion, apply_auto_motion};
    use iwbtg_sim::num::Real;
    use iwbtg_sim::path::{self, PathEnd, PathRun};
    use iwbtg_sim::rules::MotionRules;

    fn r(x: f64) -> Real {
        Real::new(x)
    }

    fn line(name: &str, points: &[(f64, f64)], closed: bool) -> PathDef {
        PathDef {
            name: name.into(),
            points: points.iter().map(|&(x, y)| (r(x), r(y), r(100.0))).collect(),
            closed,
            smooth: false,
            precision: 4,
        }
    }

    fn game(paths: Vec<PathDef>) -> iwbtg_sim::GameDef {
        let mut b = GameDefBuilder::new("wp2");
        b.room(RoomDef::new("room1", 800, 608));
        b.def_mut().paths = paths;
        b.build().expect("one-room game builds")
    }

    fn tick(m: &mut Motion, run: &mut PathRun, def: &iwbtg_sim::GameDef) -> bool {
        apply_auto_motion(m, Some(run), def, &MotionRules::default())
            .expect("motion")
            .end_of_path
    }

    /// t-evo-1: probe B at (340, 450) on a 40 px path at speed 4 moves 4 px and +0.1 position per
    /// tick, reads speed 0, reads position 0.9999999999999999 after ten ticks (x 380), and gets
    /// End of Path on the eleventh, when the position reads 1.0 and x stays 380.
    #[test]
    fn traced_40px_path() {
        let def = game(vec![line("p", &[(0.0, 0.0), (40.0, 0.0)], false)]);
        let mut m = Motion::new_at(r(340.0), r(450.0));
        let mut run = path::start(&m, PathId(0), r(4.0), PathEnd::Stop, false).expect("start");
        let mut xs = Vec::new();
        let mut ends = Vec::new();
        for _ in 0..12 {
            ends.push(tick(&mut m, &mut run, &def));
            xs.push(m.x());
            assert_eq!(m.speed(), r(0.0));
        }
        assert_eq!(xs[0], r(344.0));
        assert_eq!(xs[8], r(376.0));
        assert_eq!(run_position_after(&def, 10), r(0.9999999999999999));
        assert_eq!(xs[9], r(380.0));
        assert_eq!(xs[10], r(380.0));
        assert_eq!(run.position, r(1.0));
        assert_eq!(ends.iter().filter(|e| **e).count(), 1);
        assert!(ends[10]);
        assert_eq!(m.y(), r(450.0));
    }

    fn run_position_after(def: &iwbtg_sim::GameDef, ticks: usize) -> Real {
        let mut m = Motion::new_at(r(340.0), r(450.0));
        let mut run = path::start(&m, PathId(0), r(4.0), PathEnd::Stop, false).expect("start");
        for _ in 0..ticks {
            tick(&mut m, &mut run, def);
        }
        run.position
    }

    #[test]
    fn end_actions() {
        let def = game(vec![line("p", &[(0.0, 0.0), (40.0, 0.0)], false)]);
        // Restart: wraps and jumps back to the start.
        let mut m = Motion::new_at(r(0.0), r(0.0));
        let mut run = path::start(&m, PathId(0), r(30.0), PathEnd::Restart, false).unwrap();
        assert!(!tick(&mut m, &mut run, &def));
        assert!(tick(&mut m, &mut run, &def));
        assert_eq!(run.position, r(0.75) + r(0.75) - r(1.0));
        assert_eq!(m.x(), r(20.0));
        // Continue: the next lap starts where this one ended.
        let mut m = Motion::new_at(r(0.0), r(0.0));
        let mut run = path::start(&m, PathId(0), r(30.0), PathEnd::Continue, false).unwrap();
        tick(&mut m, &mut run, &def);
        assert!(tick(&mut m, &mut run, &def));
        assert_eq!(m.x(), r(60.0));
        // Reverse: the speed turns around and the position reflects.
        let mut m = Motion::new_at(r(0.0), r(0.0));
        let mut run = path::start(&m, PathId(0), r(30.0), PathEnd::Reverse, false).unwrap();
        tick(&mut m, &mut run, &def);
        assert!(tick(&mut m, &mut run, &def));
        assert_eq!(run.speed, r(-30.0));
        assert_eq!(run.position, r(0.5));
        assert_eq!(m.x(), r(20.0));
        // path_end drops the run.
        let mut slot = Some(run);
        path::end(&mut slot);
        assert_eq!(slot, None);
    }

    #[test]
    fn closed_and_absolute_paths() {
        let square = line(
            "sq",
            &[(100.0, 100.0), (110.0, 100.0), (110.0, 110.0), (100.0, 110.0)],
            true,
        );
        let def = game(vec![square]);
        let mut m = Motion::new_at(r(0.0), r(0.0));
        // Perimeter 40 at speed 5: 1/8 of the path per tick.
        let mut run = path::start(&m, PathId(0), r(5.0), PathEnd::Restart, true).unwrap();
        let mut at = Vec::new();
        let mut ends = Vec::new();
        for _ in 0..9 {
            ends.push(tick(&mut m, &mut run, &def));
            at.push((m.x(), m.y()));
        }
        assert_eq!(at[0], (r(105.0), r(100.0)));
        assert_eq!(at[2], (r(110.0), r(105.0)));
        assert_eq!(at[4], (r(105.0), r(110.0)));
        // The closing segment back to the first point.
        assert_eq!(at[6], (r(100.0), r(105.0)));
        // The lap ends on the first point and restarts.
        assert_eq!(at[7], (r(100.0), r(100.0)));
        assert_eq!(at[8], (r(105.0), r(100.0)));
        assert_eq!(
            ends,
            [false, false, false, false, false, false, false, true, false]
        );
    }

    #[test]
    fn unsupported_and_invalid_paths() {
        let mut smooth = line("s", &[(0.0, 0.0), (10.0, 0.0)], false);
        smooth.smooth = true;
        let def = game(vec![smooth, line("dot", &[(5.0, 5.0)], false)]);
        let mut m = Motion::new_at(r(0.0), r(0.0));
        let mut run = path::start(&m, PathId(0), r(1.0), PathEnd::Stop, false).unwrap();
        let err = apply_auto_motion(&mut m, Some(&mut run), &def, &MotionRules::default());
        assert!(matches!(err, Err(SimError::Unsupported { .. })));
        run.path = PathId(1);
        let err = apply_auto_motion(&mut m, Some(&mut run), &def, &MotionRules::default());
        assert!(matches!(err, Err(SimError::Unsupported { .. })));
        run.path = PathId(7);
        let err = apply_auto_motion(&mut m, Some(&mut run), &def, &MotionRules::default());
        assert!(matches!(err, Err(SimError::Invariant { .. })));
        assert!(matches!(
            path::start(&m, PathId(0), r(f64::NAN), PathEnd::Stop, false),
            Err(SimError::NonFinite { .. })
        ));
    }
}
