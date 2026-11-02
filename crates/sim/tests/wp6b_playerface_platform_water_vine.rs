//! WP6b.1: playerface, moving_platform, water, vine (docs/plans/sim-core.md 6.1). Hand-built
//! worlds; inert kinds stand in for the kid.

mod wp6b {
    use iwbtg_sim::builtins;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{
        Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, RoomId,
        RoomRole,
    };
    use iwbtg_sim::events::EventKind;
    use iwbtg_sim::num::Real;
    use iwbtg_sim::rng::Rng;
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

    fn yes() -> ParamValue {
        ParamValue::Bool(true)
    }

    fn no() -> ParamValue {
        ParamValue::Bool(false)
    }

    fn word(s: &str) -> ParamValue {
        ParamValue::Str(s.to_owned())
    }

    struct G {
        def: GameDef,
        room: RoomId,
        kid: KindId,
        flipped: KindId,
        block: KindId,
    }

    /// A game with two kid stand-ins, a block and `extra` kinds (each gets a 32 x 32 sprite).
    fn game(extra: Vec<KindDef>) -> G {
        let mut b = GameDefBuilder::new("wp6b-1");
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
        let mut blk = KindDef::new("block", Builtin::Block);
        blk.solid = true;
        blk.classes = ClassMask::BLOCK;
        blk.sprite = Some(spr32);
        let block = b.kind(blk);
        b.engine_kind(EngineKind::Kid, kid);
        for mut kd in extra {
            kd.sprite = Some(spr32);
            b.kind(kd);
        }
        let room = b.room(RoomDef::new("r", 800, 608));
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
            block,
        }
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

    fn spawn_at(g: &G, w: &mut World, kind: KindId, x: f64, y: f64, p: Params) -> InstId {
        with(g, w, |c| {
            let init = SpawnInit {
                params: p,
                ..SpawnInit::default()
            };
            c.spawn(kind, r(x), r(y), init).unwrap()
        })
    }

    fn spawn(g: &G, w: &mut World, kind: KindId, x: f64, y: f64) -> InstId {
        spawn_at(g, w, kind, x, y, Params::default())
    }

    fn fire(g: &G, w: &mut World, id: InstId, ev: &EventKind) -> Flow {
        with(g, w, |c| builtins::on_event(c, id, ev).unwrap())
    }

    fn touch(g: &G, w: &mut World, id: InstId, other: InstId, class: ClassMask) {
        fire(g, w, id, &EventKind::Collision { with: class, other });
    }

    fn set_v(w: &mut World, id: InstId, v: f64) {
        w.get_mut(id).unwrap().m.set_vspeed(r(v)).unwrap();
    }

    fn vspeed(w: &World, id: InstId) -> Real {
        w.get(id).unwrap().m.vspeed()
    }

    fn pos(w: &World, id: InstId) -> (f64, f64) {
        let m = &w.get(id).unwrap().m;
        (m.x().get(), m.y().get())
    }

    // ---- playerface -------------------------------------------------------------------------

    #[test]
    fn playerface_create_draws_one_frame() {
        let g = game(vec![KindDef::new("face", Builtin::Playerface)]);
        let mut w = World::new(g.room, 11);
        let mut mirror = Rng::new(11);
        let frame = mirror.choose(3).unwrap();
        let id = spawn(&g, &mut w, kind_id(&g, "face"), 0.0, 0.0);
        assert_eq!(w.get(id).unwrap().img.index(), r(f64::from(frame)));
        assert_eq!(w.rng, mirror, "exactly one draw");
    }

    // ---- vine -------------------------------------------------------------------------------

    #[test]
    fn vine_needs_a_valid_side() {
        assert!(builtins::init_state(Builtin::Vine, &Params::default()).is_err());
        assert!(builtins::init_state(Builtin::Vine, &params(&[("side", word("left"))])).is_ok());
        assert!(builtins::init_state(Builtin::Vine, &params(&[("side", word("up"))])).is_err());
    }

