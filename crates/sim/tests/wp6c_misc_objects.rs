//! WP6c.3 (#138): time_limit, sign, ladder, decor (docs/plans/sim-core.md 6.1).

mod wp6c {
    use iwbtg_sim::builtins;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, ParamValue, Params, RoomDef};
    use iwbtg_sim::events::EventKind;
    use iwbtg_sim::input::{Action, InputFrame};
    use iwbtg_sim::num::Real;
    use iwbtg_sim::output::OutputEvent;
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, World};

    fn r(v: f64) -> Real {
        Real::new(v)
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

    /// A game with a kid (class KID, 20 x 32) and `extra` kinds (32 x 32 sprites).
    fn game(extra: Vec<KindDef>) -> G {
        let mut b = GameDefBuilder::new("wp6c-3");
        let spr = b.sprite_rect((32, 32), (0, 0));
        let tall = b.sprite_rect((20, 32), (0, 0));
        let mut k = KindDef::new("kid", Builtin::Kid);
        k.classes = ClassMask::KID;
        k.sprite = Some(tall);
        let kid = b.kind(k);
        b.engine_kind(iwbtg_sim::def::EngineKind::Kid, kid);
        for mut kd in extra {
            kd.sprite = Some(spr);
            b.kind(kd);
        }
        let room = b.room(RoomDef::new("r", 800, 608));
        b.start_room(room);
        let mut def = b.build().unwrap();
        for k in &mut def.kinds {
            k.events = builtins::event_set(k.base, &k.clone());
        }
        let mut w = World::new(room, 1);
        w.room_speed = r(50.0);
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

    fn left(g: &G, id: InstId) -> (i64, i64, i64) {
        match &g.w.get(id).unwrap().state {
            KindState::Builtin(builtins::BuiltinState::TimeLimit(t)) => (t.hours, t.minutes, t.seconds),
            _ => panic!("not a time limit"),
        }
    }

    fn timer_game(p: &[(&str, ParamValue)]) -> G {
        let mut tp = vec![("role", word("timer"))];
        tp.extend_from_slice(p);
        game(vec![
            kind_of("timer", Builtin::TimeLimit, params(&tp)),
            kind_of(
                "zone",
                Builtin::TimeLimit,
                params(&[("role", word("start_zone"))]),
            ),
        ])
    }

    /// One alarm period: alarm 0 fires, then Step.
    fn second(g: &mut G, t: InstId) {
        fire(g, t, &EventKind::Alarm(0));
        if alive(g, t) {
            fire(g, t, &EventKind::Step);
        }
    }

    // ---- time_limit -------------------------------------------------------------------------

    #[test]
    fn touching_the_zone_arms_every_timer_and_destroys_the_zone() {
        let mut g = timer_game(&[("seconds", real(5.0)), ("on_expire", word("nothing"))]);
        let kid = spawn(&mut g, "kid", 0.0, 0.0);
        let t = spawn(&mut g, "timer", 100.0, 100.0);
        let z = spawn(&mut g, "zone", 200.0, 100.0);
        assert!(g.w.get(t).unwrap().alarms[0].get() < 0.0);
        touch(&mut g, z, kid, ClassMask::KID);
        assert!(!alive(&g, z));
        assert_eq!(g.w.get(t).unwrap().alarms[0].get(), 50.0);
    }

    #[test]
    fn each_alarm_takes_a_second_and_expiry_comes_one_period_after_zero() {
        let mut g = timer_game(&[("seconds", real(2.0)), ("on_expire", word("nothing"))]);
        spawn(&mut g, "kid", 0.0, 0.0);
        let t = spawn(&mut g, "timer", 100.0, 100.0);
        second(&mut g, t);
        assert_eq!(left(&g, t), (0, 0, 1));
        second(&mut g, t);
        assert_eq!(left(&g, t), (0, 0, 0));
        assert!(
            alive(&g, t),
            "the display reads 0:00:00 and the timer is still there"
        );
        assert_eq!(g.w.get(t).unwrap().alarms[0].get(), 50.0, "it re-arms itself");
        second(&mut g, t);
        assert!(!alive(&g, t));
    }

    #[test]
    fn negative_seconds_borrow_from_minutes_then_hours() {
        let mut g = timer_game(&[("hours", real(1.0)), ("seconds", real(0.0))]);
        spawn(&mut g, "kid", 0.0, 0.0);
        let t = spawn(&mut g, "timer", 100.0, 100.0);
        second(&mut g, t);
        assert_eq!(left(&g, t), (0, 59, 59));
        let mut g = timer_game(&[("minutes", real(2.0)), ("seconds", real(0.0))]);
        spawn(&mut g, "kid", 0.0, 0.0);
        let t = spawn(&mut g, "timer", 100.0, 100.0);
        second(&mut g, t);
        assert_eq!(left(&g, t), (0, 1, 59));
    }

    #[test]
    fn the_clock_waits_while_no_kid_exists() {
        let mut g = timer_game(&[("seconds", real(5.0))]);
        let t = spawn(&mut g, "timer", 100.0, 100.0);
        second(&mut g, t);
        assert_eq!(left(&g, t), (0, 0, 5));
        assert_eq!(g.w.get(t).unwrap().alarms[0].get(), 50.0);
    }

    #[test]
    fn time_limit_params_are_checked() {
        let bad =
            |items: &[(&str, ParamValue)]| builtins::init_state(Builtin::TimeLimit, &params(items)).is_err();
        assert!(bad(&[("on_expire", word("goto_room"))]));
        assert!(bad(&[("minutes", real(-1.0))]));
        assert!(bad(&[("seconds", real(1.5))]));
        assert!(bad(&[("on_expire", word("explode"))]));
        assert!(!bad(&[("hours", real(1.0)), ("on_expire", word("kill"))]));
    }

    // ---- sign -------------------------------------------------------------------------------

    fn texts(g: &mut G) -> Vec<String> {
        g.w.output
            .drain(..)
            .filter_map(|e| match e {
                OutputEvent::Builtin(builtins::BuiltinOutput::TextBox { text }) => Some(text),
                _ => None,
            })
            .collect()
    }

    fn up_pressed(g: &mut G) {
        let bit = Action::Up.bit();
        g.w.input.latch(InputFrame {
            held: bit,
            pressed: bit,
            released: 0,
        });
    }

    #[test]
    fn an_up_press_sign_speaks_only_on_an_up_press_while_a_kid_overlaps() {
        let mut g = game(vec![kind_of(
            "sign",
            Builtin::Sign,
            params(&[("text", word("hello"))]),
        )]);
        spawn(&mut g, "kid", 100.0, 100.0);
        let s = spawn(&mut g, "sign", 100.0, 100.0);
        fire(&mut g, s, &EventKind::Step);
        assert!(texts(&mut g).is_empty(), "no press, no text");
        up_pressed(&mut g);
        fire(&mut g, s, &EventKind::Step);
        assert_eq!(texts(&mut g), vec!["hello".to_owned()]);
        let far = spawn(&mut g, "sign", 500.0, 500.0);
        fire(&mut g, far, &EventKind::Step);
        assert!(texts(&mut g).is_empty(), "a sign the kid is not on stays quiet");
    }

    #[test]
    fn an_overlap_sign_speaks_every_tick_a_kid_touches_it_and_ignores_the_step() {
        let mut g = game(vec![kind_of(
            "sign",
            Builtin::Sign,
            params(&[("text", word("hi")), ("mode", word("while_overlapping"))]),
        )]);
        let kid = spawn(&mut g, "kid", 100.0, 100.0);
        let s = spawn(&mut g, "sign", 100.0, 100.0);
        up_pressed(&mut g);
        fire(&mut g, s, &EventKind::Step);
        assert!(texts(&mut g).is_empty());
        touch(&mut g, s, kid, ClassMask::KID);
        touch(&mut g, s, kid, ClassMask::KID);
        assert_eq!(texts(&mut g).len(), 2);
    }

    #[test]
    fn a_sign_needs_its_text_and_a_known_mode() {
        let bad = |items: &[(&str, ParamValue)]| builtins::init_state(Builtin::Sign, &params(items)).is_err();
        assert!(bad(&[]));
        assert!(bad(&[("text", word("x")), ("mode", word("always"))]));
        assert!(!bad(&[("text", word("x"))]));
    }

    // ---- ladder and decor -------------------------------------------------------------------

    #[test]
    fn ladder_and_decor_have_no_events_no_params_and_no_state() {
        for base in [Builtin::Ladder, Builtin::Decor] {
            assert!(builtins::param_specs(base).is_empty());
            let k = KindDef::new("k", base);
            assert_eq!(
                builtins::event_set(base, &k),
                iwbtg_sim::events::EventSet::default()
            );
            assert!(builtins::init_state(base, &Params::default()).is_ok());
        }
    }
}
