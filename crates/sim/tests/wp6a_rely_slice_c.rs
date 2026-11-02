//! WP6a.3: rely slice, part 3: blood_emitter, blood, game_over, bow (docs/plans/sim-core.md 6.2,
//! 12). Hand-built worlds; inert kinds stand in for the kid.

mod wp6a {
    use iwbtg_sim::builtins::{self, BuiltinOutput, game_over};
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{
        Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, RoomId,
        RoomRole,
    };
    use iwbtg_sim::events::{EventKind, ProbeAct, ProbeOn, ProbeState, ProbeStep, probe_event_set};
    use iwbtg_sim::input::{Action, InputFrame};
    use iwbtg_sim::num::Real;
    use iwbtg_sim::output::OutputEvent;
    use iwbtg_sim::rng::Rng;
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::tick;
    use iwbtg_sim::world::{Ctx, Flow, InstId, KindState, SpawnInit, World};

    /// The engine hooks without the sound model (WP10) and timing triggers (WP8b), which are
    /// not implemented yet.
    const HOOKS: tick::Hooks = tick::Hooks {
        sound_tick: |_| {},
        timing: |_, _| Ok(Vec::new()),
        ..tick::Hooks::ENGINE
    };

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
        select: RoomId,
        kid: KindId,
        blood: KindId,
        emitter: KindId,
        block: KindId,
    }

    /// A game with a kid, blood, an emitter taking `emitter_params`, a block, and `extra` kinds.
    fn game(emitter_params: Params, extra: impl FnOnce() -> Vec<KindDef>) -> G {
        let mut b = GameDefBuilder::new("wp6a-c");
        let spr32 = b.sprite_rect((32, 32), (0, 0));
        let spr4 = b.sprite_rect((4, 4), (0, 0));
        let mut k = KindDef::new("kid", Builtin::Inert);
        k.classes = ClassMask::KID;
        k.sprite = Some(spr4);
        let kid = b.kind(k);
        let mut bl = KindDef::new("blood", Builtin::Blood);
        bl.sprite = Some(spr4);
        let blood = b.kind(bl);
        let mut em = KindDef::new("emitter", Builtin::BloodEmitter);
        em.params = emitter_params;
        let emitter = b.kind(em);
        let mut blk = KindDef::new("block", Builtin::Block);
        blk.solid = true;
        blk.classes = ClassMask::BLOCK;
        blk.sprite = Some(spr32);
        let block = b.kind(blk);
        b.engine_kind(EngineKind::Kid, kid);
        b.engine_kind(EngineKind::Blood, blood);
        b.engine_kind(EngineKind::BloodEmitter, emitter);
        for kd in extra() {
            b.kind(kd);
        }
        let room = b.room(RoomDef::new("r", 800, 608));
        let mut rs = RoomDef::new("sel", 800, 608);
        rs.role = RoomRole::Select;
        let select = b.room(rs);
        let mut def = b.build().unwrap();
        // The loader builds each kind's event set; do the same here.
        for k in &mut def.kinds {
            k.events = builtins::event_set(k.base, &k.clone());
        }
        G {
            def,
            room,
            select,
            kid,
            blood,
            emitter,
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

    fn alive(w: &World, kind: KindId) -> Vec<InstId> {
        w.iter_alive().filter(|i| i.kind == kind).map(|i| i.id).collect()
    }

    // ---- blood ------------------------------------------------------------------------------

    #[test]
    fn blood_create_draws_the_frame_then_the_gravity() {
        let g = game(Params::default(), Vec::new);
        let mut w = World::new(g.room, 7);
        let mut mirror = Rng::new(7);
        let frame = mirror.choose(3).unwrap();
        let gravity = r(0.1) + mirror.random(r(0.2));
        let id = spawn(&g, &mut w, g.blood, 10.0, 10.0);
        let i = w.get(id).unwrap();
        assert_eq!(i.img.index(), r(f64::from(frame)));
        assert_eq!(i.img.speed(), r(0.0), "blood never animates");
        assert_eq!(i.m.gravity(), gravity);
        assert_eq!(w.rng, mirror, "exactly two draws");
    }

    #[test]
    fn blood_flipped_has_negative_gravity() {
        let g = game(Params::default(), Vec::new);
        let mut w = World::new(g.room, 7);
        let mut mirror = Rng::new(7);
        mirror.choose(3).unwrap();
        let gravity = r(-0.1) + mirror.random(r(-0.2));
        let id = spawn_at(&g, &mut w, g.blood, 0.0, 0.0, params(&[("flipped", yes())]));
        assert_eq!(w.get(id).unwrap().m.gravity(), gravity);
        assert!(gravity.get() < -0.1 && gravity.get() > -0.3);
    }

    #[test]
    fn blood_is_destroyed_outside_the_room() {
        let g = game(Params::default(), Vec::new);
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, g.blood, 10.0, 10.0);
        fire(&g, &mut w, id, &EventKind::OutsideRoom);
        assert!(!w.get(id).unwrap().alive);
    }

    #[test]
    fn blood_sticks_to_a_block_at_contact() {
        let g = game(Params::default(), Vec::new);
        let mut w = World::new(g.room, 1);
        let blk = spawn(&g, &mut w, g.block, 100.0, 0.0);
        // Moving right at 6 a tick: it was at x 90 (4 px box, reaches 93) and is now at 96,
        // inside the block at 100.
        let id = spawn(&g, &mut w, g.blood, 90.0, 10.0);
        {
            let i = w.get_mut(id).unwrap();
            i.m.motion_set(r(0.0), r(6.0)).unwrap();
            i.m.capture_previous();
            i.m.set_x(r(96.0)).unwrap();
        }
        fire(
            &g,
            &mut w,
            id,
            &EventKind::Collision {
                with: ClassMask::BLOCK,
                other: blk,
            },
        );
        let i = w.get(id).unwrap();
        assert_eq!(
            (i.m.x(), i.m.hspeed(), i.m.vspeed(), i.m.gravity()),
            (r(96.0), r(0.0), r(0.0), r(0.0))
        );
        // A block with blood_sticks = false does not stop it.
        let slick = spawn_at(
            &g,
            &mut w,
            g.block,
            200.0,
            0.0,
            params(&[("blood_sticks", ParamValue::Bool(false))]),
        );
        let id = spawn(&g, &mut w, g.blood, 190.0, 10.0);
        w.get_mut(id).unwrap().m.motion_set(r(0.0), r(6.0)).unwrap();
        fire(
            &g,
            &mut w,
            id,
            &EventKind::Collision {
                with: ClassMask::BLOCK,
                other: slick,
            },
        );
        assert_eq!(w.get(id).unwrap().m.hspeed(), r(6.0));
    }

    // ---- blood_emitter ----------------------------------------------------------------------

    #[test]
    fn emitter_draw_order_is_frame_gravity_direction_speed_per_drop() {
        let g = game(params(&[("per_step", real(3.0))]), Vec::new);
        let mut w = World::new(g.room, 99);
        let em = spawn(&g, &mut w, g.emitter, 350.0, 66.4);
        assert_eq!(w.get(em).unwrap().alarms[0], r(20.0));
        let mut mirror = Rng::new(99);
        let mut expected = Vec::new();
        for _ in 0..3 {
            let frame = mirror.choose(3).unwrap();
            let gravity = r(0.1) + mirror.random(r(0.2));
            let dir = mirror.irandom(r(35.0)).unwrap() * r(10.0);
            let speed = mirror.random(r(6.0));
            expected.push((frame, gravity, dir, speed));
        }
        fire(&g, &mut w, em, &EventKind::Step);
        let drops = alive(&w, g.blood);
        assert_eq!(drops.len(), 3);
        for (id, (frame, gravity, dir, speed)) in drops.into_iter().zip(expected) {
            let i = w.get(id).unwrap();
            assert_eq!((i.m.x(), i.m.y()), (r(350.0), r(66.4)));
            assert_eq!(i.img.index(), r(f64::from(frame)));
            assert_eq!(i.m.gravity(), gravity);
            assert_eq!((i.m.direction(), i.m.speed()), (dir, speed));
        }
        assert_eq!(w.rng, mirror, "3 drops use 4 draws each");
        // rely draws 40 * 4 = 160 here (and 2 more from the runner's own particles, not modeled).
        let g = game(Params::default(), Vec::new);
        let mut w = World::new(g.room, 99);
        let em = spawn(&g, &mut w, g.emitter, 0.0, 0.0);
        fire(&g, &mut w, em, &EventKind::Step);
        assert_eq!(alive(&w, g.blood).len(), 40);
        let mut m = Rng::new(99);
        for _ in 0..160 {
            m.next_unit();
        }
        assert_eq!(w.rng, m);
    }

    fn count_after_ticks(g: &G, w: &mut World, ticks: u32) -> usize {
        for _ in 0..ticks {
            let rules = RunnerRules::default();
            let scripts = ScriptRuntime::default();
            let mut ctx = Ctx {
                def: &g.def,
                rules: &rules,
                w,
                scripts: &scripts,
                dll_mode: DllMode::Stub,
            };
            tick::run_tick_with(&mut ctx, InputFrame::EMPTY, &HOOKS).unwrap();
        }
        alive(w, g.blood).len()
    }

    #[test]
    fn emitter_runs_19_steps_from_a_collision_phase_and_20_from_a_key_press() {
        let em_params = params(&[("per_step", real(1.0))]);
        // Made between ticks (as in a collision event of the previous tick): its Step starts
        // next tick, after the alarm, so it gets 19 Steps.
        let g = game(em_params.clone(), Vec::new);
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.emitter, 400.0, 300.0);
        assert_eq!(count_after_ticks(&g, &mut w, 30), 19);
        assert!(alive(&w, g.emitter).is_empty());
        // Made in a Key press event: it runs the creation tick's Step too: 20.
        let g = game(em_params, || {
            let mut k = KindDef::new("killer", Builtin::Probe);
            k.events = probe_event_set();
            vec![k]
        });
        let mut w = World::new(g.room, 1);
        let killer = spawn(&g, &mut w, kind_id(&g, "killer"), 0.0, 0.0);
        w.get_mut(killer).unwrap().state = KindState::Probe(ProbeState::new("K", None).with(ProbeStep::on(
            ProbeOn::KeyPress,
            ProbeAct::Spawn {
                kind: g.emitter,
                x: r(400.0),
                y: r(300.0),
                child: None,
            },
        )));
        let rules = RunnerRules::default();
        let scripts = ScriptRuntime::default();
        {
            let mut ctx = Ctx {
                def: &g.def,
                rules: &rules,
                w: &mut w,
                scripts: &scripts,
                dll_mode: DllMode::Stub,
            };
            let bit = Action::Suicide.bit();
            tick::run_tick_with(
                &mut ctx,
                InputFrame {
                    held: bit,
                    pressed: bit,
                    released: bit,
                },
                &HOOKS,
            )
            .unwrap();
        }
        assert_eq!(
            alive(&w, g.blood).len(),
            1,
            "the creation tick already ran a Step"
        );
        assert_eq!(count_after_ticks(&g, &mut w, 30), 20);
    }

    #[test]
    fn emitter_angle_sweep_uses_the_chance_per_angle() {
        let sweep = |chance: f64| {
            params(&[
                ("mode", word("angle_sweep")),
                ("chance", real(chance)),
                ("per_angle", real(2.0)),
                ("speed_max", real(15.0)),
            ])
        };
        let g = game(sweep(1.0), Vec::new);
        let mut w = World::new(g.room, 3);
        let em = spawn(&g, &mut w, g.emitter, 0.0, 0.0);
        fire(&g, &mut w, em, &EventKind::Step);
        assert_eq!(alive(&w, g.blood).len(), 37 * 2, "angles 0, 10, ..., 360");
        let dirs: Vec<f64> = alive(&w, g.blood)
            .iter()
            .map(|id| w.get(*id).unwrap().m.direction().get())
            .collect();
        assert!(dirs.contains(&0.0) && dirs.iter().all(|d| *d < 360.0 + 1e-9));
        let g = game(sweep(0.0), Vec::new);
        let mut w = World::new(g.room, 3);
        let em = spawn(&g, &mut w, g.emitter, 0.0, 0.0);
        fire(&g, &mut w, em, &EventKind::Step);
        assert!(alive(&w, g.blood).is_empty());
    }

    #[test]
    fn emitter_particles_go_to_the_client_and_draw_nothing() {
        let g = game(
            params(&[("mode", word("particles")), ("flipped", yes())]),
            Vec::new,
        );
        let mut w = World::new(g.room, 5);
        let em = spawn(&g, &mut w, g.emitter, 49.0, 519.1);
        fire(&g, &mut w, em, &EventKind::Step);
        assert!(alive(&w, g.blood).is_empty());
        assert_eq!(w.rng, Rng::new(5), "no draws (N10 placeholder)");
        assert!(
            w.output
                .contains(&OutputEvent::Builtin(BuiltinOutput::DeathParticles {
                    x: r(49.0),
                    y: r(519.1),
                    count: 15,
                    flipped: true
                }))
        );
        // With the switch on the draw count is UNKNOWN: an error, not a guess.
        let mut rules = RunnerRules::default();
        rules.num.particles_draw_rng = true;
        let scripts = ScriptRuntime::default();
        let mut ctx = Ctx {
            def: &g.def,
            rules: &rules,
            w: &mut w,
            scripts: &scripts,
            dll_mode: DllMode::Stub,
        };
        assert!(builtins::on_event(&mut ctx, em, &EventKind::Step).is_err());
    }

    #[test]
    fn emitter_restart_key_destroys_it_only_when_asked() {
        let g = game(params(&[("destroyed_by_restart", yes())]), Vec::new);
        let set = builtins::event_set(
            Builtin::BloodEmitter,
            &g.def.kinds[usize::try_from(g.emitter.0).unwrap()],
        );
        assert_eq!(set.key_press, Action::Restart.bit());
        let mut w = World::new(g.room, 1);
        let em = spawn(&g, &mut w, g.emitter, 0.0, 0.0);
        fire(&g, &mut w, em, &EventKind::KeyPress(Action::Restart));
        assert!(!w.get(em).unwrap().alive);
        let g = game(Params::default(), Vec::new);
        let set = builtins::event_set(
            Builtin::BloodEmitter,
            &g.def.kinds[usize::try_from(g.emitter.0).unwrap()],
        );
        assert_eq!(set.key_press, 0);
    }

    #[test]
    fn emitter_alarm_destroys_it() {
        let g = game(Params::default(), Vec::new);
        let mut w = World::new(g.room, 1);
        let em = spawn(&g, &mut w, g.emitter, 0.0, 0.0);
        fire(&g, &mut w, em, &EventKind::Alarm(0));
        assert!(!w.get(em).unwrap().alive);
    }

    // ---- game_over --------------------------------------------------------------------------

    fn over_game(p: Params) -> G {
        game(Params::default(), || {
            let mut k = KindDef::new("over", Builtin::GameOver);
            k.params = p;
            vec![k]
        })
    }

    #[test]
    fn game_over_is_placed_relative_to_view_zero() {
        let g = over_game(Params::default());
        let mut w = World::new(g.room, 1);
        w.views[0].x = r(800.0);
        w.views[0].y = r(608.0);
        let id = spawn(&g, &mut w, kind_id(&g, "over"), 5.0, 5.0);
        let i = w.get(id).unwrap();
        assert_eq!((i.m.x(), i.m.y()), (r(1200.0), r(912.0)), "view + (400, 304)");
        assert_eq!(i.img.alpha(), r(0.0));
        assert!(i.visible, "no delay: visible at once");
        let g = over_game(params(&[("offset_x", real(480.0)), ("offset_y", real(270.0))]));
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "over"), 5.0, 5.0);
        assert_eq!(
            (w.get(id).unwrap().m.x(), w.get(id).unwrap().m.y()),
            (r(480.0), r(270.0))
        );
        let g = over_game(params(&[("view_relative", ParamValue::Bool(false))]));
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "over"), 5.0, 6.0);
        assert_eq!(
            (w.get(id).unwrap().m.x(), w.get(id).unwrap().m.y()),
            (r(5.0), r(6.0))
        );
    }

    #[test]
    fn game_over_delay_hides_it_until_alarm_zero() {
        let g = over_game(params(&[("delay", real(30.0))]));
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "over"), 0.0, 0.0);
        assert!(!w.get(id).unwrap().visible);
        assert_eq!(w.get(id).unwrap().alarms[0], r(30.0));
        fire(&g, &mut w, id, &EventKind::Alarm(0));
        assert!(w.get(id).unwrap().visible);
    }

    #[test]
    fn game_over_fades_in_the_draw_slot() {
        let g = over_game(Params::default());
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "over"), 0.0, 0.0);
        for _ in 0..10 {
            fire(&g, &mut w, id, &EventKind::Late);
        }
        let a = w.get(id).unwrap().img.alpha().get();
        assert!((a - 0.1).abs() < 1e-12, "alpha rises 0.01 a tick: {a}");
        assert!(
            (game_over::overlay_alpha(&w, id).get() - 0.15).abs() < 1e-12,
            "overlay 0.015 a tick"
        );
        for _ in 0..200 {
            fire(&g, &mut w, id, &EventKind::Late);
        }
        assert_eq!(w.get(id).unwrap().img.alpha(), r(1.0));
        assert_eq!(
            game_over::overlay_alpha(&w, id),
            r(0.6),
            "the overlay stops at 0.6"
        );
        // The bob is 32 * sin(phase), phase += pi/100: 210 ticks is 2.1 pi.
        let want = 32.0 * iwbtg_sim::num::sin(r(210.0 * std::f64::consts::PI / 100.0)).get();
        assert!((game_over::bob_offset(&w, id).get() - want).abs() < 1e-9);
        let set = builtins::event_set(
            Builtin::GameOver,
            &g.def.kinds[usize::try_from(kind_id(&g, "over").0).unwrap()],
        );
        assert!(set.late);
    }

    // ---- bow --------------------------------------------------------------------------------

    fn bow_game(p: Params) -> G {
        game(Params::default(), || {
            let mut k = KindDef::new("bow", Builtin::Bow);
            k.persistent = true;
            k.params = p;
            vec![k]
        })
    }

    #[test]
    fn bow_copies_the_kid_in_its_follow_phase_only() {
        let g = bow_game(Params::default());
        let mut w = World::new(g.room, 1);
        let kid = spawn(&g, &mut w, g.kid, 120.0, 340.0);
        {
            let i = w.get_mut(kid).unwrap();
            i.img.set_xscale(r(-1.0)).unwrap();
            i.img.set_alpha(r(0.5)).unwrap();
        }
        let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 0.0, 0.0);
        fire(&g, &mut w, bow, &EventKind::BeginStep);
        assert_eq!(w.get(bow).unwrap().m.x(), r(0.0), "not its phase");
        fire(&g, &mut w, bow, &EventKind::Step);
        let i = w.get(bow).unwrap();
        assert_eq!(
            (i.m.x(), i.m.y(), i.img.xscale(), i.img.alpha()),
            (r(120.0), r(340.0), r(-1.0), r(0.5))
        );
        // begin_step games (wanna): the copy is in Begin Step.
        let g = bow_game(params(&[("follow", word("begin_step"))]));
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.kid, 7.0, 8.0);
        let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 0.0, 0.0);
        fire(&g, &mut w, bow, &EventKind::Step);
        assert_eq!(w.get(bow).unwrap().m.x(), r(0.0));
        fire(&g, &mut w, bow, &EventKind::BeginStep);
        assert_eq!(w.get(bow).unwrap().m.x(), r(7.0));
    }

    #[test]
    fn bow_keeps_its_place_without_a_kid() {
        let g = bow_game(Params::default());
        let mut w = World::new(g.room, 1);
        let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 11.0, 12.0);
        fire(&g, &mut w, bow, &EventKind::Step);
        let i = w.get(bow).unwrap();
        assert!(i.alive);
        assert_eq!((i.m.x(), i.m.y()), (r(11.0), r(12.0)));
    }

    #[test]
    fn bow_is_destroyed_above_difficulty_zero_and_in_select_rooms() {
        let g = bow_game(Params::default());
        let mut w = World::new(g.room, 1);
        w.engine.run.difficulty = 1;
        let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 0.0, 0.0);
        fire(&g, &mut w, bow, &EventKind::Step);
        assert!(!w.get(bow).unwrap().alive);
        let mut w = World::new(g.select, 1);
        let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 0.0, 0.0);
        fire(&g, &mut w, bow, &EventKind::Step);
        assert!(!w.get(bow).unwrap().alive, "rSelectStage");
        // Games that keep it on every difficulty say so.
        let g = bow_game(params(&[("medium_only", ParamValue::Bool(false))]));
        let mut w = World::new(g.room, 1);
        w.engine.run.difficulty = 2;
        let bow = spawn(&g, &mut w, kind_id(&g, "bow"), 0.0, 0.0);
        fire(&g, &mut w, bow, &EventKind::Step);
        assert!(w.get(bow).unwrap().alive);
        let set = builtins::event_set(
            Builtin::Bow,
            &g.def.kinds[usize::try_from(kind_id(&g, "bow").0).unwrap()],
        );
        assert!(set.begin_step && set.step && set.end_step);
    }
}
