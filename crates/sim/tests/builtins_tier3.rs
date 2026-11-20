//! WP6c.2 (#137): breakable_block, trap_stopper, orbit, path_platform (docs/plans/sim-core.md 6.1).

mod wp6c {
    use iwbtg_sim::builtins;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{
        Builtin, ClassMask, GameDef, KindDef, KindId, ParamValue, Params, PathDef, PathId, RoomDef,
    };
    use iwbtg_sim::events::EventKind;
    use iwbtg_sim::num::Real;
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::world::{Ctx, InstId, SpawnInit, World};

    fn r(v: f64) -> Real {
        Real::new(v)
    }

    fn deg(d: f64) -> f64 {
        iwbtg_sim::num::trig::degtorad(r(d)).get()
    }

    fn cosr(a: f64) -> f64 {
        iwbtg_sim::num::cos(r(a)).get()
    }

    fn sinr(a: f64) -> f64 {
        iwbtg_sim::num::sin(r(a)).get()
    }

    fn real(v: f64) -> ParamValue {
        ParamValue::Real(r(v))
    }

    fn word(s: &str) -> ParamValue {
        ParamValue::Str(s.to_owned())
    }

    fn params(items: &[(&str, ParamValue)]) -> Params {
        let mut p = Params::default();
        for (k, v) in items {
            p.0.insert((*k).to_owned(), v.clone());
        }
        p
    }

    struct G {
        def: GameDef,
        w: World,
    }

    /// A game with a kid (class KID, 20 x 32), a bullet, and `extra` kinds (32 x 32 sprites).
    fn game(extra: Vec<KindDef>) -> G {
        let mut b = GameDefBuilder::new("wp6c-2");
        let spr = b.sprite_rect((32, 32), (0, 0));
        let tall = b.sprite_rect((20, 32), (0, 0));
        let mut k = KindDef::new("kid", Builtin::Kid);
        k.classes = ClassMask::KID;
        k.sprite = Some(tall);
        let kid = b.kind(k);
        let mut bullet = KindDef::new("bullet", Builtin::Inert);
        bullet.classes = ClassMask::KID_BULLET;
        bullet.sprite = Some(spr);
        b.kind(bullet);
        b.engine_kind(iwbtg_sim::def::EngineKind::Kid, kid);
        for mut kd in extra {
            kd.sprite = Some(spr);
            b.kind(kd);
        }
        let room = b.room(RoomDef::new("r", 800, 608));
        b.start_room(room);
        let mut def = b.build().unwrap();
        def.paths.push(PathDef {
            name: "p".into(),
            points: vec![(r(0.0), r(0.0), r(100.0)), (r(100.0), r(0.0), r(100.0))],
            closed: false,
            smooth: false,
            precision: 4,
        });
        for k in &mut def.kinds {
            k.events = builtins::event_set(k.base, &k.clone());
        }
        let w = World::new(room, 1);
        G { def, w }
    }

    fn kind_of(name: &str, base: Builtin, p: Params) -> KindDef {
        let mut k = KindDef::new(name, base);
        k.params = p;
        k
    }

    fn kind(g: &G, name: &str) -> KindId {
        KindId(u32::try_from(g.def.kinds.iter().position(|k| k.name == name).unwrap()).unwrap())
    }

    fn with<T>(g: &mut G, f: impl FnOnce(&mut Ctx) -> T) -> T {
        let rules = RunnerRules::default();
        let scripts = ScriptRuntime::default();
        let mut ctx = Ctx {
            def: &g.def,
            rules: &rules,
            w: &mut g.w,
            scripts: &scripts,
            dll_mode: DllMode::Stub,
        };
        f(&mut ctx)
    }

    fn spawn(g: &mut G, name: &str, x: f64, y: f64) -> InstId {
        let k = kind(g, name);
        with(g, |c| c.spawn(k, r(x), r(y), SpawnInit::default()).unwrap())
    }

