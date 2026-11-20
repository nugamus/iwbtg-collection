//! WP6c.1 (#136): jump_refresher, spring, conveyor, kid_param_zone (docs/plans/sim-core.md 6.1).
//! Hand-built worlds with a real kid.

mod wp6c {
    use iwbtg_sim::builtins;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, ParamValue, Params, RoomDef};
    use iwbtg_sim::events::EventKind;
    use iwbtg_sim::kid::api::{self, KidField};
    use iwbtg_sim::num::Real;
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::world::{Ctx, InstId, SpawnInit, Value, World};

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

    /// A game with a kid (class KID, 8 x 8) and one `thing` of `base` with `p` (32 x 32).
    fn game(base: Builtin, p: Params) -> G {
        let mut b = GameDefBuilder::new("wp6c-1");
        let spr = b.sprite_rect((32, 32), (0, 0));
        let small = b.sprite_rect((8, 8), (0, 0));
        let mut k = KindDef::new("kid", Builtin::Kid);
        k.classes = ClassMask::KID;
        k.sprite = Some(small);
        let kid = b.kind(k);
        let mut thing = KindDef::new("thing", base);
        thing.params = p;
        thing.sprite = Some(spr);
        b.kind(thing);
        b.engine_kind(iwbtg_sim::def::EngineKind::Kid, kid);
        let room = b.room(RoomDef::new("r", 800, 608));
        b.start_room(room);
        let mut def = b.build().unwrap();
        for k in &mut def.kinds {
            k.events = builtins::event_set(k.base, &k.clone());
        }
        let w = World::new(room, 1);
        G { def, w }
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

    fn touch(g: &mut G, id: InstId, kid: InstId) {
        fire(
            g,
            id,
            &EventKind::Collision {
                with: ClassMask::KID,
                other: kid,
            },
        );
    }

    fn read(g: &G, kid: InstId, name: &str) -> f64 {
        let f = KidField {
            name: name.into(),
            index: None,
        };
        match api::read_field(&g.w, kid, &f) {
            Some(Value::Real(v)) => v.get(),
            other => panic!("{name}: {other:?}"),
        }
    }

    fn djump(g: &G, kid: InstId) -> bool {
        read(g, kid, "djump") > 0.0
    }

    fn pos(g: &G, id: InstId) -> (f64, f64) {
        let m = &g.w.get(id).unwrap().m;
        (m.x().get(), m.y().get())
    }

    // ---- jump_refresher ---------------------------------------------------------------------

    #[test]
    fn refresher_gives_the_air_jump_and_hides_for_fifty_ticks() {
        let mut g = game(Builtin::JumpRefresher, Params::default());
        let kid = spawn(&mut g, "kid", 100.0, 100.0);
        let rf = spawn(&mut g, "thing", 120.0, 90.0);
        assert!(!djump(&g, kid));
        touch(&mut g, rf, kid);
        assert!(djump(&g, kid));
        assert_eq!(pos(&g, rf), (0.0, 0.0), "hidden at the origin");
        assert_eq!(g.w.get(rf).unwrap().alarms[0], r(50.0));
        fire(&mut g, rf, &EventKind::Alarm(0));
        assert_eq!(pos(&g, rf), (120.0, 90.0), "back at its start");
    }

    #[test]
    fn refresher_hide_time_is_a_param() {
        let mut g = game(Builtin::JumpRefresher, params(&[("hide_ticks", real(7.0))]));
        let kid = spawn(&mut g, "kid", 0.0, 0.0);
        let rf = spawn(&mut g, "thing", 50.0, 50.0);
        touch(&mut g, rf, kid);
        assert_eq!(g.w.get(rf).unwrap().alarms[0], r(7.0));
        let ok = params(&[("tjump", word("none"))]);
        assert!(builtins::init_state(Builtin::JumpRefresher, &ok).is_ok());
        let bad = params(&[("tjump", word("maybe"))]);
        assert!(builtins::init_state(Builtin::JumpRefresher, &bad).is_err());
    }

    #[test]
    fn refresher_sets_or_clears_tjump_and_none_leaves_it() {
        for (mode, start, want) in [("set", false, true), ("clear", true, false), ("none", true, true)] {
            let mut g = game(Builtin::JumpRefresher, params(&[("tjump", word(mode))]));
            let kid = spawn(&mut g, "kid", 100.0, 100.0);
            let rf = spawn(&mut g, "thing", 120.0, 90.0);
            with(&mut g, |c| api::set_triple_jump(c, kid, start));
            touch(&mut g, rf, kid);
            assert_eq!(read(&g, kid, "tjump") > 0.0, want, "tjump = {mode}");
            assert!(djump(&g, kid), "the air jump is refreshed either way");
        }
    }

    // ---- spring ------------------------------------------------------------------------------

    #[test]
    fn spring_launches_straight_up_and_can_refresh() {
        let mut g = game(Builtin::Spring, params(&[("djump", word("set"))]));
        let kid = spawn(&mut g, "kid", 100.0, 100.0);
        let sp = spawn(&mut g, "thing", 100.0, 100.0);
        {
            let m = &mut g.w.get_mut(kid).unwrap().m;
            m.set_hspeed(r(3.0)).unwrap();
            m.set_vspeed(r(5.0)).unwrap();
        }
        touch(&mut g, sp, kid);
        let m = &g.w.get(kid).unwrap().m;
        assert_eq!((m.hspeed().get(), m.vspeed().get()), (0.0, -10.0));
        assert!(djump(&g, kid));
    }

    #[test]
    fn spring_keeps_djump_by_default() {
        let mut g = game(Builtin::Spring, Params::default());
        let kid = spawn(&mut g, "kid", 0.0, 0.0);
        let sp = spawn(&mut g, "thing", 0.0, 0.0);
        touch(&mut g, sp, kid);
        assert!(!djump(&g, kid));
        with(&mut g, |c| api::set_air_jump(c, kid, true));
        touch(&mut g, sp, kid);
        assert!(djump(&g, kid), "keep leaves an existing djump alone");
    }

    #[test]
    fn spring_clear_takes_the_air_jump_away_and_still_launches() {
        let mut g = game(Builtin::Spring, params(&[("djump", word("clear"))]));
        let kid = spawn(&mut g, "kid", 0.0, 0.0);
        let sp = spawn(&mut g, "thing", 0.0, 0.0);
        with(&mut g, |c| api::set_air_jump(c, kid, true));
        touch(&mut g, sp, kid);
        assert!(!djump(&g, kid));
        assert_eq!(g.w.get(kid).unwrap().m.vspeed(), r(-10.0));
        let bad = params(&[("djump", word("toggle"))]);
        assert!(builtins::init_state(Builtin::Spring, &bad).is_err());
    }

    #[test]
    fn spring_speed_is_a_param() {
        let mut g = game(Builtin::Spring, params(&[("speed", real(12.0))]));
        let kid = spawn(&mut g, "kid", 0.0, 0.0);
        let sp = spawn(&mut g, "thing", 0.0, 0.0);
        touch(&mut g, sp, kid);
        assert_eq!(g.w.get(kid).unwrap().m.vspeed(), r(-12.0));
    }

    #[test]
    fn spring_is_destroyed_outside_the_room() {
        let mut g = game(Builtin::Spring, Params::default());
        let sp = spawn(&mut g, "thing", 0.0, 0.0);
        fire(&mut g, sp, &EventKind::OutsideRoom);
        assert!(!g.w.get(sp).is_some_and(|i| i.alive));
    }

    // ---- conveyor ----------------------------------------------------------------------------

    #[test]
    fn conveyor_adds_h_on_every_collision_tick() {
        let mut g = game(Builtin::Conveyor, params(&[("h", real(-2.0))]));
        let kid = spawn(&mut g, "kid", 100.0, 100.0);
        let cv = spawn(&mut g, "thing", 100.0, 108.0);
        g.w.get_mut(kid).unwrap().m.set_hspeed(r(1.0)).unwrap();
        touch(&mut g, cv, kid);
        touch(&mut g, cv, kid);
        assert_eq!(g.w.get(kid).unwrap().m.hspeed(), r(-3.0));
    }

    #[test]
    fn conveyor_top_edge_sets_the_speed_only_on_the_top_line() {
        let p = params(&[("h", real(4.0)), ("mode", word("set_on_top_edge"))]);
        let mut g = game(Builtin::Conveyor, p);
        let k = &g.def.kinds[usize::try_from(kind(&g, "thing").0).unwrap()];
        assert!(k.events.step && k.events.collisions.is_empty());
        // The conveyor top edge is y = 200; the kid (8 x 8) stands on it.
        let cv = spawn(&mut g, "thing", 100.0, 200.0);
        let on = spawn(&mut g, "kid", 110.0, 193.0);
        fire(&mut g, cv, &EventKind::Step);
        assert_eq!(g.w.get(on).unwrap().m.hspeed(), r(4.0));
        // A kid far above is untouched.
        g.w.get_mut(on).unwrap().m.set_hspeed(r(0.0)).unwrap();
        g.w.get_mut(on).unwrap().m.set_y(r(100.0)).unwrap();
        fire(&mut g, cv, &EventKind::Step);
        assert_eq!(g.w.get(on).unwrap().m.hspeed(), r(0.0));
    }

    #[test]
    fn conveyor_needs_h() {
        assert!(builtins::init_state(Builtin::Conveyor, &Params::default()).is_err());
    }

    // ---- kid_param_zone ----------------------------------------------------------------------

    #[test]
    fn zone_writes_the_kid_numbers_every_overlap_tick() {
        let p = params(&[
            ("run_speed", real(5.0)),
            ("jump", real(16.0)),
            ("jump2", real(13.0)),
        ]);
        let mut g = game(Builtin::KidParamZone, p);
        let kid = spawn(&mut g, "kid", 100.0, 100.0);
        let z = spawn(&mut g, "thing", 100.0, 100.0);
        touch(&mut g, z, kid);
        assert_eq!(read(&g, kid, "maxSpeed"), 5.0);
        assert_eq!(read(&g, kid, "jump"), 16.0);
        assert_eq!(read(&g, kid, "jump2"), 13.0);
    }

    #[test]
    fn zone_target_player_acts_on_the_first_kid_whoever_touches() {
        let p = params(&[("run_speed", real(5.0)), ("target", word("player"))]);
        let mut g = game(Builtin::KidParamZone, p);
        let first = spawn(&mut g, "kid", 0.0, 0.0);
        let second = spawn(&mut g, "kid", 300.0, 0.0);
        let z = spawn(&mut g, "thing", 300.0, 0.0);
        let before = read(&g, second, "maxSpeed");
        touch(&mut g, z, second);
        assert_eq!(read(&g, first, "maxSpeed"), 5.0);
        assert_eq!(read(&g, second, "maxSpeed"), before, "the toucher is not changed");
    }

    #[test]
    fn zone_needs_a_number_and_checks_values() {
        assert!(builtins::init_state(Builtin::KidParamZone, &Params::default()).is_err());
        let bad = params(&[("jump", word("high"))]);
        assert!(builtins::init_state(Builtin::KidParamZone, &bad).is_err());
    }
}
