//! WP6a.4: tier 1 rest: camera, killer, platform (docs/plans/sim-core.md 6.1, 12). Hand-built
//! worlds; an inert kind stands in for the kid.

mod wp6a {
    use iwbtg_sim::builtins;
    use iwbtg_sim::collide::Target;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{
        Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, RoomId,
    };
    use iwbtg_sim::events::{EventKind, EventSet};
    use iwbtg_sim::num::Real;
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::world::{Ctx, Flow, InstId, SpawnInit, World};

    fn r(v: f64) -> Real {
        Real::new(v)
    }

    fn params(items: &[(&str, ParamValue)]) -> Params {
        let mut p = Params::default();
        for (k, v) in items {
            p.0.insert((*k).to_owned(), v.clone());
        }
        p
    }

    fn real(v: f64) -> ParamValue {
        ParamValue::Real(r(v))
    }

    fn word(s: &str) -> ParamValue {
        ParamValue::Str(s.to_owned())
    }

    struct G {
        def: GameDef,
        room: RoomId,
        kid: KindId,
        flipped: KindId,
        thing: KindId,
    }

    /// A game with both kid kinds and one kind `thing` of base `base` taking `p`. The room is
    /// 3200 x 1216.
    fn game(base: Builtin, p: Params) -> G {
        let mut b = GameDefBuilder::new("wp6a-d");
        let mut k = KindDef::new("kid", Builtin::Inert);
        k.classes = ClassMask::KID;
        let kid = b.kind(k);
        let mut kf = KindDef::new("kid2", Builtin::Inert);
        kf.classes = ClassMask::KID_FLIPPED;
        let flipped = b.kind(kf);
        let mut t = KindDef::new("thing", base);
        t.params = p;
        let thing = b.kind(t);
        b.engine_kind(EngineKind::Kid, kid);
        b.engine_kind(EngineKind::KidFlipped, flipped);
        let room = b.room(RoomDef::new("r", 3200, 1216));
        let mut def = b.build().unwrap();
        for k in &mut def.kinds {
            k.events = builtins::event_set(k.base, &k.clone());
        }
        G {
            def,
            room,
            kid,
            flipped,
            thing,
        }
    }

    fn with<T>(g: &G, w: &mut World, f: impl FnOnce(&mut Ctx) -> T) -> T {
        let rules = RunnerRules::default();
        let scripts = ScriptRuntime::default();
        let mut ctx = Ctx {
            def: &g.def,
            rules: &rules,
            w,
            scripts: &scripts,
            dll_mode: DllMode::Stub,
        };
        f(&mut ctx)
    }

    fn spawn(g: &G, w: &mut World, kind: KindId, x: f64, y: f64) -> InstId {
        with(g, w, |c| c.spawn(kind, r(x), r(y), SpawnInit::default()).unwrap())
    }

    fn fire(g: &G, w: &mut World, id: InstId, ev: &EventKind) -> Flow {
        with(g, w, |c| builtins::on_event(c, id, ev).unwrap())
    }

    fn view(w: &World) -> (f64, f64) {
        (w.views[0].x.get(), w.views[0].y.get())
    }

    fn kind_events(g: &G) -> EventSet {
        g.def.kinds[usize::try_from(g.thing.0).unwrap()].events.clone()
    }

    // ---- camera: screen_snap ----------------------------------------------------------------

