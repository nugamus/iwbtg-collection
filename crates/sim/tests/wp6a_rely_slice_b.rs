//! WP6a.2: rely slice, part 2: save, warp, difficulty_warp (docs/plans/sim-core.md 6.2, 12).
//! Hand-built worlds; inert kinds stand in for the kid and the bow.

mod wp6a {
    use iwbtg_sim::builtins;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{
        Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, RoomId,
    };
    use iwbtg_sim::events::EventKind;
    use iwbtg_sim::input::{Action, InputFrame};
    use iwbtg_sim::num::Real;
    use iwbtg_sim::output::OutputEvent;
    use iwbtg_sim::room::{RoomOutput, RoomRequest};
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

    fn yes() -> ParamValue {
        ParamValue::Bool(true)
    }

    struct G {
        def: GameDef,
        room: RoomId,
        room2: RoomId,
        kid: KindId,
        flipped: KindId,
        bow: KindId,
    }

    /// A game with kid kinds, a bow and the kinds `kinds` builds (it gets `room2` for warps).
    fn game(kinds: impl FnOnce(RoomId) -> Vec<KindDef>) -> G {
        let mut b = GameDefBuilder::new("wp6a-b");
        let spr32 = b.sprite_rect((32, 32), (0, 0));
        let spr4 = b.sprite_rect((4, 4), (0, 0));
        let mut k = KindDef::new("kid", Builtin::Inert);
        k.persistent = true;
        k.classes = ClassMask::KID;
        k.sprite = Some(spr4);
        let kid = b.kind(k);
        let mut kf = KindDef::new("kid2", Builtin::Inert);
        kf.persistent = true;
        kf.classes = ClassMask::KID_FLIPPED;
        kf.sprite = Some(spr4);
        let flipped = b.kind(kf);
        let bow = b.kind(KindDef::new("bow", Builtin::Inert));
        b.engine_kind(EngineKind::Kid, kid);
        b.engine_kind(EngineKind::KidFlipped, flipped);
        b.engine_kind(EngineKind::Bow, bow);
        let room = b.room(RoomDef::new("r", 800, 608));
        let room2 = b.room(RoomDef::new("r2", 800, 608));
        for mut kd in kinds(room2) {
            kd.sprite = Some(spr32);
            b.kind(kd);
        }
        let mut def = b.build().unwrap();
        def.new_game_room = room;
        G {
            def,
            room,
            room2,
            kid,
            flipped,
            bow,
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

    fn spawn(g: &G, w: &mut World, kind: KindId, x: f64, y: f64) -> InstId {
        with(g, w, |c| c.spawn(kind, r(x), r(y), SpawnInit::default()).unwrap())
    }

    fn fire(g: &G, w: &mut World, id: InstId, ev: &EventKind) -> Flow {
        with(g, w, |c| builtins::on_event(c, id, ev).unwrap())
    }

    fn touch(kid: InstId) -> EventKind {
        EventKind::Collision {
            with: ClassMask::KID,
            other: kid,
        }
    }

    fn alive(w: &World, kind: KindId) -> Vec<InstId> {
        w.iter_alive().filter(|i| i.kind == kind).map(|i| i.id).collect()
    }

    fn frame(w: &mut World, pressed: &[Action]) {
        let bits = pressed.iter().fold(0u32, |acc, x| acc | x.bit());
        w.input.latch(InputFrame {
            held: bits,
            pressed: bits,
            released: 0,
        });
    }

    // ---- warp -------------------------------------------------------------------------------

    fn warp_game(p: impl FnOnce(RoomId) -> Params) -> G {
        game(|room2| {
            let mut k = KindDef::new("warp", Builtin::Warp);
            let mut p = p(room2);
            p.0.insert("room_to".to_owned(), ParamValue::Room(room2));
            k.params = p;
            vec![k]
        })
    }

    #[test]
    fn warp_to_zero_zero_destroys_the_kid_and_the_bow_and_requests_the_room() {
        let g = warp_game(|_| Params::default());
        let mut w = World::new(g.room, 1);
        let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
        spawn(&g, &mut w, g.bow, 5.0, 5.0);
        let flow = fire(&g, &mut w, warp, &touch(kid));
        assert_eq!(flow, Flow::AbortTick);
        assert_eq!(w.pending, Some(RoomRequest::Goto(g.room2)));
        assert!(alive(&w, g.kid).is_empty() && alive(&w, g.bow).is_empty());
    }

    #[test]
    fn warp_with_a_target_moves_the_kid_and_keeps_his_speed() {
        let g = warp_game(|_| params(&[("warp_x", real(40.0)), ("warp_y", real(80.0))]));
        let mut w = World::new(g.room, 1);
        let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
        w.get_mut(kid).unwrap().m.set_hspeed(r(3.0)).unwrap();
        fire(&g, &mut w, warp, &touch(kid));
        let k = w.get(kid).unwrap();
        assert!(k.alive);
        assert_eq!((k.m.x(), k.m.y(), k.m.hspeed()), (r(40.0), r(80.0), r(3.0)));
        assert_eq!(w.pending, Some(RoomRequest::Goto(g.room2)));
    }

    #[test]
    fn warp_with_on_collide_off_does_nothing_and_a_flipped_kid_also_triggers_it() {
        let g = warp_game(|_| params(&[("on_collide", ParamValue::Bool(false))]));
        let mut w = World::new(g.room, 1);
        let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
        assert_eq!(fire(&g, &mut w, warp, &touch(kid)), Flow::Continue);
        assert_eq!(w.pending, None);
        let g = warp_game(|_| Params::default());
        let mut w = World::new(g.room, 1);
        let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.flipped, 5.0, 5.0);
        let ev = EventKind::Collision {
            with: ClassMask::KID_FLIPPED,
            other: kid,
        };
        assert_eq!(fire(&g, &mut w, warp, &ev), Flow::AbortTick);
        // The handler is declared for both kid classes.
        let set = builtins::event_set(
            Builtin::Warp,
            &g.def.kinds[usize::try_from(kind_id(&g, "warp").0).unwrap()],
        );
        assert!(
            set.collisions[0].contains(ClassMask::KID) && set.collisions[0].contains(ClassMask::KID_FLIPPED)
        );
    }

    #[test]
    fn warp_transition_goes_to_the_client() {
        let g = warp_game(|_| params(&[("transition", real(21.0)), ("transition_steps", real(30.0))]));
        let mut w = World::new(g.room, 1);
        let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
        fire(&g, &mut w, warp, &touch(kid));
        assert!(
            w.output
                .contains(&OutputEvent::Room(RoomOutput::Transition { kind: 21, steps: 30 }))
        );
    }

    #[test]
    fn warp_params_are_checked() {
        assert!(
            builtins::init_state(Builtin::Warp, &Params::default()).is_err(),
            "room_to is required"
        );
        let p = params(&[
            ("room_to", ParamValue::Room(RoomId(1))),
            ("transition", real(-1.0)),
        ]);
        assert!(builtins::init_state(Builtin::Warp, &p).is_err());
        let p = params(&[
            ("room_to", ParamValue::Room(RoomId(1))),
            ("transition", real(300.0)),
        ]);
        assert!(builtins::init_state(Builtin::Warp, &p).is_err());
    }

    // ---- difficulty_warp --------------------------------------------------------------------

    #[test]
    fn difficulty_warp_starts_a_new_game() {
        let g = game(|_| {
            let mut k = KindDef::new("dw", Builtin::DifficultyWarp);
            k.params = params(&[("dif", real(2.0))]);
            vec![k]
        });
        let mut w = World::new(g.room2, 1);
        w.save = Some(iwbtg_sim::save::SaveSlot {
            room: g.room2,
            x: r(1.0),
            y: r(1.0),
            flipped: false,
            facing: 1,
            difficulty: 0,
            deaths: 4,
            time: 5,
            bosses: 0,
            items: 0,
            clear: false,
        });
        w.engine.run.deaths = 4;
        w.engine.run.time = 5;
        let dw = spawn(&g, &mut w, kind_id(&g, "dw"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
        spawn(&g, &mut w, g.bow, 5.0, 5.0);
        let flow = fire(&g, &mut w, dw, &touch(kid));
        assert_eq!(flow, Flow::AbortTick);
        assert_eq!(w.engine.run.difficulty, 2);
        assert!(w.save.is_none(), "the save is deleted");
        assert_eq!((w.engine.run.deaths, w.engine.run.time), (0, 0));
        assert_eq!(
            w.pending,
            Some(RoomRequest::Goto(g.room)),
            "the game's new-game room"
        );
        assert!(alive(&w, g.kid).is_empty() && alive(&w, g.bow).is_empty());
    }

    #[test]
    fn difficulty_warp_room_to_overrides_and_dif_is_required() {
        let g = game(|room2| {
            let mut k = KindDef::new("dw", Builtin::DifficultyWarp);
            k.params = params(&[("dif", real(0.0)), ("room_to", ParamValue::Room(room2))]);
            vec![k]
        });
        let mut w = World::new(g.room, 1);
        let dw = spawn(&g, &mut w, kind_id(&g, "dw"), 0.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
        fire(&g, &mut w, dw, &touch(kid));
        assert_eq!(w.pending, Some(RoomRequest::Goto(g.room2)));
        assert!(builtins::init_state(Builtin::DifficultyWarp, &Params::default()).is_err());
        assert!(builtins::init_state(Builtin::DifficultyWarp, &params(&[("dif", real(1.5))])).is_err());
    }

    // ---- save -------------------------------------------------------------------------------

    /// A game with one save point kind `sp` taking `p`.
    fn save_game(p: Params) -> G {
        game(|_| {
            let mut k = KindDef::new("sp", Builtin::Save);
            k.params = p;
            vec![k]
        })
    }

    /// A save point at (0, 0) over a kid at (49.4, 567.4) in room r, and the pair's ids.
    fn save_world(g: &G) -> (World, InstId, InstId) {
        let mut w = World::new(g.room, 1);
        let sp = spawn(g, &mut w, kind_id(g, "sp"), 0.0, 0.0);
        let kid = spawn(g, &mut w, g.kid, 10.4, 10.4);
        (w, sp, kid)
    }

    #[test]
    fn touch_press_saves_only_with_s_or_z_pressed() {
        let g = save_game(Params::default());
        let (mut w, sp, kid) = save_world(&g);
        frame(&mut w, &[]);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_none(), "touching alone does nothing");
        frame(&mut w, &[Action::Shoot]);
        fire(&g, &mut w, sp, &touch(kid));
        let s = w.save.clone().unwrap();
        assert_eq!(
            (s.room, s.x, s.y),
            (g.room, r(10.0), r(10.0)),
            "base 100 bytes round 10.4 to 10"
        );
        w.save = None;
        frame(&mut w, &[Action::Skip]);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_some(), "S saves too");
    }

    #[test]
    fn shoot_press_ignores_s() {
        let g = save_game(params(&[("on_touch", word("shoot_press"))]));
        let (mut w, sp, kid) = save_world(&g);
        frame(&mut w, &[Action::Skip]);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_none());
        frame(&mut w, &[Action::Shoot]);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_some());
    }

