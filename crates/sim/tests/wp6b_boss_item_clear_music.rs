//! WP6b.4: music, boss_gate, boss_item, secret_item, item_viewer, game_clear (docs/plans/
//! sim-core.md 6.1). Hand-built worlds; inert kinds stand in for the kid. The music events call
//! the WP10 sound model, which is not implemented yet, so only its params are tested.

mod wp6b {
    use iwbtg_sim::builtins;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{
        Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, RoomId,
        RoomRole, SoundId,
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

    fn word(s: &str) -> ParamValue {
        ParamValue::Str(s.to_owned())
    }

    struct G {
        def: GameDef,
        room: RoomId,
        kid: KindId,
    }

    /// A game with a kid stand-in, a boss stand-in (kind 1) and `extra` kinds (from kind 2).
    fn game(extra: Vec<KindDef>) -> G {
        let mut b = GameDefBuilder::new("wp6b-4");
        let spr = b.sprite_rect((32, 32), (0, 0));
        let mut k = KindDef::new("kid", Builtin::Inert);
        k.classes = ClassMask::KID;
        k.sprite = Some(spr);
        let kid = b.kind(k);
        let mut boss = KindDef::new("boss", Builtin::Inert);
        boss.sprite = Some(spr);
        b.kind(boss);
        b.engine_kind(EngineKind::Kid, kid);
        for mut kd in extra {
            kd.sprite = Some(spr);
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

    fn touch(g: &G, w: &mut World, id: InstId, other: InstId) {
        fire(
            g,
            w,
            id,
            &EventKind::Collision {
                with: ClassMask::KID,
                other,
            },
        );
    }

    fn alive(w: &World, id: InstId) -> bool {
        w.get(id).is_some_and(|i| i.alive)
    }

    // ---- flags ------------------------------------------------------------------------------

    #[test]
    fn flag_helpers_use_one_bit_per_index() {
        let mut m = builtins::MechGlobals::default();
        assert!(!m.boss(3));
        m.set_boss(3);
        m.set_item(15);
        assert!(m.boss(3) && !m.boss(4) && m.item(15));
        assert_eq!((m.bosses, m.items), (8, 0x8000));
    }

    #[test]
    fn flag_indexes_are_checked() {
        for bad in [real(16.0), real(-1.0), real(1.5)] {
            let p = params(&[("boss_index", bad)]);
            assert!(builtins::init_state(Builtin::BossGate, &p).is_err());
        }
        assert!(builtins::init_state(Builtin::BossGate, &Params::default()).is_err());
    }

    // ---- boss_gate --------------------------------------------------------------------------

    #[test]
    fn boss_gate_stays_until_its_boss_is_down() {
        let g = game(vec![kind(
            Builtin::BossGate,
            "gate",
            params(&[("boss_index", real(1.0))]),
        )]);
        let mut w = World::new(g.room, 1);
        let gate = spawn(&g, &mut w, kind_id(&g, "gate"), 0.0, 0.0);
        fire(&g, &mut w, gate, &EventKind::Step);
        assert!(alive(&w, gate));
        w.engine.mech.set_boss(1);
        fire(&g, &mut w, gate, &EventKind::Step);
        assert!(!alive(&w, gate));
        let late = spawn(&g, &mut w, kind_id(&g, "gate"), 0.0, 0.0);
        assert!(!alive(&w, late), "Create checks too");
    }

    // ---- boss_item --------------------------------------------------------------------------

    fn boss_item_game() -> G {
        game(vec![kind(
            Builtin::BossItem,
            "reward",
            params(&[
                ("boss_index", real(1.0)),
                ("watched_object", ParamValue::Kind(KindId(1))),
            ]),
        )])
    }

    #[test]
    fn boss_item_appears_when_the_boss_is_gone_and_is_taken_by_touch() {
        let g = boss_item_game();
        let mut w = World::new(g.room, 1);
        let boss = spawn(&g, &mut w, KindId(1), 100.0, 100.0);
        let item = spawn(&g, &mut w, kind_id(&g, "reward"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        assert!(!w.get(item).unwrap().visible);
        fire(&g, &mut w, item, &EventKind::Step);
        assert!(!w.get(item).unwrap().visible, "the boss still lives");
        touch(&g, &mut w, item, kid);
        assert!(alive(&w, item) && !w.engine.mech.boss(1), "invisible: no touch");
        with(&g, &mut w, |c| c.destroy(boss));
        fire(&g, &mut w, item, &EventKind::Step);
        assert!(w.get(item).unwrap().visible);
        touch(&g, &mut w, item, kid);
        assert!(w.engine.mech.boss(1));
        assert!(!alive(&w, item));
    }

    #[test]
    fn boss_item_with_the_flag_set_destroys_itself() {
        let g = boss_item_game();
        let mut w = World::new(g.room, 1);
        w.engine.mech.set_boss(1);
        let item = spawn(&g, &mut w, kind_id(&g, "reward"), 0.0, 0.0);
        fire(&g, &mut w, item, &EventKind::Step);
        assert!(!alive(&w, item));
    }

    // ---- secret_item ------------------------------------------------------------------------

    #[test]
    fn secret_item_sets_its_flag_and_is_removed_when_taken() {
        let g = game(vec![kind(
            Builtin::SecretItem,
            "secret",
            params(&[("item_index", real(2.0))]),
        )]);
        let mut w = World::new(g.room, 1);
        let it = spawn(&g, &mut w, kind_id(&g, "secret"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        touch(&g, &mut w, it, kid);
        assert!(w.engine.mech.item(2));
        assert!(!alive(&w, it));
        let again = spawn(&g, &mut w, kind_id(&g, "secret"), 0.0, 0.0);
        assert!(!alive(&w, again), "Create removes a taken item");
    }

    // ---- item_viewer ------------------------------------------------------------------------

    #[test]
    fn item_viewer_shows_only_while_its_flag_is_set() {
        let g = game(vec![
            kind(
                Builtin::ItemViewer,
                "boss_view",
                params(&[("kind", word("boss")), ("index", real(1.0))]),
            ),
            kind(
                Builtin::ItemViewer,
                "item_view",
                params(&[("kind", word("item")), ("index", real(1.0))]),
            ),
        ]);
        let mut w = World::new(g.room, 1);
        let bv = spawn(&g, &mut w, kind_id(&g, "boss_view"), 0.0, 0.0);
        let iv = spawn(&g, &mut w, kind_id(&g, "item_view"), 0.0, 0.0);
        assert!(!w.get(bv).unwrap().visible && !w.get(iv).unwrap().visible);
        w.engine.mech.set_boss(1);
        fire(&g, &mut w, bv, &EventKind::Step);
        fire(&g, &mut w, iv, &EventKind::Step);
        assert!(w.get(bv).unwrap().visible, "boss flag 1 set");
        assert!(!w.get(iv).unwrap().visible, "item flag 1 not set");
    }

    // ---- game_clear -------------------------------------------------------------------------

    #[test]
    fn game_clear_sets_the_flag_saves_and_raises_run_end() {
        let g = game(vec![kind(Builtin::GameClear, "clear", Params::default())]);
        let mut w = World::new(g.room, 1);
        let c = spawn(&g, &mut w, kind_id(&g, "clear"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 100.0, 200.0);
        touch(&g, &mut w, c, kid);
        assert!(w.engine.run.clear);
        assert!(w.timing.run_end_raised);
        let slot = w.save.as_ref().expect("saved");
        assert!(slot.clear);
        assert_eq!(slot.room, g.room);
    }

    #[test]
    fn game_clear_without_position_updates_only_an_existing_slot() {
        let p = params(&[("save_with_position", ParamValue::Bool(false))]);
        let g = game(vec![kind(Builtin::GameClear, "clear", p)]);
        let mut w = World::new(g.room, 1);
        let c = spawn(&g, &mut w, kind_id(&g, "clear"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 100.0, 200.0);
        touch(&g, &mut w, c, kid);
        assert!(w.engine.run.clear && w.timing.run_end_raised);
        assert!(w.save.is_none(), "no slot to update");
    }

    #[test]
    fn game_clear_needs_its_fixed_point() {
        let p = params(&[("saved_position", word("fixed_point"))]);
        assert!(builtins::init_state(Builtin::GameClear, &p).is_err());
    }

    // ---- music ------------------------------------------------------------------------------

    #[test]
    fn music_needs_a_track_and_picks_its_events_from_the_mode() {
        assert!(builtins::init_state(Builtin::Music, &Params::default()).is_err());
        let track = ("track", ParamValue::Sound(SoundId(0)));
        let once = kind(
            Builtin::Music,
            "m1",
            params(&[("track", ParamValue::Sound(SoundId(0)))]),
        );
        assert!(!builtins::event_set(Builtin::Music, &once).step);
        let each = kind(
            Builtin::Music,
            "m2",
            params(&[track, ("mode", word("every_step_if_not_playing"))]),
        );
        assert!(builtins::event_set(Builtin::Music, &each).step);
    }
}