    fn fire(g: &mut G, id: InstId, ev: &EventKind) {
        with(g, |c| builtins::on_event(c, id, ev).unwrap());
    }

    fn touch(g: &mut G, id: InstId, other: InstId, class: ClassMask) {
        fire(g, id, &EventKind::Collision { with: class, other });
    }

    fn alive(g: &G, id: InstId) -> bool {
        g.w.get(id).is_some_and(|i| i.alive)
    }

    fn pos(g: &G, id: InstId) -> (f64, f64) {
        let m = &g.w.get(id).unwrap().m;
        (m.x().get(), m.y().get())
    }

    // ---- breakable_block --------------------------------------------------------------------

    #[test]
    fn shot_breaks_the_block_and_a_kid_touch_does_not() {
        let mut g = game(vec![kind_of(
            "brk",
            Builtin::BreakableBlock,
            params(&[("break_on", word("shot"))]),
        )]);
        let kid = spawn(&mut g, "kid", 0.0, 0.0);
        let bullet = spawn(&mut g, "bullet", 0.0, 0.0);
        let b = spawn(&mut g, "brk", 100.0, 100.0);
        touch(&mut g, b, kid, ClassMask::KID);
        assert!(alive(&g, b));
        touch(&mut g, b, bullet, ClassMask::KID_BULLET);
        assert!(!alive(&g, b));
    }

    #[test]
    fn touch_delay_runs_its_stages_then_destroys() {
        let p = params(&[("break_on", word("touch_delay")), ("stages", real(3.0))]);
        let mut g = game(vec![kind_of("brk", Builtin::BreakableBlock, p)]);
        let kid = spawn(&mut g, "kid", 0.0, 0.0);
        let b = spawn(&mut g, "brk", 100.0, 100.0);
        touch(&mut g, b, kid, ClassMask::KID);
        assert_eq!(g.w.get(b).unwrap().alarms[0], r(50.0));
        // A second touch does not re-arm it.
        g.w.get_mut(b).unwrap().alarms[0] = r(20.0);
        touch(&mut g, b, kid, ClassMask::KID);
        assert_eq!(g.w.get(b).unwrap().alarms[0], r(20.0));
        fire(&mut g, b, &EventKind::Alarm(0));
        assert_eq!(g.w.get(b).unwrap().img.index(), r(1.0));
        fire(&mut g, b, &EventKind::Alarm(0));
        assert_eq!(g.w.get(b).unwrap().img.index(), r(2.0));
        assert!(alive(&g, b));
        fire(&mut g, b, &EventKind::Alarm(0));
        assert!(!alive(&g, b), "destroyed after the third stage");
    }

    #[test]
    fn one_stage_block_is_destroyed_after_the_first_alarm() {
        let p = params(&[("break_on", word("touch_delay"))]);
        let mut g = game(vec![kind_of("brk", Builtin::BreakableBlock, p)]);
        let kid = spawn(&mut g, "kid", 0.0, 0.0);
        let b = spawn(&mut g, "brk", 100.0, 100.0);
        touch(&mut g, b, kid, ClassMask::KID);
        fire(&mut g, b, &EventKind::Alarm(0));
        assert!(!alive(&g, b));
    }

    #[test]
    fn breakable_checks_its_counts() {
        let bad = params(&[("stages", real(0.0))]);
        assert!(builtins::init_state(Builtin::BreakableBlock, &bad).is_err());
    }

    // ---- trap_stopper -----------------------------------------------------------------------

    #[test]
    fn stop_stopper_has_no_events_and_destroy_stopper_runs_in_step() {
        let g = game(vec![
            kind_of("stop", Builtin::TrapStopper, params(&[("mode", word("stop"))])),
            kind_of("kill", Builtin::TrapStopper, params(&[("mode", word("destroy"))])),
        ]);
        let ev = |name: &str| {
            g.def.kinds[usize::try_from(kind(&g, name).0).unwrap()]
                .events
                .clone()
        };
        assert!(!ev("stop").step);
        assert!(ev("kill").step);
    }