    #[test]
    fn on_shot_saves_from_the_live_kid() {
        let g = save_game(params(&[("on_touch", word("never")), ("on_shot", yes())]));
        let (mut w, sp, kid) = save_world(&g);
        frame(&mut w, &[Action::Shoot]);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_none(), "touch is off");
        let bullet = spawn(&g, &mut w, g.bow, 0.0, 0.0);
        fire(
            &g,
            &mut w,
            sp,
            &EventKind::Collision {
                with: ClassMask::KID_BULLET,
                other: bullet,
            },
        );
        assert!(w.save.is_some());
        // The handlers follow the params.
        let set = builtins::event_set(
            Builtin::Save,
            &g.def.kinds[usize::try_from(kind_id(&g, "sp").0).unwrap()],
        );
        assert_eq!(set.collisions.as_slice(), &[ClassMask::KID_BULLET]);
    }

    #[test]
    fn cooldown_gates_until_the_timer_runs_out() {
        let g = save_game(params(&[("on_touch", word("always")), ("cooldown", real(30.0))]));
        let (mut w, sp, kid) = save_world(&g);
        frame(&mut w, &[]);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_some());
        w.save = None;
        for _ in 0..30 {
            fire(&g, &mut w, sp, &EventKind::Step);
        }
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_none(), "timer is 0 after 30 steps: still blocked");
        fire(&g, &mut w, sp, &EventKind::Step);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_some(), "below 0: saves again");
        // rely's cooldown 0 never gates.
        let g = save_game(Params::default());
        let (mut w, sp, kid) = save_world(&g);
        frame(&mut w, &[Action::Shoot]);
        fire(&g, &mut w, sp, &touch(kid));
        w.save = None;
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_some());
    }

    #[test]
    fn difficulty_filter_destroys_in_create() {
        let g = save_game(params(&[("min_difficulty_removed", real(1.0))]));
        let mut w = World::new(g.room, 1);
        w.engine.run.difficulty = 1;
        let kept = spawn(&g, &mut w, kind_id(&g, "sp"), 0.0, 0.0);
        assert!(w.get(kept).unwrap().alive, "removed only above the value");
        w.engine.run.difficulty = 2;
        let gone = spawn(&g, &mut w, kind_id(&g, "sp"), 0.0, 0.0);
        assert!(!w.get(gone).unwrap().alive);
    }

    #[test]
    fn gravity_and_depth_requirements() {
        let g = save_game(params(&[
            ("on_touch", word("always")),
            ("requires_gravity", word("flipped")),
        ]));
        let (mut w, sp, kid) = save_world(&g);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_none(), "a normal kid is refused");
        w.engine.kid.grav = true;
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_some(), "global.grav matches");
        let g = save_game(params(&[
            ("on_touch", word("always")),
            ("requires_gravity", word("normal")),
        ]));
        let (mut w, sp, kid) = save_world(&g);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_some());
        let g = save_game(params(&[
            ("on_touch", word("always")),
            ("requires_kid_depth0", yes()),
        ]));
        let (mut w, sp, kid) = save_world(&g);
        w.get_mut(kid).unwrap().depth = r(5.0);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_none());
        w.get_mut(kid).unwrap().depth = r(0.0);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_some());
    }

    #[test]
    fn once_destroys_after_the_save() {
        let g = save_game(params(&[("on_touch", word("always")), ("once", yes())]));
        let (mut w, sp, kid) = save_world(&g);
        fire(&g, &mut w, sp, &touch(kid));
        assert!(w.save.is_some() && !w.get(sp).unwrap().alive);
    }

    #[test]
    fn blink_shows_frame_one_until_animation_end() {
        let g = save_game(Params::default());
        let (mut w, sp, kid) = save_world(&g);
        frame(&mut w, &[Action::Shoot]);
        fire(&g, &mut w, sp, &touch(kid));
        let i = w.get(sp).unwrap();
        assert_eq!((i.img.index(), i.img.speed()), (r(1.0), r(0.017)));
        fire(&g, &mut w, sp, &EventKind::AnimationEnd);
        let i = w.get(sp).unwrap();
        assert_eq!((i.img.index(), i.img.speed()), (r(0.0), r(0.0)));
    }

    #[test]
    fn held_s_saves_only_while_overlapping_the_kid() {
        let g = save_game(params(&[
            ("on_touch", word("skip_held")),
            ("saved_position", word("xy_previous")),
        ]));
        let (mut w, sp, kid) = save_world(&g);
        w.get_mut(kid).unwrap().m.set_xprevious(r(9.25)).unwrap();
        w.get_mut(kid).unwrap().m.set_yprevious(r(8.5)).unwrap();
        fire(&g, &mut w, sp, &EventKind::KeyHeld(Action::Skip));
        let s = w.save.clone().unwrap();
        assert_eq!(
            (s.x, s.y),
            (r(9.25), r(8.5)),
            "kaenbyou keeps the exact previous position"
        );
        w.save = None;
        w.get_mut(kid).unwrap().m.set_x(r(300.0)).unwrap();
        fire(&g, &mut w, sp, &EventKind::KeyHeld(Action::Skip));
        assert!(w.save.is_none(), "not overlapping");
        let set = builtins::event_set(
            Builtin::Save,
            &g.def.kinds[usize::try_from(kind_id(&g, "sp").0).unwrap()],
        );
        assert_eq!(set.key_held, Action::Skip.bit());
        assert!(set.collisions.is_empty());
    }

    #[test]
    fn fixed_point_and_self_destroy_alarm() {
        let g = save_game(params(&[
            ("on_touch", word("always")),
            ("saved_position", word("fixed_point")),
            ("fixed_x", real(100.0)),
            ("fixed_y", real(200.0)),
            ("self_destroy_alarm", real(5.0)),
        ]));
        let (mut w, sp, kid) = save_world(&g);
        fire(&g, &mut w, sp, &touch(kid));
        let s = w.save.clone().unwrap();
        assert_eq!((s.x, s.y), (r(100.0), r(200.0)));
        assert_eq!(w.get(sp).unwrap().alarms[0], r(5.0));
        fire(&g, &mut w, sp, &EventKind::Alarm(0));
        assert!(!w.get(sp).unwrap().alive);
        assert!(
            builtins::init_state(Builtin::Save, &params(&[("saved_position", word("fixed_point"))])).is_err()
        );
    }
}
