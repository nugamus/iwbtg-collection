//! WP6a (#233): the world built-in's pause, mute flag and play timer (docs/plans/sim-core.md 6.1).

mod wp6a {
    use iwbtg_sim::builtins::{self, world};
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{
        Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RestartTrigger,
        RoomDef, RoomId,
    };
    use iwbtg_sim::events::EventKind;
    use iwbtg_sim::input::{Action, InputFrame};
    use iwbtg_sim::num::Real;
    use iwbtg_sim::room::RoomRequest;
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::world::{Ctx, Flow, InstId, SpawnInit, World};

    fn params(items: &[(&str, ParamValue)]) -> Params {
        let mut p = Params::default();
        for (k, v) in items {
            p.0.insert((*k).to_owned(), v.clone());
        }
        p
    }

    fn real(v: f64) -> ParamValue {
        ParamValue::Real(Real::new(v))
    }

    fn yes() -> ParamValue {
        ParamValue::Bool(true)
    }

    fn deactivate() -> (&'static str, ParamValue) {
        ("pause", ParamValue::Str("deactivate".into()))
    }

    struct G {
        def: GameDef,
        room: RoomId,
        world: KindId,
        kid: KindId,
    }

    fn game(p: Params, edit: impl FnOnce(&mut GameDef)) -> G {
        let mut b = GameDefBuilder::new("wp6a-w");
        let mut k = KindDef::new("kid", Builtin::Inert);
        k.classes = ClassMask::KID;
        let kid = b.kind(k);
        let mut w = KindDef::new("world", Builtin::World);
        w.persistent = true;
        w.params = p;
        let world = b.kind(w);
        b.engine_kind(EngineKind::Kid, kid);
        let room = b.room(RoomDef::new("r", 800, 608));
        let mut def = b.build().unwrap();
        for k in &mut def.kinds {
            k.events = builtins::event_set(k.base, &k.clone());
        }
        edit(&mut def);
        G {
            def,
            room,
            world,
            kid,
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

    fn spawn(g: &G, w: &mut World, kind: KindId) -> InstId {
        with(g, w, |c| {
            c.spawn(kind, Real::ZERO, Real::ZERO, SpawnInit::default())
                .unwrap()
        })
    }

    fn fire(g: &G, w: &mut World, id: InstId, ev: &EventKind) -> Flow {
        with(g, w, |c| builtins::on_event(c, id, ev).unwrap())
    }

    fn frame(w: &mut World, held: &[Action], pressed: &[Action]) {
        let bits = |a: &[Action]| a.iter().fold(0u32, |acc, x| acc | x.bit());
        w.input.latch(InputFrame {
            held: bits(held),
            pressed: bits(pressed),
            released: 0,
        });
    }

    fn events(g: &G) -> iwbtg_sim::events::EventSet {
        builtins::event_set(Builtin::World, &g.def.kinds[usize::try_from(g.world.0).unwrap()])
    }

    // ---- pause ------------------------------------------------------------------------------

    #[test]
    fn pause_deactivates_everything_else_and_unpause_wakes_only_those() {
        let g = game(params(&[deactivate()]), |_| {});
        let mut w = World::new(g.room, 1);
        let world = spawn(&g, &mut w, g.world);
        let a = spawn(&g, &mut w, g.kid);
        let sleeper = spawn(&g, &mut w, g.kid);
        w.get_mut(sleeper).unwrap().active = false; // deactivated by something else
        frame(&mut w, &[], &[]);
        fire(&g, &mut w, world, &EventKind::Step);
        assert!(!world::is_paused(&w, world));
        frame(&mut w, &[Action::Pause, Action::Left], &[Action::Pause]);
        fire(&g, &mut w, world, &EventKind::Step);
        assert!(world::is_paused(&w, world));
        assert!(w.get(world).unwrap().active, "the controller keeps running");
        assert!(!w.get(a).unwrap().active);
        // The next press wakes the ones the pause deactivated, not the other sleeper, and drops
        // the held keys (io_clear).
        frame(&mut w, &[Action::Left, Action::Pause], &[Action::Pause]);
        fire(&g, &mut w, world, &EventKind::Step);
        assert!(!world::is_paused(&w, world));
        assert!(w.get(a).unwrap().active);
        assert!(!w.get(sleeper).unwrap().active);
        assert!(
            !w.input.held(Action::Left) && !w.input.pressed(Action::Pause),
            "io_clear"
        );
    }

    #[test]
    fn pause_delay_blocks_a_quick_second_toggle() {
        let g = game(params(&[deactivate(), ("pause_delay", real(3.0))]), |_| {});
        let mut w = World::new(g.room, 1);
        let world = spawn(&g, &mut w, g.world);
        frame(&mut w, &[], &[Action::Pause]);
        fire(&g, &mut w, world, &EventKind::Step);
        assert!(world::is_paused(&w, world));
        for _ in 0..2 {
            frame(&mut w, &[], &[Action::Pause]);
            fire(&g, &mut w, world, &EventKind::Step);
            assert!(world::is_paused(&w, world), "still inside the delay");
        }
        frame(&mut w, &[], &[Action::Pause]);
        fire(&g, &mut w, world, &EventKind::Step);
        assert!(!world::is_paused(&w, world));
    }

    #[test]
    fn pause_none_does_nothing_in_the_sim() {
        let g = game(Params::default(), |_| {});
        let mut w = World::new(g.room, 1);
        let world = spawn(&g, &mut w, g.world);
        let a = spawn(&g, &mut w, g.kid);
        frame(&mut w, &[], &[Action::Pause]);
        fire(&g, &mut w, world, &EventKind::Step);
        assert!(!world::is_paused(&w, world) && w.get(a).unwrap().active);
    }

    #[test]
    fn skip_while_paused_skips_the_retry() {
        for (skip, expect) in [(true, None), (false, Some(RoomRequest::GameRestart))] {
            let g = game(params(&[deactivate()]), |d| {
                d.restart.trigger = RestartTrigger::StepPressed {
                    skip_while_paused: skip,
                }
            });
            let mut w = World::new(g.room, 1);
            let world = spawn(&g, &mut w, g.world);
            frame(&mut w, &[], &[Action::Pause]);
            fire(&g, &mut w, world, &EventKind::Step);
            assert!(world::is_paused(&w, world));
            frame(&mut w, &[], &[Action::Restart]);
            fire(&g, &mut w, world, &EventKind::Step);
            assert_eq!(w.pending, expect, "skip_while_paused = {skip}");
        }
    }

    // ---- mute -------------------------------------------------------------------------------

    #[test]
    fn mute_press_flips_the_music_flag_only_when_the_game_has_it() {
        let g = game(params(&[("mute", yes())]), |_| {});
        assert_ne!(events(&g).key_press & Action::Mute.bit(), 0);
        let mut w = World::new(g.room, 1);
        let world = spawn(&g, &mut w, g.world);
        fire(&g, &mut w, world, &EventKind::KeyPress(Action::Mute));
        assert!(w.engine.mech.music_muted);
        fire(&g, &mut w, world, &EventKind::KeyPress(Action::Mute));
        assert!(!w.engine.mech.music_muted);
        let g = game(Params::default(), |_| {});
        assert_eq!(events(&g).key_press & Action::Mute.bit(), 0);
        let mut w = World::new(g.room, 1);
        let world = spawn(&g, &mut w, g.world);
        fire(&g, &mut w, world, &EventKind::KeyPress(Action::Mute));
        assert!(!w.engine.mech.music_muted);
    }

    // ---- play timer -------------------------------------------------------------------------

    #[test]
    fn family_b_timer_counts_two_a_tick_and_two_on_retry() {
        let g = game(
            params(&[("timer_per_tick", real(2.0)), ("timer_on_restart", real(2.0))]),
            |_| {},
        );
        let mut w = World::new(g.room, 1);
        let world = spawn(&g, &mut w, g.world);
        frame(&mut w, &[], &[]);
        for _ in 0..5 {
            fire(&g, &mut w, world, &EventKind::Step);
        }
        assert_eq!(w.engine.run.time, 10);
        fire(&g, &mut w, world, &EventKind::KeyPress(Action::Restart));
        assert_eq!(w.engine.run.time, 12, "the retry adds 2 before the restart");
        // A game without the params leaves the counter alone.
        let g = game(Params::default(), |_| {});
        let mut w = World::new(g.room, 1);
        let world = spawn(&g, &mut w, g.world);
        frame(&mut w, &[], &[]);
        fire(&g, &mut w, world, &EventKind::Step);
        assert_eq!(w.engine.run.time, 0);
    }

    #[test]
    fn timer_stops_while_paused_and_overflow_is_an_error() {
        let g = game(params(&[deactivate(), ("timer_per_tick", real(2.0))]), |_| {});
        let mut w = World::new(g.room, 1);
        let world = spawn(&g, &mut w, g.world);
        frame(&mut w, &[], &[Action::Pause]);
        fire(&g, &mut w, world, &EventKind::Step);
        frame(&mut w, &[], &[]);
        fire(&g, &mut w, world, &EventKind::Step);
        assert_eq!(w.engine.run.time, 0);
        let g = game(params(&[("timer_per_tick", real(2.0))]), |_| {});
        let mut w = World::new(g.room, 1);
        let world = spawn(&g, &mut w, g.world);
        w.engine.run.time = u32::MAX - 2;
        frame(&mut w, &[], &[]);
        fire(&g, &mut w, world, &EventKind::Step);
        assert_eq!(w.engine.run.time, u32::MAX);
        let r = with(&g, &mut w, |c| builtins::on_event(c, world, &EventKind::Step));
        assert!(r.is_err(), "checked add");
    }

    #[test]
    fn world_params_are_checked() {
        let bad = |p: Params| builtins::init_state(Builtin::World, &p).is_err();
        assert!(bad(params(&[("pause", ParamValue::Str("freeze".into()))])));
        assert!(bad(params(&[("timer_per_tick", real(-1.0))])));
        assert!(bad(params(&[("mute", real(1.0))])));
    }
}

mod wp6b_trigger_clear {
    use iwbtg_sim::builtins;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{Builtin, KindDef, ParamValue, Params, RoomDef};
    use iwbtg_sim::events::EventKind;
    use iwbtg_sim::num::Real;
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::world::{Ctx, SpawnInit, World};

    fn run(p: &[(&str, ParamValue)], ev: &EventKind, setup: impl FnOnce(&mut World)) -> World {
        let mut params = Params::default();
        for (k, v) in p {
            params.0.insert((*k).to_owned(), v.clone());
        }
        let mut b = GameDefBuilder::new("wp6b-clear");
        let mut k = KindDef::new("world", Builtin::World);
        k.params = params;
        let kind = b.kind(k);
        let room = b.room(RoomDef::new("r", 800, 608));
        let mut def = b.build().unwrap();
        for k in &mut def.kinds {
            k.events = builtins::event_set(k.base, &k.clone());
        }
        let mut w = World::new(room, 1);
        let rules = RunnerRules::default();
        let scripts = ScriptRuntime::default();
        let mut ctx = Ctx {
            def: &def,
            rules: &rules,
            w: &mut w,
            scripts: &scripts,
            dll_mode: DllMode::Stub,
        };
        let me = ctx
            .spawn(kind, Real::ZERO, Real::ZERO, SpawnInit::default())
            .unwrap();
        setup(ctx.w);
        builtins::on_event(&mut ctx, me, ev).unwrap();
        w
    }

    fn seed(w: &mut World) {
        for i in [0, 5, 10, 11, 12] {
            w.engine.mech.trigger_flags.insert(i);
            w.engine.mech.stop_flags.insert(i);
        }
        w.engine.mech.switches.insert(3);
    }

    #[test]
    fn room_start_clears_trigger_ids_zero_to_n_only() {
        let w = run(
            &[("clear_triggers_to", ParamValue::Real(Real::new(10.0)))],
            &EventKind::RoomStart,
            seed,
        );
        let m = &w.engine.mech;
        assert_eq!(m.trigger_flags.iter().copied().collect::<Vec<_>>(), vec![11, 12]);
        assert_eq!(m.stop_flags.len(), 5, "the stop array is not touched");
        assert!(m.switches.contains(&3));
    }

    #[test]
    fn room_start_without_the_param_clears_nothing() {
        let w = run(&[], &EventKind::RoomStart, seed);
        assert_eq!(w.engine.mech.trigger_flags.len(), 5);
    }

    #[test]
    fn room_end_clears_the_switches_when_asked() {
        let w = run(
            &[("clear_switches_on_room_end", ParamValue::Bool(true))],
            &EventKind::RoomEnd,
            seed,
        );
        assert!(w.engine.mech.switches.is_empty());
        assert_eq!(w.engine.mech.trigger_flags.len(), 5);
        let w = run(&[], &EventKind::RoomEnd, seed);
        assert!(!w.engine.mech.switches.is_empty());
    }

    #[test]
    fn the_event_set_handles_room_end_only_with_the_param() {
        let mut k = KindDef::new("w", Builtin::World);
        assert!(!builtins::event_set(Builtin::World, &k).room_end);
        k.params
            .0
            .insert("clear_switches_on_room_end".into(), ParamValue::Bool(true));
        assert!(builtins::event_set(Builtin::World, &k).room_end);
    }
}