    #[test]
    fn destroy_stopper_removes_the_traps_it_overlaps() {
        let trap = params(&[("trigger_id", real(1.0))]);
        let mut g = game(vec![
            kind_of("kill", Builtin::TrapStopper, params(&[("mode", word("destroy"))])),
            kind_of("trap", Builtin::Trap, trap),
        ]);
        let stopper = spawn(&mut g, "kill", 100.0, 100.0);
        let over = spawn(&mut g, "trap", 110.0, 110.0);
        let away = spawn(&mut g, "trap", 400.0, 400.0);
        fire(&mut g, stopper, &EventKind::Step);
        assert!(!alive(&g, over));
        assert!(alive(&g, away));
    }

    // ---- orbit ------------------------------------------------------------------------------

    fn orbit_game(p: &[(&str, ParamValue)]) -> G {
        game(vec![kind_of("orb", Builtin::Orbit, params(p))])
    }

    #[test]
    fn orbiter_starts_mirrored_across_the_center_then_turns() {
        // object761: placed at (1840, 912), center (1840, 976): first position (1840, 1040).
        let mut g = orbit_game(&[
            ("center_x", real(1840.0)),
            ("center_y", real(976.0)),
            ("speed", real(1.0)),
        ]);
        let o = spawn(&mut g, "orb", 1840.0, 912.0);
        fire(&mut g, o, &EventKind::Step);
        let (x, y) = pos(&g, o);
        assert!(
            (x - 1840.0).abs() < 1e-9 && (y - 1040.0).abs() < 1e-9,
            "({x}, {y})"
        );
        fire(&mut g, o, &EventKind::Step);
        let (x2, y2) = pos(&g, o);
        // One degree on, radius 64: x = cx + r cos(271), y = cy - r sin(271).
        let a = deg(271.0);
        assert!((x2 - (1840.0 + 64.0 * cosr(a))).abs() < 1e-9);
        assert!((y2 - (976.0 - 64.0 * sinr(a))).abs() < 1e-9);
    }

    #[test]
    fn radians_orbiter_wraps_its_angle_into_zero_to_two_pi() {
        let mut g = orbit_game(&[
            ("center_x", real(100.0)),
            ("center_y", real(100.0)),
            ("radius", real(50.0)),
            ("angle", real(6.0)),
            ("speed", real(1.0)),
            ("unit", word("radians")),
        ]);
        let o = spawn(&mut g, "orb", 0.0, 0.0);
        fire(&mut g, o, &EventKind::Step);
        let (x, y) = pos(&g, o);
        assert!((x - (100.0 + 50.0 * cosr(6.0))).abs() < 1e-9);
        assert!((y - (100.0 - 50.0 * sinr(6.0))).abs() < 1e-9);
        fire(&mut g, o, &EventKind::Step);
        let (x2, _) = pos(&g, o);
        let wrapped = 7.0 - std::f64::consts::TAU;
        assert!((x2 - (100.0 + 50.0 * cosr(wrapped))).abs() < 1e-9);
    }

    #[test]
    fn spawner_makes_evenly_spaced_orbiters_and_goes_away() {
        let child = kind_of(
            "orb",
            Builtin::Orbit,
            params(&[("center_x", real(0.0)), ("center_y", real(0.0))]),
        );
        // Kinds: kid 0, bullet 1, then orb 2 and the maker.
        let orb = KindId(2);
        let spawner = kind_of(
            "maker",
            Builtin::Orbit,
            params(&[
                ("mode", word("spawner")),
                ("child", ParamValue::Kind(orb)),
                ("count", real(4.0)),
                ("center_x", real(200.0)),
                ("center_y", real(200.0)),
                ("radius", real(32.0)),
                ("speed", real(2.0)),
            ]),
        );
        let mut g = game(vec![child, spawner]);
        assert_eq!(kind(&g, "orb"), orb);
        let before = g.w.instances().len();
        let maker = spawn(&mut g, "maker", 0.0, 0.0);
        assert!(!alive(&g, maker), "the spawner destroys itself");
        let kids: Vec<InstId> =
            g.w.instances()
                .iter()
                .filter(|i| i.alive && i.kind == orb)
                .map(|i| i.id)
                .collect();
        assert_eq!(kids.len(), 4);
        assert!(g.w.instances().len() > before);
        for (i, id) in kids.iter().enumerate() {
            fire(&mut g, *id, &EventKind::Step);
            let a = deg(90.0 * i as f64);
            let (x, y) = pos(&g, *id);
            assert!((x - (200.0 + 32.0 * cosr(a))).abs() < 1e-9, "x of {i}");
            assert!((y - (200.0 - 32.0 * sinr(a))).abs() < 1e-9, "y of {i}");
        }
    }

