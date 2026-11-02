//! WP6b.3: drop_platform, gravity_flip, edge_warp, bullet_blocker, slope (docs/plans/sim-core.md
//! 6.1). Hand-built worlds; inert kinds stand in for the kid.

mod wp6b {
    use iwbtg_sim::builtins::{self, BuiltinState, edge_warp::Edge};
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{
        Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, RoomId,
        RoomRole,
    };
    use iwbtg_sim::events::EventKind;
    use iwbtg_sim::num::Real;
    use iwbtg_sim::room::RoomRequest;
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::world::{Ctx, Flow, InstId, KindState, SpawnInit, World};

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

    fn word(s: &str) -> ParamValue {
        ParamValue::Str(s.to_owned())
    }

    struct G {
        def: GameDef,
        room: RoomId,
        kid: KindId,
        flipped: KindId,
        bullet: KindId,
    }

    /// A game with a kid, a flipped kid, a bullet and `extra` kinds (32 x 32 sprite).
    fn game(extra: Vec<KindDef>) -> G {
        let mut b = GameDefBuilder::new("wp6b-3");
        let spr32 = b.sprite_rect((32, 32), (0, 0));
        let spr4 = b.sprite_rect((4, 4), (0, 0));
        let mut k = KindDef::new("kid", Builtin::Inert);
        k.classes = ClassMask::KID;
        k.sprite = Some(spr4);
        let kid = b.kind(k);
        let mut f = KindDef::new("flipped", Builtin::Inert);
        f.classes = ClassMask::KID_FLIPPED;
        f.sprite = Some(spr4);
        let flipped = b.kind(f);
        let mut bu = KindDef::new("bullet", Builtin::Inert);
        bu.classes = ClassMask::KID_BULLET;
        bu.sprite = Some(spr4);
        let bullet = b.kind(bu);
        b.engine_kind(EngineKind::Kid, kid);
        for mut kd in extra {
            kd.sprite = Some(spr32);
            b.kind(kd);
        }
        let room = b.room(RoomDef::new("r", 800, 608));
        b.room(RoomDef::new("r2", 800, 608));
        let mut rs = RoomDef::new("sel", 800, 608);
        rs.role = RoomRole::Select;
        b.room(rs);
        let mut def = b.build().unwrap();
        for k in &mut def.kinds {
            k.events = builtins::event_set(k.base, &k.clone());
        }
        G {
            def,
            room,
            kid,
            flipped,
            bullet,
        }
    }

    fn kind(base: Builtin, name: &str, p: Params) -> KindDef {
        let mut k = KindDef::new(name, base);
        k.params = p;
        k
    }

