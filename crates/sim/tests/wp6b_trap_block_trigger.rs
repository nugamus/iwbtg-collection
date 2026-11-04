//! WP6b.2: fake_block, hidden_block, trigger, trap (docs/plans/sim-core.md 6.1). Hand-built
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

    fn word(s: &str) -> ParamValue {
        ParamValue::Str(s.to_owned())
    }

    struct G {
        def: GameDef,
        room: RoomId,
        kid: KindId,
    }

    /// A game with a kid stand-in, a kid bullet stand-in, a trap stopper stand-in and `extra`
    /// kinds (each gets a 32 x 32 sprite). Kind ids: kid 0, bullet 1, stopper 2, extras from 3.
    fn game(extra: Vec<KindDef>) -> G {
        let mut b = GameDefBuilder::new("wp6b-2");
        let spr32 = b.sprite_rect((32, 32), (0, 0));
        let spr4 = b.sprite_rect((4, 4), (0, 0));
        let mut k = KindDef::new("kid", Builtin::Inert);
        k.classes = ClassMask::KID;
        k.sprite = Some(spr4);
        let kid = b.kind(k);
        let mut bu = KindDef::new("bullet", Builtin::Inert);
        bu.classes = ClassMask::KID_BULLET;
        bu.sprite = Some(spr4);
        b.kind(bu);
        let mut st = KindDef::new("stopper", Builtin::Inert);
        st.classes = ClassMask::TRAP_STOPPER;
        st.sprite = Some(spr32);
        b.kind(st);
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
        G { def, room, kid }
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

    fn touch(g: &G, w: &mut World, id: InstId, other: InstId, class: ClassMask) {
        fire(g, w, id, &EventKind::Collision { with: class, other });
    }

    fn alive(w: &World, id: InstId) -> bool {
        w.get(id).is_some_and(|i| i.alive)
    }

    fn speeds(w: &World, id: InstId) -> (f64, f64) {
        let m = &w.get(id).unwrap().m;
        (m.hspeed().get(), m.vspeed().get())
    }

    // ---- trigger ----------------------------------------------------------------------------

    fn trig_game(model: &str, extra: &[(&str, ParamValue)]) -> G {
        let mut items = vec![("model", word(model)), ("trg", real(3.0))];
        items.extend(extra.iter().cloned());
        game(vec![kind(Builtin::Trigger, "trig", params(&items))])
    }

    #[test]
    fn shared_last_writes_every_instance_of_the_kind() {
        let g = trig_game("shared_last", &[]);
        let mut w = World::new(g.room, 1);
        let t = kind_id(&g, "trig");
        let a = spawn(&g, &mut w, t, 0.0, 0.0);
        let b = spawn(&g, &mut w, t, 100.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        assert!(!with(&g, &mut w, |c| builtins::trigger::fired(
            c,
            builtins::trigger::Model::SharedLast,
            r(3.0),
            &[t]
        )
        .unwrap()));
        // The second instance is touched; the trap reads the first one.
        touch(&g, &mut w, b, kid, ClassMask::KID);
        assert!(with(&g, &mut w, |c| builtins::trigger::fired(
            c,
            builtins::trigger::Model::SharedLast,
            r(3.0),
            &[t]
        )
        .unwrap()));
        let _ = a;
    }

    #[test]
    fn global_last_sets_the_global_and_destroys_itself() {
        let g = trig_game("global_last", &[]);
        let mut w = World::new(g.room, 1);
        w.engine.mech.global_triggered = r(9.0);
        let t = spawn(&g, &mut w, kind_id(&g, "trig"), 0.0, 0.0);
        assert_eq!(
            w.engine.mech.global_triggered,
            r(0.0),
            "Create resets the global id"
        );
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        touch(&g, &mut w, t, kid, ClassMask::KID);
        assert_eq!(w.engine.mech.global_triggered, r(3.0));
        assert!(!alive(&w, t));
    }

    #[test]
    fn global_last_key_variant_needs_the_key() {
        let g = trig_game("global_last", &[("need_key", real(7.0))]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trig"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        touch(&g, &mut w, t, kid, ClassMask::KID);
        assert!(alive(&w, t));
        assert_eq!(w.engine.mech.global_triggered, r(0.0));
        w.engine.mech.global_triggered = r(7.0);
        touch(&g, &mut w, t, kid, ClassMask::KID);
        assert_eq!(w.engine.mech.global_triggered, r(3.0));
        assert!(!alive(&w, t));
    }

    #[test]
    fn flag_array_sets_and_resets() {
        let g = trig_game("flag_array", &[]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trig"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        touch(&g, &mut w, t, kid, ClassMask::KID);
        assert!(w.engine.mech.trigger_flags.contains(&3));
        let g2 = trig_game("flag_array", &[("reset", yes())]);
        let mut w2 = World::new(g2.room, 1);
        w2.engine.mech.trigger_flags.insert(3);
        let t2 = spawn(&g2, &mut w2, kind_id(&g2, "trig"), 0.0, 0.0);
        let kid2 = spawn(&g2, &mut w2, g2.kid, 0.0, 0.0);
        touch(&g2, &mut w2, t2, kid2, ClassMask::KID);
        assert!(!w2.engine.mech.trigger_flags.contains(&3));
    }

    #[test]
    fn flag_array_key_variant_and_stop_array() {
        let g = trig_game("flag_array", &[("need_key", real(1.0))]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trig"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        touch(&g, &mut w, t, kid, ClassMask::KID);
        assert!(w.engine.mech.trigger_flags.is_empty());
        w.engine.mech.trigger_flags.insert(1);
        touch(&g, &mut w, t, kid, ClassMask::KID);
        assert!(w.engine.mech.trigger_flags.contains(&3));
        let g2 = trig_game("flag_array", &[("array", word("stop"))]);
        let mut w2 = World::new(g2.room, 1);
        let t2 = spawn(&g2, &mut w2, kind_id(&g2, "trig"), 0.0, 0.0);
        let kid2 = spawn(&g2, &mut w2, g2.kid, 0.0, 0.0);
        touch(&g2, &mut w2, t2, kid2, ClassMask::KID);
        assert!(w2.engine.mech.stop_flags.contains(&3));
        assert!(w2.engine.mech.trigger_flags.is_empty());
    }

    #[test]
    fn switch_array_and_one_shot_and_frame() {
        let g = trig_game(
            "switch_array",
            &[("one_shot_destroy", yes()), ("sets_frame", yes())],
        );
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trig"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        touch(&g, &mut w, t, kid, ClassMask::KID);
        assert!(w.engine.mech.switches.contains(&3));
        assert_eq!(w.get(t).unwrap().img.index(), r(1.0));
        assert!(!alive(&w, t));
        w.engine.mech.clear_switches();
        assert!(w.engine.mech.switches.is_empty());
    }

    #[test]
    fn per_object_flag_reaches_the_kind_only() {
        let g = game(vec![
            kind(Builtin::Trigger, "t1", params(&[("model", word("per_object"))])),
            kind(Builtin::Trigger, "t2", params(&[("model", word("per_object"))])),
        ]);
        let mut w = World::new(g.room, 1);
        let (k1, k2) = (kind_id(&g, "t1"), kind_id(&g, "t2"));
        let a = spawn(&g, &mut w, k1, 0.0, 0.0);
        spawn(&g, &mut w, k2, 100.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        touch(&g, &mut w, a, kid, ClassMask::KID);
        let seen = |w: &mut World, k: KindId| {
            with(&g, w, |c| {
                builtins::trigger::fired(c, builtins::trigger::Model::PerObject, r(0.0), &[k]).unwrap()
            })
        };
        assert!(seen(&mut w, k1));
        assert!(!seen(&mut w, k2));
    }

    #[test]
    fn trigger_key_needs_a_matching_model() {
        let p = params(&[("model", word("switch_array")), ("need_key", real(1.0))]);
        assert!(builtins::init_state(Builtin::Trigger, &p).is_err());
    }

    // ---- trap -------------------------------------------------------------------------------

    fn trap_game(p: &[(&str, ParamValue)]) -> G {
        let mut items = vec![("trigger_model", word("flag_array")), ("trigger_id", real(1.0))];
        items.extend(p.iter().cloned());
        game(vec![kind(Builtin::Trap, "trap", params(&items))])
    }

    fn step(g: &G, w: &mut World, id: InstId) {
        fire(g, w, id, &EventKind::Step);
        let i = w.get_mut(id).unwrap();
        let (h, v) = (i.m.hspeed(), i.m.vspeed());
        let (x, y) = (i.m.x(), i.m.y());
        i.m.set_x(x + h).unwrap();
        i.m.set_y(y + v).unwrap();
    }

    #[test]
    fn trap_waits_then_launches_once() {
        let g = trap_game(&[("vspeed", real(6.0))]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
        step(&g, &mut w, t);
        assert_eq!(speeds(&w, t), (0.0, 0.0));
        w.engine.mech.trigger_flags.insert(1);
        step(&g, &mut w, t);
        assert_eq!(speeds(&w, t), (0.0, 6.0));
        // Stopped by hand: the speed guard alone would relaunch, the flag is not asked for.
        w.get_mut(t).unwrap().m.set_vspeed(r(0.0)).unwrap();
        step(&g, &mut w, t);
        assert_eq!(
            speeds(&w, t),
            (0.0, 6.0),
            "without flag_guard a stopped trap relaunches"
        );
    }

    #[test]
    fn trap_flag_guard_launches_once() {
        let g = trap_game(&[("vspeed", real(6.0)), ("flag_guard", yes())]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
        w.engine.mech.trigger_flags.insert(1);
        step(&g, &mut w, t);
        w.get_mut(t).unwrap().m.set_vspeed(r(0.0)).unwrap();
        step(&g, &mut w, t);
        assert_eq!(speeds(&w, t), (0.0, 0.0));
    }

    #[test]
    fn trap_with_id_zero_and_shared_trigger_launches_at_once() {
        let g = game(vec![
            kind(
                Builtin::Trigger,
                "trig",
                params(&[("model", word("shared_last"))]),
            ),
            kind(
                Builtin::Trap,
                "trap",
                params(&[
                    ("trigger_model", word("shared_last")),
                    ("trigger_kind", ParamValue::Kind(KindId(3))),
                    ("vspeed", real(-8.0)),
                ]),
            ),
        ]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
        step(&g, &mut w, t);
        assert_eq!(speeds(&w, t), (0.0, -8.0), "uninitialized triggered reads as 0");
    }

    #[test]
    fn trap_angle_launch() {
        let g = trap_game(&[
            ("launch", word("angle")),
            ("dir", real(90.0)),
            ("speed", real(4.0)),
        ]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
        w.engine.mech.trigger_flags.insert(1);
        step(&g, &mut w, t);
        let (h, v) = speeds(&w, t);
        assert!(h.abs() < 1e-9 && (v + 4.0).abs() < 1e-9, "{h} {v}");
    }

    #[test]
    fn trap_stop_chain_walks_its_alarms() {
        let g = trap_game(&[
            ("vspeed", real(-5.0)),
            ("alm", real(3.0)),
            ("alm2", real(2.0)),
            ("v2", real(5.0)),
        ]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
        w.engine.mech.trigger_flags.insert(1);
        step(&g, &mut w, t);
        assert_eq!(speeds(&w, t), (0.0, -5.0));
        assert_eq!(w.get(t).unwrap().alarms[0], r(3.0));
        fire(&g, &mut w, t, &EventKind::Alarm(0));
        assert_eq!(speeds(&w, t), (0.0, 0.0));
        assert_eq!(w.get(t).unwrap().alarms[1], r(2.0));
        fire(&g, &mut w, t, &EventKind::Alarm(1));
        assert_eq!(speeds(&w, t), (0.0, 5.0));
        // Alarm 2 was unset (alm3 = 0), so the chain ends here.
        assert_eq!(w.get(t).unwrap().alarms[2], r(-1.0));
    }

    #[test]
    fn trap_stop_chain_ends_when_the_trigger_is_gone() {
        let g = trap_game(&[
            ("vspeed", real(-5.0)),
            ("alm", real(3.0)),
            ("alm2", real(2.0)),
            ("v2", real(5.0)),
        ]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
        w.engine.mech.trigger_flags.insert(1);
        step(&g, &mut w, t);
        fire(&g, &mut w, t, &EventKind::Alarm(0));
        w.engine.mech.trigger_flags.clear();
        fire(&g, &mut w, t, &EventKind::Alarm(1));
        assert_eq!(speeds(&w, t), (0.0, 0.0));
    }

    #[test]
    fn trap_move_to_runs_toward_its_target_and_snaps() {
        let g = trap_game(&[
            ("launch", word("move_to")),
            ("endx", real(111.0)),
            ("hspd", real(4.0)),
        ]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
        step(&g, &mut w, t);
        assert_eq!(w.get(t).unwrap().m.x(), r(100.0), "waits for global.stop");
        w.engine.mech.stop_flags.insert(1);
        step(&g, &mut w, t);
        step(&g, &mut w, t);
        assert_eq!(w.get(t).unwrap().m.x(), r(108.0));
        step(&g, &mut w, t);
        assert_eq!(w.get(t).unwrap().m.x(), r(111.0), "within half a step: snaps");
        step(&g, &mut w, t);
        assert_eq!(w.get(t).unwrap().m.x(), r(111.0), "step is 0 now");
    }

    #[test]
    fn trap_stopper_zero_speed_and_flush() {
        let g = trap_game(&[("vspeed", real(6.0)), ("stop_on_stopper", word("flush"))]);
        let mut w = World::new(g.room, 1);
        let stopper = spawn(&g, &mut w, KindId(2), 100.0, 200.0);
        let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 165.0);
        w.get_mut(t).unwrap().m.set_vspeed(r(6.0)).unwrap();
        touch(&g, &mut w, t, stopper, ClassMask::TRAP_STOPPER);
        assert_eq!(speeds(&w, t), (0.0, 0.0));
        // Box 32 high: flush puts the bottom row against the stopper top row at 200.
        assert_eq!(w.get(t).unwrap().m.y(), r(168.0));
        let g2 = trap_game(&[("vspeed", real(6.0)), ("stop_on_stopper", word("zero_speed"))]);
        let mut w2 = World::new(g2.room, 1);
        let s2 = spawn(&g2, &mut w2, KindId(2), 100.0, 200.0);
        let t2 = spawn(&g2, &mut w2, kind_id(&g2, "trap"), 100.0, 165.0);
        w2.get_mut(t2).unwrap().m.set_vspeed(r(6.0)).unwrap();
        touch(&g2, &mut w2, t2, s2, ClassMask::TRAP_STOPPER);
        assert_eq!(w2.get(t2).unwrap().m.y(), r(165.0));
        assert_eq!(speeds(&w2, t2), (0.0, 0.0));
    }

    #[test]
    fn trap_depth_reveal_after_32_px() {
        let g = trap_game(&[("vspeed", real(20.0)), ("depth_reveal", yes())]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
        assert_eq!(w.get(t).unwrap().depth, r(1_000_002.0));
        w.engine.mech.trigger_flags.insert(1);
        step(&g, &mut w, t);
        assert_eq!(w.get(t).unwrap().depth, r(1_000_002.0), "20 px is not far enough");
        step(&g, &mut w, t);
        step(&g, &mut w, t);
        assert_eq!(w.get(t).unwrap().depth, r(0.0));
    }

    #[test]
    fn trap_destroyed_outside_the_room_when_asked() {
        let g = trap_game(&[("destroy_outside_room", yes())]);
        let mut w = World::new(g.room, 1);
        let t = spawn(&g, &mut w, kind_id(&g, "trap"), 100.0, 100.0);
        fire(&g, &mut w, t, &EventKind::OutsideRoom);
        assert!(!alive(&w, t));
    }

    #[test]
    fn trap_path_launch_needs_a_path() {
        assert!(builtins::init_state(Builtin::Trap, &params(&[("launch", word("path"))])).is_err());
    }

    #[test]
    fn trap_chain_needs_the_velocity_launch() {
        let p = params(&[("launch", word("angle")), ("alm", real(3.0))]);
        assert!(builtins::init_state(Builtin::Trap, &p).is_err());
    }

    // ---- hidden_block -----------------------------------------------------------------------

    fn hidden_game(p: &[(&str, ParamValue)]) -> G {
        game(vec![kind(Builtin::HiddenBlock, "hid", params(p))])
    }

    #[test]
    fn hidden_block_starts_invisible_and_shows_near_the_kid() {
        let g = hidden_game(&[]);
        let mut w = World::new(g.room, 1);
        let b = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
        assert!(!w.get(b).unwrap().visible);
        // Kid box 4 wide at x 132: next to the block (right edge 131).
        let kid = spawn(&g, &mut w, g.kid, 200.0, 100.0);
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(!w.get(b).unwrap().visible, "far away");
        w.get_mut(kid).unwrap().m.set_x(r(132.0)).unwrap();
        w.get_mut(kid).unwrap().m.set_y(r(100.0)).unwrap();
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(w.get(b).unwrap().visible);
    }

    #[test]
    fn hidden_block_without_a_kid_destroys_itself() {
        let g = hidden_game(&[]);
        let mut w = World::new(g.room, 1);
        let b = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(!alive(&w, b));
        let g2 = hidden_game(&[("destroy_when_no_kid", ParamValue::Bool(false))]);
        let mut w2 = World::new(g2.room, 1);
        let b2 = spawn(&g2, &mut w2, kind_id(&g2, "hid"), 100.0, 100.0);
        fire(&g2, &mut w2, b2, &EventKind::Step);
        assert!(alive(&w2, b2));
    }

    #[test]
    fn hidden_block_can_stop_bullets() {
        let g = hidden_game(&[("stops_bullets", yes())]);
        let mut w = World::new(g.room, 1);
        let b = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
        let bullet = spawn(&g, &mut w, KindId(1), 100.0, 100.0);
        touch(&g, &mut w, b, bullet, ClassMask::KID_BULLET);
        assert!(!alive(&w, bullet));
    }

    #[test]
    fn xia_hidden_block_is_destroyed_by_a_shot_and_by_blood_only_while_invisible() {
        let g = hidden_game(&[("destroyed_by_bullet", yes()), ("destroyed_by_blood", yes())]);
        let mut w = World::new(g.room, 1);
        let bullet = spawn(&g, &mut w, KindId(1), 100.0, 100.0);
        let shot = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
        touch(&g, &mut w, shot, bullet, ClassMask::KID_BULLET);
        assert!(!alive(&w, shot));
        assert!(alive(&w, bullet), "stops_bullets is its own param");
        let bled = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
        touch(&g, &mut w, bled, bullet, ClassMask::BLOOD);
        assert!(!alive(&w, bled), "blood destroys it while invisible");
        let shown = spawn(&g, &mut w, kind_id(&g, "hid"), 100.0, 100.0);
        w.get_mut(shown).unwrap().visible = true;
        touch(&g, &mut w, shown, bullet, ClassMask::BLOOD);
        assert!(alive(&w, shown), "a revealed block ignores blood");
        let plain = hidden_game(&[]);
        let mut w2 = World::new(plain.room, 1);
        let b = spawn(&plain, &mut w2, kind_id(&plain, "hid"), 100.0, 100.0);
        let bullet2 = spawn(&plain, &mut w2, KindId(1), 100.0, 100.0);
        touch(&plain, &mut w2, b, bullet2, ClassMask::KID_BULLET);
        assert!(alive(&w2, b), "without the params a shot does nothing");
    }

    // ---- fake_block -------------------------------------------------------------------------

    fn fake_game(p: &[(&str, ParamValue)]) -> G {
        game(vec![kind(Builtin::FakeBlock, "fake", params(p))])
    }

    #[test]
    fn fake_block_applies_solid_and_visible() {
        let g = fake_game(&[("solid", yes()), ("visible", ParamValue::Bool(false))]);
        let mut w = World::new(g.room, 1);
        let b = spawn(&g, &mut w, kind_id(&g, "fake"), 100.0, 100.0);
        let i = w.get(b).unwrap();
        assert!(i.solid && !i.visible);
    }

    #[test]
    fn fake_block_vanishes_on_touch() {
        let g = fake_game(&[("vanish_on", word("touch"))]);
        let mut w = World::new(g.room, 1);
        let b = spawn(&g, &mut w, kind_id(&g, "fake"), 100.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 100.0, 100.0);
        touch(&g, &mut w, b, kid, ClassMask::KID);
        assert!(!alive(&w, b));
    }

    #[test]
    fn fake_block_vanishes_on_approach() {
        let g = fake_game(&[("vanish_on", word("approach"))]);
        let mut w = World::new(g.room, 1);
        let b = spawn(&g, &mut w, kind_id(&g, "fake"), 100.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 300.0, 100.0);
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(alive(&w, b));
        w.get_mut(kid).unwrap().m.set_x(r(132.0)).unwrap();
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(!alive(&w, b));
    }

    #[test]
    fn fake_block_group_fades_out_after_twenty_steps() {
        let g = fake_game(&[("vanish_on", word("group_fade"))]);
        let mut w = World::new(g.room, 1);
        let k = kind_id(&g, "fake");
        let a = spawn(&g, &mut w, k, 100.0, 100.0);
        let b = spawn(&g, &mut w, k, 300.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 100.0, 100.0);
        fire(&g, &mut w, b, &EventKind::Step);
        assert_eq!(w.get(b).unwrap().img.alpha(), r(1.0), "not active yet");
        touch(&g, &mut w, a, kid, ClassMask::KID);
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(
            (w.get(b).unwrap().img.alpha().get() - 0.95).abs() < 1e-12,
            "both fade"
        );
        for _ in 0..25 {
            fire(&g, &mut w, b, &EventKind::Step);
        }
        assert!(!alive(&w, b));
    }

    #[test]
    fn fake_block_is_solid_for_blood_when_no_kid_exists() {
        let g = fake_game(&[("vanish_on", word("never")), ("blood_solid_when_no_kid", yes())]);
        let mut w = World::new(g.room, 1);
        let b = spawn(&g, &mut w, kind_id(&g, "fake"), 100.0, 100.0);
        let kid = spawn(&g, &mut w, g.kid, 300.0, 100.0);
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(!w.get(b).unwrap().solid);
        with(&g, &mut w, |c| c.destroy(kid));
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(w.get(b).unwrap().solid);
    }

    #[test]
    fn fake_block_tile_delete_is_refused() {
        let p = params(&[("vanish_on", word("tile_delete"))]);
        assert!(builtins::init_state(Builtin::FakeBlock, &p).is_err());
    }

    #[test]
    fn a_marker_reverses_a_moving_platform() {
        let g = game(vec![kind(
            Builtin::MovingPlatform,
            "plat",
            params(&[("hspeed", real(2.0))]),
        )]);
        let mut w = World::new(g.room, 1);
        let p = spawn(&g, &mut w, kind_id(&g, "plat"), 100.0, 100.0);
        let marker = spawn(&g, &mut w, KindId(2), 100.0, 100.0);
        touch(&g, &mut w, p, marker, ClassMask::FAKE_BLOCK);
        assert_eq!(speeds(&w, p), (-2.0, 0.0));
    }
}