    #[test]
    fn screen_snap_puts_view_zero_on_the_screen_holding_the_kid() {
        let g = game(Builtin::Camera, Params::default());
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.kid, 1650.0, 700.0);
        let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
        fire(&g, &mut w, cam, &EventKind::Step);
        assert_eq!(
            view(&w),
            (1600.0, 608.0),
            "floor(1650/800)*800, floor(700/608)*608"
        );
        w.get_mut(w.kid(&RunnerRules::default().tick).unwrap())
            .unwrap()
            .m
            .set_x(r(799.0))
            .unwrap();
        fire(&g, &mut w, cam, &EventKind::Step);
        assert_eq!(view(&w).0, 0.0);
    }

    #[test]
    fn screen_snap_y_offset_screen_size_and_events() {
        // machinary: +4, poronkusema: 960 x 540.
        let g = game(
            Builtin::Camera,
            params(&[
                ("y_offset", real(4.0)),
                ("screen_w", real(960.0)),
                ("screen_h", real(540.0)),
            ]),
        );
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.kid, 1000.0, 600.0);
        let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
        fire(&g, &mut w, cam, &EventKind::Step);
        assert_eq!(view(&w), (960.0, 544.0));
        // Room Start and Room End only snap when the game says so.
        let g = game(Builtin::Camera, Params::default());
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.kid, 900.0, 10.0);
        let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
        fire(&g, &mut w, cam, &EventKind::RoomStart);
        assert_eq!(view(&w), (0.0, 0.0));
        let e = kind_events(&g);
        assert!(e.step && !e.room_start && !e.room_end);
        let g = game(
            Builtin::Camera,
            params(&[
                ("on_room_start", ParamValue::Bool(true)),
                ("on_room_end", ParamValue::Bool(true)),
            ]),
        );
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.kid, 900.0, 10.0);
        let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
        fire(&g, &mut w, cam, &EventKind::RoomStart);
        assert_eq!(view(&w), (800.0, 0.0));
        w.views[0].x = r(0.0);
        fire(&g, &mut w, cam, &EventKind::RoomEnd);
        assert_eq!(view(&w), (800.0, 0.0));
        let e = kind_events(&g);
        assert!(e.step && e.room_start && e.room_end);
    }

    #[test]
    fn screen_snap_follows_the_flipped_kid_and_waits_for_a_kid() {
        let g = game(Builtin::Camera, Params::default());
        let mut w = World::new(g.room, 1);
        let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
        w.views[0].x = r(1600.0);
        fire(&g, &mut w, cam, &EventKind::Step);
        assert_eq!(view(&w).0, 1600.0, "no kid: the view stays");
        spawn(&g, &mut w, g.flipped, 850.0, 20.0);
        fire(&g, &mut w, cam, &EventKind::Step);
        assert_eq!(view(&w).0, 800.0);
    }

    // ---- camera: end_step_clamp -------------------------------------------------------------

    #[test]
    fn end_step_clamp_uses_the_views_size_and_clamps_the_point() {
        let g = game(Builtin::Camera, params(&[("mode", word("end_step_clamp"))]));
        let mut w = World::new(g.room, 1);
        w.views[0].w = r(800.0);
        w.views[0].h = r(608.0);
        let kid = spawn(&g, &mut w, g.kid, 900.0, 700.0);
        let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
        assert_eq!(view(&w), (800.0, 608.0), "Create snaps too");
        // Past the room's edge: clamped to 3199, 1215 before the split.
        w.get_mut(kid).unwrap().m.set_x(r(4000.0)).unwrap();
        w.get_mut(kid).unwrap().m.set_y(r(-50.0)).unwrap();
        fire(&g, &mut w, cam, &EventKind::EndStep);
        assert_eq!(view(&w), (2400.0, 0.0), "3199 / 800 floors to 3");
        let e = kind_events(&g);
        assert!(e.end_step && !e.step);
    }

    // ---- camera: smooth_follow --------------------------------------------------------------

    #[test]
    fn smooth_follow_create_becomes_the_followed_object_and_jumps_to_the_kid() {
        let g = game(Builtin::Camera, params(&[("mode", word("smooth_follow"))]));
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.kid, 500.0, 300.0);
        let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
        assert_eq!(w.views[0].follow, Some(Target::Inst(cam)));
        assert_eq!((w.views[0].hborder, w.views[0].vborder), (r(400.0), r(304.0)));
        let i = w.get(cam).unwrap();
        assert_eq!((i.m.x(), i.m.y()), (r(500.0), r(300.0)));
    }

    #[test]
    fn smooth_follow_speeds_are_a_fraction_of_the_distance() {
        let g = game(Builtin::Camera, params(&[("mode", word("smooth_follow"))]));
        let mut w = World::new(g.room, 1);
        let kid = spawn(&g, &mut w, g.kid, 500.0, 300.0);
        let cam = spawn(&g, &mut w, g.thing, 0.0, 0.0);
        w.get_mut(kid).unwrap().m.set_x(r(596.0)).unwrap();
        w.get_mut(kid).unwrap().m.set_y(r(336.0)).unwrap();
        fire(&g, &mut w, cam, &EventKind::Step);
        let i = w.get(cam).unwrap();
        assert_eq!(
            (i.m.hspeed(), i.m.vspeed()),
            (r(6.0), r(3.0)),
            "96 / 16 and 36 / 12"
        );
        // Within 2 px (inclusive) an axis stops; the other keeps going.
        w.get_mut(kid).unwrap().m.set_x(r(502.0)).unwrap();
        fire(&g, &mut w, cam, &EventKind::Step);
        let i = w.get(cam).unwrap();
        assert_eq!((i.m.hspeed(), i.m.vspeed()), (r(0.0), r(3.0)));
        w.get_mut(kid).unwrap().m.set_x(r(502.5)).unwrap();
        fire(&g, &mut w, cam, &EventKind::Step);
        assert!(w.get(cam).unwrap().m.hspeed().get() > 0.0, "2.5 px is outside");
        // No kid: both speeds are 0.
        w.get_mut(kid).unwrap().alive = false;
        fire(&g, &mut w, cam, &EventKind::Step);
        let i = w.get(cam).unwrap();
        assert_eq!((i.m.hspeed(), i.m.vspeed()), (r(0.0), r(0.0)));
    }

    #[test]
    fn camera_params_are_checked() {
        assert!(builtins::init_state(Builtin::Camera, &params(&[("mode", word("zoom"))])).is_err());
        assert!(builtins::init_state(Builtin::Camera, &params(&[("screen_w", real(0.0))])).is_err());
        assert!(builtins::init_state(Builtin::Camera, &params(&[("h_div", real(0.0))])).is_err());
    }

    // ---- killer -----------------------------------------------------------------------------

    #[test]
    fn killer_create_applies_the_cherry_settings() {
        let g = game(
            Builtin::Killer,
            params(&[
                ("image_speed", real(1.0 / 15.0)),
                ("scale", real(9.0)),
                ("vspeed", real(8.0)),
                ("visible", ParamValue::Bool(false)),
            ]),
        );
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, g.thing, 100.0, 100.0);
        let i = w.get(id).unwrap();
        assert_eq!(i.img.speed(), r(1.0 / 15.0));
        assert_eq!((i.img.xscale(), i.img.yscale()), (r(9.0), r(9.0)));
        assert_eq!((i.m.hspeed(), i.m.vspeed()), (r(0.0), r(8.0)));
        assert!(!i.visible);
        // A plain killer changes nothing.
        let g = game(Builtin::Killer, Params::default());
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, g.thing, 5.0, 6.0);
        let i = w.get(id).unwrap();
        assert_eq!(
            (i.img.speed(), i.img.xscale(), i.m.speed()),
            (r(1.0), r(1.0), r(0.0))
        );
    }

    #[test]
    fn killer_outside_room_destroys_only_when_asked() {
        let g = game(
            Builtin::Killer,
            params(&[("destroy_outside_room", ParamValue::Bool(true))]),
        );
        assert!(kind_events(&g).outside_room);
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, g.thing, 0.0, 0.0);
        fire(&g, &mut w, id, &EventKind::OutsideRoom);
        assert!(!w.get(id).unwrap().alive);
        let g = game(Builtin::Killer, Params::default());
        assert!(!kind_events(&g).outside_room);
    }

    // ---- platform ---------------------------------------------------------------------------

    #[test]
    fn platform_has_no_events_and_checks_its_params() {
        let g = game(
            Builtin::Platform,
            params(&[("width", real(64.0)), ("parent_kind", word("solidblock"))]),
        );
        let e = kind_events(&g);
        assert_eq!(e, EventSet::default());
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, g.thing, 0.0, 0.0);
        assert_eq!(fire(&g, &mut w, id, &EventKind::Step), Flow::Continue);
        assert!(w.get(id).unwrap().alive);
        assert!(builtins::init_state(Builtin::Platform, &params(&[("width", real(0.0))])).is_err());
        assert!(builtins::init_state(Builtin::Platform, &params(&[("parent_kind", word("wall"))])).is_err());
    }
}