    fn kind_id(g: &G, name: &str) -> KindId {
        KindId(u32::try_from(g.def.kinds.iter().position(|k| k.name == name).unwrap()).unwrap())
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

    fn touch(g: &G, w: &mut World, id: InstId, other: InstId, class: ClassMask) -> Flow {
        fire(g, w, id, &EventKind::Collision { with: class, other })
    }

    fn alive(w: &World, id: InstId) -> bool {
        w.get(id).is_some_and(|i| i.alive)
    }

    fn pos(w: &World, id: InstId) -> (f64, f64) {
        let m = &w.get(id).unwrap().m;
        (m.x().get(), m.y().get())
    }

    // ---- drop_platform ----------------------------------------------------------------------

    fn drop_game(mode: &str) -> G {
        game(vec![kind(
            Builtin::DropPlatform,
            "drop",
            params(&[("mode", word(mode))]),
        )])
    }

    #[test]
    fn fall_platform_starts_falling_when_stood_on() {
        let g = drop_game("fall");
        let mut w = World::new(g.room, 1);
        let p = spawn(&g, &mut w, kind_id(&g, "drop"), 100.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 300.0, 96.0);
        fire(&g, &mut w, p, &EventKind::Step);
        assert_eq!(w.get(p).unwrap().m.vspeed(), r(0.0));
        w.get_mut(kid).unwrap().m.set_x(r(110.0)).unwrap();
        fire(&g, &mut w, p, &EventKind::Step);
        assert_eq!(w.get(p).unwrap().m.vspeed(), r(2.0));
    }

    #[test]
    fn rise_platform_keeps_rising_after_the_kid_leaves() {
        let g = drop_game("rise");
        let mut w = World::new(g.room, 1);
        let p = spawn(&g, &mut w, kind_id(&g, "drop"), 100.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 110.0, 96.0);
        fire(&g, &mut w, p, &EventKind::Step);
        assert_eq!(pos(&w, kid).1, 94.0, "the kid is lifted by the speed");
        assert_eq!(pos(&w, p).1, 98.0);
        w.get_mut(kid).unwrap().m.set_x(r(300.0)).unwrap();
        fire(&g, &mut w, p, &EventKind::Step);
        assert_eq!(pos(&w, p).1, 96.0);
    }

    #[test]
    fn kae_rise_block_takes_the_kid_fall_speed() {
        let g = drop_game("rise_kae");
        let mut w = World::new(g.room, 1);
        let p = spawn(&g, &mut w, kind_id(&g, "drop"), 100.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 110.0, 98.0);
        w.get_mut(kid).unwrap().m.set_vspeed(r(3.0)).unwrap();
        fire(&g, &mut w, p, &EventKind::Step);
        fire(&g, &mut w, p, &EventKind::EndStep);
        assert_eq!(pos(&w, kid).1, 101.0);
        assert_eq!(pos(&w, p).1, 103.0);
    }

    #[test]
    fn kae_rise_block_ignores_a_rising_kid() {
        let g = drop_game("rise_kae");
        let mut w = World::new(g.room, 1);
        let p = spawn(&g, &mut w, kind_id(&g, "drop"), 100.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 110.0, 98.0);
        w.get_mut(kid).unwrap().m.set_vspeed(r(-3.0)).unwrap();
        fire(&g, &mut w, p, &EventKind::Step);
        fire(&g, &mut w, p, &EventKind::EndStep);
        assert_eq!(pos(&w, p).1, 100.0);
    }

    // ---- gravity_flip -----------------------------------------------------------------------

    fn flip_game(dir: &str) -> G {
        game(vec![kind(
            Builtin::GravityFlip,
            "flip",
            params(&[("direction", word(dir))]),
        )])
    }

    #[test]
    fn gravity_up_sets_the_flag_for_the_normal_kid_only() {
        let g = flip_game("up");
        let mut w = World::new(g.room, 1);
        let f = spawn(&g, &mut w, kind_id(&g, "flip"), 0.0, 0.0);
        let flipped = spawn(&g, &mut w, g.flipped, 0.0, 0.0);
        touch(&g, &mut w, f, flipped, ClassMask::KID_FLIPPED);
        assert!(!w.engine.kid.grav);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        touch(&g, &mut w, f, kid, ClassMask::KID);
        assert!(w.engine.kid.grav);
    }

    #[test]
    fn gravity_down_clears_the_flag_for_the_flipped_kid_only() {
        let g = flip_game("down");
        let mut w = World::new(g.room, 1);
        w.engine.kid.grav = true;
        let f = spawn(&g, &mut w, kind_id(&g, "flip"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        touch(&g, &mut w, f, kid, ClassMask::KID);
        assert!(w.engine.kid.grav);
        let flipped = spawn(&g, &mut w, g.flipped, 0.0, 0.0);
        touch(&g, &mut w, f, flipped, ClassMask::KID_FLIPPED);
        assert!(!w.engine.kid.grav);
    }

    #[test]
    fn gravity_flip_is_destroyed_outside_the_room_and_refuses_yflag() {
        let g = flip_game("up");
        let mut w = World::new(g.room, 1);
        let f = spawn(&g, &mut w, kind_id(&g, "flip"), 0.0, 0.0);
        fire(&g, &mut w, f, &EventKind::OutsideRoom);
        assert!(!alive(&w, f));
        let bad = params(&[
            ("direction", word("up")),
            ("transition", word("yflag_with_view_rotation")),
        ]);
        assert!(builtins::init_state(Builtin::GravityFlip, &bad).is_err());
        assert!(builtins::init_state(Builtin::GravityFlip, &Params::default()).is_err());
    }

    // ---- bullet_blocker ---------------------------------------------------------------------

    #[test]
    fn bullet_blocker_destroys_bullets() {
        let g = game(vec![kind(Builtin::BulletBlocker, "bb", Params::default())]);
        let mut w = World::new(g.room, 1);
        let b = spawn(&g, &mut w, kind_id(&g, "bb"), 100.0, 100.0);
        let bullet = spawn(&g, &mut w, g.bullet, 100.0, 100.0);
        touch(&g, &mut w, b, bullet, ClassMask::KID_BULLET);
        assert!(!alive(&w, bullet));
    }

    #[test]
    fn bullet_blocker_can_spare_bullets_while_it_overlaps_the_kid() {
        let p = params(&[("except_when_overlapping_kid", ParamValue::Bool(true))]);
        let g = game(vec![kind(Builtin::BulletBlocker, "bb", p)]);
        let mut w = World::new(g.room, 1);
        let b = spawn(&g, &mut w, kind_id(&g, "bb"), 100.0, 100.0);
        let bullet = spawn(&g, &mut w, g.bullet, 100.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 110.0, 110.0);
        touch(&g, &mut w, b, bullet, ClassMask::KID_BULLET);
        assert!(alive(&w, bullet));
        w.get_mut(kid).unwrap().m.set_x(r(500.0)).unwrap();
        touch(&g, &mut w, b, bullet, ClassMask::KID_BULLET);
        assert!(!alive(&w, bullet));
    }

    // ---- slope ------------------------------------------------------------------------------

    #[test]
    fn slope_checks_its_params() {
        let ok = params(&[
            ("shape", word("rise_left")),
            ("scale_x", ParamValue::Real(r(-1.0))),
        ]);
        assert!(builtins::init_state(Builtin::Slope, &ok).is_ok());
        assert!(builtins::init_state(Builtin::Slope, &params(&[("shape", word("wavy"))])).is_err());
    }

    // ---- edge_warp --------------------------------------------------------------------------

    fn room(n: u32) -> ParamValue {
        ParamValue::Room(RoomId(n))
    }

    #[test]
    fn single_target_edge_warp_keeps_a_warp_state_for_the_kid() {
        let p = params(&[
            ("room_to", room(1)),
            ("warp_x", ParamValue::Real(r(32.0))),
            ("warp_y", ParamValue::Real(r(64.0))),
        ]);
        match builtins::init_state(Builtin::EdgeWarp, &p).unwrap() {
            BuiltinState::Warp(s) => {
                assert_eq!((s.room_to, s.warp_x, s.warp_y), (RoomId(1), r(32.0), r(64.0)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn per_edge_table_answers_by_edge() {
        let p = params(&[("left_room", room(1)), ("down_room", room(2))]);
        match builtins::init_state(Builtin::EdgeWarp, &p).unwrap() {
            BuiltinState::EdgeTable(t) => {
                assert_eq!(t.target(Edge::Left), Some(RoomId(1)));
                assert_eq!(t.target(Edge::Down), Some(RoomId(2)));
                assert_eq!(t.target(Edge::Up), None);
                assert_eq!(t.target(Edge::Right), None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn edge_warp_needs_exactly_one_form() {
        assert!(builtins::init_state(Builtin::EdgeWarp, &Params::default()).is_err());
        let both = params(&[("room_to", room(1)), ("up_room", room(2))]);
        assert!(builtins::init_state(Builtin::EdgeWarp, &both).is_err());
    }

    #[test]
    fn room_assign_queues_the_room_and_moves_the_kid() {
        let p = params(&[
            ("mode", word("room_assign")),
            ("room_to", room(1)),
            ("warp_x", ParamValue::Real(r(8.0))),
        ]);
        let g = game(vec![kind(Builtin::EdgeWarp, "sw", p)]);
        let mut w = World::new(g.room, 1);
        let s = spawn(&g, &mut w, kind_id(&g, "sw"), 800.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 797.0, 503.4);
        let flow = touch(&g, &mut w, s, kid, ClassMask::KID);
        assert_eq!(flow, Flow::AbortTick);
        assert_eq!(w.pending, Some(RoomRequest::Goto(RoomId(1))));
        assert_eq!(pos(&w, kid), (8.0, 503.4), "y is kept when warp_y is 0");
        assert!(matches!(
            w.get(s).map(|i| &i.state),
            Some(KindState::Builtin(BuiltinState::RoomAssign(_)))
        ));
    }

    #[test]
    fn room_assign_without_a_target_does_nothing() {
        let g = game(vec![kind(
            Builtin::EdgeWarp,
            "sw",
            params(&[("mode", word("room_assign"))]),
        )]);
        let mut w = World::new(g.room, 1);
        let s = spawn(&g, &mut w, kind_id(&g, "sw"), 800.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 797.0, 503.0);
        assert_eq!(touch(&g, &mut w, s, kid, ClassMask::KID), Flow::Continue);
        assert_eq!(w.pending, None);
    }
}