    #[test]
    fn orbit_needs_a_center_and_a_spawner_needs_a_child() {
        assert!(builtins::init_state(Builtin::Orbit, &Params::default()).is_err());
        let p = params(&[
            ("mode", word("spawner")),
            ("center_x", real(0.0)),
            ("center_y", real(0.0)),
        ]);
        assert!(builtins::init_state(Builtin::Orbit, &p).is_err());
    }

    // ---- path_platform ----------------------------------------------------------------------

    fn path_game(extra: &[(&str, ParamValue)]) -> G {
        let mut items = vec![("path", real(0.0)), ("speed", real(2.0))];
        items.extend_from_slice(extra);
        game(vec![kind_of("pp", Builtin::PathPlatform, params(&items))])
    }

    #[test]
    fn path_platform_starts_its_path_in_create() {
        let mut g = path_game(&[]);
        let pp = spawn(&mut g, "pp", 50.0, 50.0);
        let run = g.w.get(pp).unwrap().path.clone().expect("path started");
        assert_eq!(run.path, PathId(0));
        assert_eq!(run.speed, r(2.0));
    }

    #[test]
    fn touch_mode_waits_and_starts_slightly_faster() {
        let mut g = path_game(&[("start_on_touch", ParamValue::Bool(true)), ("scale", real(2.0))]);
        let kid = spawn(&mut g, "kid", 0.0, 0.0);
        let pp = spawn(&mut g, "pp", 50.0, 50.0);
        assert!(g.w.get(pp).unwrap().path.is_none());
        touch(&mut g, pp, kid, ClassMask::KID);
        let run = g.w.get(pp).unwrap().path.clone().expect("started");
        assert_eq!(run.speed, r(2.0001));
        assert_eq!(run.scale, r(2.0));
    }

    #[test]
    fn path_platform_carries_a_kid_on_both_lines_when_it_moved() {
        let mut g = path_game(&[("start_on_touch", ParamValue::Bool(true))]);
        let pp = spawn(&mut g, "pp", 100.0, 100.0);
        // A 20 x 32 kid whose body spans both lines (y 83 and 98 for a platform at y 100).
        let kid = spawn(&mut g, "kid", 105.0, 70.0);
        {
            let i = g.w.get_mut(pp).unwrap();
            i.m.set_x(r(103.0)).unwrap();
            let mut run =
                iwbtg_sim::path::start(&i.m, PathId(0), r(2.0), iwbtg_sim::path::PathEnd::Stop, false)
                    .unwrap();
            run.position = r(0.5);
            i.path = Some(run);
        }
        // xprevious is 100: the platform moved 3 px this tick.
        fire(&mut g, pp, &EventKind::EndStep);
        assert_eq!(pos(&g, kid), (108.0, 70.0));
        // The position did not change again: no carry.
        fire(&mut g, pp, &EventKind::EndStep);
        assert_eq!(pos(&g, kid), (108.0, 70.0));
    }

    #[test]
    fn path_platform_needs_a_path_index() {
        assert!(builtins::init_state(Builtin::PathPlatform, &Params::default()).is_err());
        let bad = params(&[("path", real(1.5))]);
        assert!(builtins::init_state(Builtin::PathPlatform, &bad).is_err());
    }
}