    // ---- water ------------------------------------------------------------------------------

    fn water_game(p: Params) -> G {
        let mut k = KindDef::new("water", Builtin::Water);
        k.params = p;
        game(vec![k])
    }

    #[test]
    fn water_caps_the_fall_at_two_by_default() {
        let g = water_game(Params::default());
        let mut w = World::new(g.room, 1);
        let water = spawn(&g, &mut w, kind_id(&g, "water"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        set_v(&mut w, kid, 5.0);
        touch(&g, &mut w, water, kid, ClassMask::KID);
        assert_eq!(vspeed(&w, kid), r(2.0));
        set_v(&mut w, kid, 1.0);
        touch(&g, &mut w, water, kid, ClassMask::KID);
        assert_eq!(vspeed(&w, kid), r(1.0), "slower falls are kept");
        set_v(&mut w, kid, -5.0);
        touch(&g, &mut w, water, kid, ClassMask::KID);
        assert_eq!(vspeed(&w, kid), r(-5.0), "rising is kept");
    }

    #[test]
    fn water_caps_the_flipped_kid_upward() {
        let g = water_game(params(&[("fall_cap", real(2.0))]));
        let mut w = World::new(g.room, 1);
        let water = spawn(&g, &mut w, kind_id(&g, "water"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.flipped, 0.0, 0.0);
        set_v(&mut w, kid, -5.0);
        touch(&g, &mut w, water, kid, ClassMask::KID_FLIPPED);
        assert_eq!(vspeed(&w, kid), r(-2.0));
        set_v(&mut w, kid, 5.0);
        touch(&g, &mut w, water, kid, ClassMask::KID_FLIPPED);
        assert_eq!(vspeed(&w, kid), r(5.0));
    }

    #[test]
    fn water_without_a_cap_leaves_the_speed() {
        let g = water_game(params(&[("fall_cap", real(-1.0))]));
        let mut w = World::new(g.room, 1);
        let water = spawn(&g, &mut w, kind_id(&g, "water"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        set_v(&mut w, kid, 9.0);
        touch(&g, &mut w, water, kid, ClassMask::KID);
        assert_eq!(vspeed(&w, kid), r(9.0));
    }

    fn area_water() -> G {
        water_game(params(&[
            ("area_left", real(0.0)),
            ("area_right", real(100.0)),
            ("area_top", real(50.0)),
            ("area_bottom", real(200.0)),
        ]))
    }

    #[test]
    fn water_with_an_area_acts_in_step_on_kids_strictly_inside() {
        let g = area_water();
        let k = &g.def.kinds[usize::try_from(kind_id(&g, "water").0).unwrap()];
        assert!(
            k.events.step && k.events.collisions.is_empty(),
            "no collision event"
        );
        let mut w = World::new(g.room, 1);
        let water = spawn(&g, &mut w, kind_id(&g, "water"), 500.0, 500.0);
        let inside = spawn(&g, &mut w, g.kid, 10.0, 60.0);
        let edge = spawn(&g, &mut w, g.kid, 0.0, 60.0);
        let below = spawn(&g, &mut w, g.kid, 10.0, 200.0);
        for id in [inside, edge, below] {
            set_v(&mut w, id, 6.0);
        }
        fire(&g, &mut w, water, &EventKind::Step);
        assert_eq!(vspeed(&w, inside), r(2.0));
        assert_eq!(vspeed(&w, edge), r(6.0), "x = left is not strictly inside");
        assert_eq!(vspeed(&w, below), r(6.0), "y = bottom is not strictly inside");
    }

    #[test]
    fn water_with_an_area_ignores_collision_events() {
        let g = area_water();
        let mut w = World::new(g.room, 1);
        let water = spawn(&g, &mut w, kind_id(&g, "water"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 10.0, 60.0);
        set_v(&mut w, kid, 6.0);
        touch(&g, &mut w, water, kid, ClassMask::KID);
        assert_eq!(vspeed(&w, kid), r(6.0));
    }

    #[test]
    fn water_area_needs_all_four_edges() {
        let p = params(&[("area_left", real(0.0)), ("area_right", real(1.0))]);
        assert!(builtins::init_state(Builtin::Water, &p).is_err());
    }

    #[test]
    fn water_djump_ignoring_flag_is_refused_until_the_kid_can_set_djump() {
        let p = params(&[("air_jump", word("djump_ignoring_flag"))]);
        assert!(builtins::init_state(Builtin::Water, &p).is_err());
    }

    // ---- moving_platform --------------------------------------------------------------------

    fn plat_game(p: Params) -> G {
        let mut k = KindDef::new("plat", Builtin::MovingPlatform);
        k.params = p;
        game(vec![k])
    }

    /// One platform Step, then the runner automatic motion.
    fn step(g: &G, w: &mut World, id: InstId) {
        fire(g, w, id, &EventKind::Step);
        let i = w.get_mut(id).unwrap();
        let (h, v) = (i.m.hspeed(), i.m.vspeed());
        let (x, y) = (i.m.x(), i.m.y());
        i.m.set_x(x + h).unwrap();
        i.m.set_y(y + v).unwrap();
    }

    #[test]
    fn platform_moves_at_its_start_speed() {
        let g = plat_game(params(&[("hspeed", real(2.0))]));
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        step(&g, &mut w, id);
        step(&g, &mut w, id);
        assert_eq!(pos(&w, id), (104.0, 100.0));
    }

    #[test]
    fn platform_reverses_at_a_solid() {
        let g = plat_game(params(&[("hspeed", real(2.0))]));
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.block, 133.0, 100.0);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        step(&g, &mut w, id);
        assert_eq!(w.get(id).unwrap().m.hspeed(), r(-2.0));
    }

    #[test]
    fn platform_without_bounce_drives_into_the_solid() {
        let g = plat_game(params(&[("hspeed", real(2.0)), ("bounce", no())]));
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.block, 133.0, 100.0);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        step(&g, &mut w, id);
        assert_eq!(w.get(id).unwrap().m.hspeed(), r(2.0));
    }

    #[test]
    fn platform_carries_a_kid_standing_on_it() {
        let g = plat_game(params(&[("hspeed", real(2.0))]));
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 110.0, 97.0);
        step(&g, &mut w, id);
        assert_eq!(pos(&w, kid), (112.0, 97.0));
    }

    #[test]
    fn platform_ignores_a_kid_far_above() {
        let g = plat_game(params(&[("hspeed", real(2.0))]));
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 110.0, 60.0);
        step(&g, &mut w, id);
        assert_eq!(pos(&w, kid), (110.0, 60.0));
    }

    #[test]
    fn platform_upward_motion_goes_through_yspeed() {
        let g = plat_game(params(&[("vspeed", real(-2.0))]));
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        step(&g, &mut w, id);
        // Step 5 moved vspeed into yspeed, so the automatic motion adds nothing yet.
        assert_eq!(vspeed(&w, id), r(0.0));
        assert_eq!(pos(&w, id), (100.0, 100.0));
        step(&g, &mut w, id);
        assert_eq!(pos(&w, id), (100.0, 98.0));
    }

    #[test]
    fn platform_waits_for_the_first_touch() {
        let g = plat_game(params(&[("hspeed", real(3.0)), ("start_on_touch", yes())]));
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        step(&g, &mut w, id);
        assert_eq!(pos(&w, id), (100.0, 100.0));
        let kid = spawn(&g, &mut w, g.kid, 110.0, 97.0);
        touch(&g, &mut w, id, kid, ClassMask::KID);
        step(&g, &mut w, id);
        assert_eq!(pos(&w, id), (103.0, 100.0));
    }

    #[test]
    fn platform_marker_reverses_per_rule() {
        for (rule, want_h, want_v) in [("both", -2.0, 1.0), ("vertical", 2.0, 1.0), ("none", 2.0, -1.0)] {
            let g = plat_game(params(&[("hspeed", real(2.0)), ("marker_reverse", word(rule))]));
            let mut w = World::new(g.room, 1);
            let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
            set_v(&mut w, id, -1.0);
            with(&g, &mut w, |c| {
                builtins::moving_platform::reverse_on_marker(c, id).unwrap()
            });
            let m = &w.get(id).unwrap().m;
            assert_eq!((m.hspeed().get(), m.vspeed().get()), (want_h, want_v), "{rule}");
        }
    }

    fn b_game(p: &[(&str, ParamValue)]) -> G {
        let mut items = vec![("carry_rule", word("b_lines"))];
        items.extend_from_slice(p);
        let mut k = KindDef::new("plat", Builtin::MovingPlatform);
        k.params = params(&items);
        let mut tall = KindDef::new("tall", Builtin::Inert);
        tall.classes = ClassMask::KID;
        game(vec![k, tall])
    }

    /// One End Step, then the runner automatic motion (b_lines runs in End Step, not Step).
    fn end_step(g: &G, w: &mut World, id: InstId) {
        fire(g, w, id, &EventKind::EndStep);
        let i = w.get_mut(id).unwrap();
        let (h, v) = (i.m.hspeed(), i.m.vspeed());
        let (x, y) = (i.m.x(), i.m.y());
        i.m.set_x(x + h).unwrap();
        i.m.set_y(y + v).unwrap();
    }

    #[test]
    fn platform_b_lines_runs_in_end_step_only() {
        let g = b_game(&[("hspeed", real(2.0))]);
        let k = &g.def.kinds[usize::try_from(kind_id(&g, "plat").0).unwrap()];
        assert!(k.events.end_step && !k.events.step);
        let y = plat_game(Params::default());
        let k = &y.def.kinds[usize::try_from(kind_id(&y, "plat").0).unwrap()];
        assert!(k.events.step && !k.events.end_step);
    }

    #[test]
    fn platform_b_lines_carries_a_kid_touching_both_lines() {
        let g = b_game(&[("hspeed", real(2.0))]);
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        let kid = spawn(&g, &mut w, kind_id(&g, "tall"), 110.0, 70.0);
        end_step(&g, &mut w, id);
        assert_eq!(pos(&w, kid), (112.0, 70.0));
        assert_eq!(pos(&w, id), (102.0, 100.0), "the platform moved by its own speed");
    }

    #[test]
    fn platform_b_lines_needs_both_lines_to_touch() {
        let g = b_game(&[("hspeed", real(2.0))]);
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        let high = spawn(&g, &mut w, kind_id(&g, "tall"), 110.0, 40.0);
        end_step(&g, &mut w, id);
        assert_eq!(pos(&w, high), (110.0, 40.0), "touches neither line");
        let low = spawn(&g, &mut w, kind_id(&g, "tall"), 110.0, 95.0);
        w.get_mut(high).unwrap().alive = false;
        end_step(&g, &mut w, id);
        assert_eq!(pos(&w, low), (110.0, 95.0), "only the lower line");
    }

    #[test]
    fn platform_b_lines_moves_x_and_y_only_where_free() {
        // A block just beside the kid stops the x move but not the y carry.
        let g = b_game(&[("hspeed", real(2.0)), ("vspeed", real(3.0)), ("bounce", no())]);
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.block, 143.0, 70.0);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        let kid = spawn(&g, &mut w, kind_id(&g, "tall"), 110.0, 70.0);
        end_step(&g, &mut w, id);
        assert_eq!(pos(&w, kid), (110.0, 73.0), "x blocked, y carried by vspeed 3");
    }

    #[test]
    fn platform_b_lines_with_no_kid_just_moves() {
        let g = b_game(&[("hspeed", real(2.0))]);
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        end_step(&g, &mut w, id);
        assert_eq!(pos(&w, id), (102.0, 100.0));
    }
}
