//! WP6a.1: rely slice, part 1: world, kid_start, block, spike, kid_bullet (docs/plans/sim-core.md
//! 6.2, 12). Hand-built worlds; inert kinds stand in for the kid and the bow.

mod wp6a {
    use iwbtg_sim::builtins;
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
        block: KindId,
    }

    /// A game with kid kinds, a 32x32 block kind and any extra kinds `extra` adds.
    fn game(
        extra: impl FnOnce(&mut GameDefBuilder, KindId) -> Vec<KindDef>,
        edit: impl FnOnce(&mut GameDef),
    ) -> G {
        let mut b = GameDefBuilder::new("wp6a");
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
        let mut bl = KindDef::new("block", Builtin::Block);
        bl.solid = true;
        bl.classes = ClassMask::BLOCK;
        bl.sprite = Some(spr32);
        let block = b.kind(bl);
        b.engine_kind(EngineKind::Kid, kid);
        b.engine_kind(EngineKind::KidFlipped, flipped);
        for kd in extra(&mut b, kid) {
            b.kind(kd);
        }
        let room = b.room(RoomDef::new("r", 800, 608));
        let mut def = b.build().unwrap();
        edit(&mut def);
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

    fn kind_of<'a>(g: &'a G, name: &str) -> &'a KindDef {
        g.def.kinds.iter().find(|k| k.name == name).unwrap()
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

    fn pos(w: &World, id: InstId) -> (f64, f64) {
        let i = w.get(id).unwrap();
        (i.m.x().get(), i.m.y().get())
    }

    // ---- block ------------------------------------------------------------------------------

    #[test]
    fn block_create_applies_overrides() {
        let g = game(|_, _| vec![], |_| {});
        let mut w = World::new(g.room, 1);
        let p = params(&[
            ("solid", ParamValue::Bool(false)),
            ("visible", ParamValue::Bool(true)),
            ("depth", real(-5.0)),
        ]);
        let id = spawn_at(&g, &mut w, g.block, 64.0, 96.0, p);
        let i = w.get(id).unwrap();
        assert!(!i.solid && i.visible);
        assert_eq!(i.depth, r(-5.0));
        // Without params the kind's defaults stay.
        let id = spawn(&g, &mut w, g.block, 0.0, 0.0);
        assert!(w.get(id).unwrap().solid);
    }

    #[test]
    fn block_visual_spawns_the_collider_at_its_position() {
        let g = game(|_, _| vec![], |_| {});
        let mut w = World::new(g.room, 1);
        let p = params(&[
            ("visual_spawns_collider", ParamValue::Bool(true)),
            ("collider", ParamValue::Kind(g.block)),
            ("visible", ParamValue::Bool(true)),
        ]);
        spawn_at(&g, &mut w, g.block, 32.0, 576.0, p);
        let blocks = alive(&w, g.block);
        assert_eq!(blocks.len(), 2, "the visual and its collider");
        assert_eq!(pos(&w, blocks[1]), (32.0, 576.0));
        assert!(w.get(blocks[1]).unwrap().solid);
    }

    #[test]
    fn block_collider_param_is_required_with_visual_spawns_collider() {
        let p = params(&[("visual_spawns_collider", ParamValue::Bool(true))]);
        assert!(builtins::init_state(Builtin::Block, &p).is_err());
        let p = params(&[("slip", ParamValue::Bool(true))]);
        assert!(
            builtins::init_state(Builtin::Block, &p).is_err(),
            "a flag is not a real"
        );
        let p = params(&[("mask", word("huge"))]);
        assert!(
            builtins::init_state(Builtin::Block, &p).is_err(),
            "not in the enum"
        );
    }

    #[test]
    fn block_slip_and_blood_sticks_are_readable() {
        let g = game(|_, _| vec![], |_| {});
        let mut w = World::new(g.room, 1);
        let ice = spawn_at(&g, &mut w, g.block, 0.0, 0.0, params(&[("slip", real(0.2))]));
        let plain = spawn_at(
            &g,
            &mut w,
            g.block,
            32.0,
            0.0,
            params(&[("blood_sticks", ParamValue::Bool(false))]),
        );
        assert_eq!(builtins::block::slip_of(&w, ice), r(0.2));
        assert_eq!(builtins::block::slip_of(&w, plain), r(0.0));
        assert!(builtins::block::blood_sticks(&w, ice));
        assert!(!builtins::block::blood_sticks(&w, plain));
    }

    // ---- spike ------------------------------------------------------------------------------

    fn spike_game() -> G {
        game(
            |_, _| {
                let mut s = KindDef::new("spike", Builtin::Spike);
                s.classes = ClassMask::KILLER;
                let mut s2 = KindDef::new("bulletspike", Builtin::Spike);
                s2.classes = ClassMask::KILLER;
                s2.params = params(&[("destroys_bullets", ParamValue::Bool(true))]);
                vec![s, s2]
            },
            |_| {},
        )
    }

    #[test]
    fn spike_auto_rotate_sets_angle_and_shift() {
        let g = spike_game();
        let mut w = World::new(g.room, 1);
        let spike = kind_id(&g, "spike");
        // kid/B.md autoSpikeSprite.
        for (dir, size, angle, dx, dy) in [
            ("up", 32.0, 0.0, 16.0, 16.0),
            ("down", 32.0, 180.0, 15.0, 15.0),
            ("left", 32.0, 90.0, 16.0, 15.0),
            ("right", 32.0, 270.0, 15.0, 16.0),
            ("up", 16.0, 0.0, 8.0, 8.0),
            ("down", 16.0, 180.0, 7.0, 7.0),
            ("left", 16.0, 90.0, 8.0, 7.0),
            ("right", 16.0, 270.0, 7.0, 8.0),
        ] {
            let p = params(&[
                ("dir", word(dir)),
                ("size", real(size)),
                ("auto_rotate", ParamValue::Bool(true)),
            ]);
            let id = spawn_at(&g, &mut w, spike, 100.0, 200.0, p);
            assert_eq!(pos(&w, id), (100.0 + dx, 200.0 + dy), "{dir} {size}");
            assert_eq!(w.get(id).unwrap().img.angle(), r(angle), "{dir} {size}");
        }
        // Without auto_rotate nothing moves.
        let id = spawn_at(&g, &mut w, spike, 5.0, 6.0, params(&[("dir", word("up"))]));
        assert_eq!(pos(&w, id), (5.0, 6.0));
        // auto_rotate needs dir and a known size.
        let p = params(&[("auto_rotate", ParamValue::Bool(true))]);
        assert!(builtins::init_state(Builtin::Spike, &p).is_err());
        let p = params(&[
            ("auto_rotate", ParamValue::Bool(true)),
            ("dir", word("up")),
            ("size", real(24.0)),
        ]);
        assert!(builtins::init_state(Builtin::Spike, &p).is_err());
    }

    #[test]
    fn spike_overrides_visible_and_depth() {
        let g = spike_game();
        let mut w = World::new(g.room, 1);
        let p = params(&[("visible", ParamValue::Bool(false)), ("depth", real(7.0))]);
        let id = spawn_at(&g, &mut w, kind_id(&g, "spike"), 0.0, 0.0, p);
        assert!(!w.get(id).unwrap().visible);
        assert_eq!(w.get(id).unwrap().depth, r(7.0));
    }

    #[test]
    fn spike_destroys_bullets_only_when_asked() {
        let g = spike_game();
        let set = |name: &str| builtins::event_set(Builtin::Spike, kind_of(&g, name));
        assert!(set("spike").collisions.is_empty());
        assert_eq!(set("bulletspike").collisions.as_slice(), &[ClassMask::KID_BULLET]);
        let mut w = World::new(g.room, 1);
        let spike = spawn(&g, &mut w, kind_id(&g, "bulletspike"), 0.0, 0.0);
        let victim = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        fire(
            &g,
            &mut w,
            spike,
            &EventKind::Collision {
                with: ClassMask::KID_BULLET,
                other: victim,
            },
        );
        assert!(!w.get(victim).unwrap().alive);
    }

    // ---- kid_start --------------------------------------------------------------------------

    fn start_game_with(params_: Params) -> (G, World, InstId) {
        let g = game(
            |_, _| {
                let mut k = KindDef::new("start", Builtin::KidStart);
                k.params = params_;
                vec![k]
            },
            |_| {},
        );
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "start"), 32.0, 544.0);
        (g, w, id)
    }

    #[test]
    fn kid_start_moves_itself_and_creates_the_kid_at_room_start() {
        let (g, mut w, start) = start_game_with(Params::default());
        // Create added (17, 23): rely room1's start (32, 544) gives the kid (49, 567).
        assert_eq!(pos(&w, start), (49.0, 567.0));
        fire(&g, &mut w, start, &EventKind::RoomStart);
        let kids = alive(&w, g.kid);
        assert_eq!(kids.len(), 1);
        assert_eq!(pos(&w, kids[0]), (49.0, 567.0));
        // A second start finds the kid and does nothing.
        fire(&g, &mut w, start, &EventKind::RoomStart);
        assert_eq!(alive(&w, g.kid).len(), 1);
    }

    #[test]
    fn kid_start_with_grav_creates_nothing_when_gated() {
        let (g, mut w, start) = start_game_with(Params::default());
        w.engine.kid.grav = true;
        fire(&g, &mut w, start, &EventKind::RoomStart);
        assert!(alive(&w, g.kid).is_empty() && alive(&w, g.flipped).is_empty());
        // Ungated games (RORI, showdown) create the normal kid anyway.
        let (g, mut w, start) = start_game_with(params(&[("grav_gated", ParamValue::Bool(false))]));
        w.engine.kid.grav = true;
        fire(&g, &mut w, start, &EventKind::RoomStart);
        assert_eq!(alive(&w, g.kid).len(), 1);
    }

    #[test]
    fn kid_start_player_or_player2_shifts_in_room_start() {
        let p = params(&[
            ("creates", word("player_or_player2")),
            ("shift_at", word("room_start")),
        ]);
        let (g, mut w, start) = start_game_with(p.clone());
        assert_eq!(pos(&w, start), (32.0, 544.0), "Create does not shift");
        fire(&g, &mut w, start, &EventKind::RoomStart);
        assert_eq!(pos(&w, alive(&w, g.kid)[0]), (49.0, 567.0));
        let (g, mut w, start) = start_game_with(p);
        w.engine.kid.grav = true;
        fire(&g, &mut w, start, &EventKind::RoomStart);
        let flipped = alive(&w, g.flipped);
        assert_eq!(flipped.len(), 1);
        assert_eq!(
            pos(&w, flipped[0]),
            (55.0, 561.0),
            "animus shifts by (23, 17) with grav 1"
        );
        // The gravity-dependent shift only works in Room Start.
        let bad = params(&[("creates", word("player_or_player2"))]);
        assert!(builtins::init_state(Builtin::KidStart, &bad).is_err());
    }

    #[test]
    fn kid_start_offset_is_a_param() {
        let (g, mut w, start) = start_game_with(params(&[("offset_x", real(16.0))]));
        fire(&g, &mut w, start, &EventKind::RoomStart);
        assert_eq!(
            pos(&w, alive(&w, g.kid)[0]),
            (48.0, 567.0),
            "machinary shifts by (16, 23)"
        );
    }

    // ---- world ------------------------------------------------------------------------------

    fn world_game(p: Params, edit: impl FnOnce(&mut GameDef)) -> (G, World, InstId) {
        let g = game(
            |_, _| {
                let mut k = KindDef::new("world", Builtin::World);
                k.persistent = true;
                k.params = p;
                vec![k]
            },
            edit,
        );
        let mut w = World::new(g.room, 1);
        let id = spawn(&g, &mut w, kind_id(&g, "world"), 0.0, 0.0);
        (g, w, id)
    }

    fn frame(w: &mut World, held: &[Action], pressed: &[Action]) {
        let bits = |a: &[Action]| a.iter().fold(0u32, |acc, x| acc | x.bit());
        w.input.latch(InputFrame {
            held: bits(held),
            pressed: bits(pressed),
            released: 0,
        });
    }

    #[test]
    fn world_create_and_room_start() {
        let (g, mut w, id) = world_game(Params::default(), |_| {});
        assert_eq!(w.engine.run.slot, 1);
        w.room_speed = r(30.0);
        fire(&g, &mut w, id, &EventKind::RoomStart);
        assert_eq!(
            w.room_speed,
            r(50.0),
            "world's Room Start sets 50 after the file's 30"
        );
    }

    #[test]
    fn world_key_press_runs_the_retry_when_the_model_says_so() {
        let (g, mut w, id) = world_game(Params::default(), |_| {});
        let flow = fire(&g, &mut w, id, &EventKind::KeyPress(Action::Restart));
        assert_eq!(
            (flow, w.pending),
            (Flow::AbortTick, Some(RoomRequest::GameRestart))
        );
        w.pending = None;
        let flow = fire(&g, &mut w, id, &EventKind::KeyPress(Action::Reset));
        assert_eq!(
            (flow, w.pending),
            (Flow::AbortTick, Some(RoomRequest::GameRestart))
        );
        // A game whose retry runs in Step ignores the key press event.
        let (g, mut w, id) = world_game(Params::default(), |d| {
            d.restart.trigger = RestartTrigger::StepHeld
        });
        let flow = fire(&g, &mut w, id, &EventKind::KeyPress(Action::Restart));
        assert_eq!((flow, w.pending), (Flow::Continue, None));
    }

    #[test]
    fn world_step_runs_held_and_pressed_triggers() {
        let (g, mut w, id) = world_game(Params::default(), |d| {
            d.restart.trigger = RestartTrigger::StepHeld
        });
        frame(&mut w, &[], &[]);
        assert_eq!(fire(&g, &mut w, id, &EventKind::Step), Flow::Continue);
        frame(&mut w, &[Action::Restart], &[]);
        assert_eq!(
            fire(&g, &mut w, id, &EventKind::Step),
            Flow::AbortTick,
            "held, not pressed"
        );
        assert_eq!(w.pending, Some(RoomRequest::GameRestart));

        let (g, mut w, id) = world_game(Params::default(), |d| {
            d.restart.trigger = RestartTrigger::StepPressed {
                skip_while_paused: false,
            }
        });
        frame(&mut w, &[Action::Restart], &[]);
        assert_eq!(
            fire(&g, &mut w, id, &EventKind::Step),
            Flow::Continue,
            "held is not pressed"
        );
        frame(&mut w, &[Action::Restart], &[Action::Restart]);
        assert_eq!(fire(&g, &mut w, id, &EventKind::Step), Flow::AbortTick);
    }

    #[test]
    fn world_step_counts_down_the_boot_load() {
        let (g, mut w, id) = world_game(Params::default(), |_| {});
        assert_eq!(
            fire(&g, &mut w, id, &EventKind::Step),
            Flow::Continue,
            "nothing waiting"
        );
        w.save = Some(iwbtg_sim::save::SaveSlot {
            room: g.room,
            x: r(49.0),
            y: r(567.0),
            flipped: false,
            facing: 1,
            difficulty: 0,
            deaths: 0,
            time: 0,
            bosses: 0,
            items: 0,
            clear: false,
        });
        w.engine.run.boot_load = Some(iwbtg_sim::run_globals::BootLoad { ticks_left: 1 });
        frame(&mut w, &[], &[]);
        assert_eq!(fire(&g, &mut w, id, &EventKind::Step), Flow::AbortTick);
        assert_eq!(w.pending, Some(RoomRequest::Goto(g.room)));
        assert_eq!(pos(&w, alive(&w, g.kid)[0]), (49.0, 567.0));
    }

    #[test]
    fn world_latch_clearing_consumes_the_tap() {
        let tap = |w: &mut World| {
            // Right pressed and released inside one tick: latched, not held.
            w.input.latch(InputFrame {
                held: 0,
                pressed: Action::Right.bit(),
                released: Action::Right.bit(),
            });
        };
        // Without the param the latch survives the world's Step: the kid's later read sees it.
        let (g, mut w, id) = world_game(Params::default(), |_| {});
        tap(&mut w);
        fire(&g, &mut w, id, &EventKind::Step);
        assert!(w.input.direct(Action::Right));
        // With it the world's two reads eat it (kaenbyou, eat-the-lemon, xia).
        let p = params(&[("latch_cleared_by_world", ParamValue::Bool(true))]);
        let (g, mut w, id) = world_game(p, |_| {});
        tap(&mut w);
        fire(&g, &mut w, id, &EventKind::Step);
        assert!(!w.input.direct(Action::Right));
    }

    // ---- kid_bullet -------------------------------------------------------------------------

    fn bullet_game(check: Option<&str>) -> G {
        game(
            |b, _| {
                let spr = b.sprite_rect((4, 4), (0, 0));
                let mut k = KindDef::new("bullet", Builtin::KidBullet);
                k.classes = ClassMask::KID_BULLET;
                k.sprite = Some(spr);
                if let Some(c) = check {
                    k.params = params(&[("block_check", word(c))]);
                }
                let spr32 = b.sprite_rect((32, 32), (0, 0));
                let mut plat = KindDef::new("plat", Builtin::Inert);
                plat.solid = false;
                plat.classes = ClassMask::BLOCK | ClassMask::PLATFORM;
                plat.sprite = Some(spr32);
                plat.visible = false;
                vec![k, plat]
            },
            |_| {},
        )
    }

    /// A bullet at (x, y) moving `h` a tick, as the shooter leaves it after Create.
    fn shoot(g: &G, w: &mut World, x: f64, y: f64, h: f64) -> InstId {
        let id = spawn(g, w, kind_id(g, "bullet"), x, y);
        w.get_mut(id).unwrap().m.set_hspeed(r(h)).unwrap();
        id
    }

    #[test]
    fn bullet_create_arms_alarm_zero_and_alarm_destroys() {
        let g = bullet_game(None);
        let mut w = World::new(g.room, 1);
        let b = shoot(&g, &mut w, 100.0, 100.0, 16.0);
        assert_eq!(w.get(b).unwrap().alarms[0], r(40.0));
        fire(&g, &mut w, b, &EventKind::Alarm(0));
        assert!(!w.get(b).unwrap().alive);
        // `life` is a param.
        let g = game(
            |_, _| {
                let mut k = KindDef::new("bullet", Builtin::KidBullet);
                k.params = params(&[("life", real(25.0))]);
                vec![k]
            },
            |_| {},
        );
        let mut w = World::new(g.room, 1);
        let b = spawn(&g, &mut w, kind_id(&g, "bullet"), 0.0, 0.0);
        assert_eq!(w.get(b).unwrap().alarms[0], r(25.0));
    }

    #[test]
    fn bullet_before_move_dies_at_a_block_one_move_ahead() {
        let g = bullet_game(None);
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.block, 64.0, 0.0);
        // The bullet is 4x4 at x 40: x + 16 = 56 reaches 59, the block starts at 64: free.
        let free = shoot(&g, &mut w, 40.0, 10.0, 16.0);
        fire(&g, &mut w, free, &EventKind::Step);
        assert!(w.get(free).unwrap().alive);
        // At x 50 the next move reaches 66: blocked, destroyed before it moves.
        let hit = shoot(&g, &mut w, 50.0, 10.0, 16.0);
        fire(&g, &mut w, hit, &EventKind::Step);
        assert!(!w.get(hit).unwrap().alive);
        assert_eq!(pos(&w, hit).0, 50.0, "it never moved");
    }

    #[test]
    fn bullet_before_move_passes_invisible_platforms_but_not_visible_ones() {
        let g = bullet_game(None);
        let mut w = World::new(g.room, 1);
        let plat = spawn(&g, &mut w, kind_id(&g, "plat"), 64.0, 0.0);
        let b = shoot(&g, &mut w, 50.0, 10.0, 16.0);
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(
            w.get(b).unwrap().alive,
            "an invisible platform does not stop bullets"
        );
        w.get_mut(plat).unwrap().visible = true;
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(!w.get(b).unwrap().alive, "a visible one does");
    }

    #[test]
    fn bullet_collision_destroy_declares_the_block_collision_only() {
        let g = bullet_game(Some("collision_destroy"));
        let k = builtins::event_set(Builtin::KidBullet, kind_of(&g, "bullet"));
        assert_eq!(k.collisions.as_slice(), &[ClassMask::BLOCK]);
        assert!(!k.step, "no Step check in this mode");
        let mut w = World::new(g.room, 1);
        let blk = spawn(&g, &mut w, g.block, 64.0, 0.0);
        let b = shoot(&g, &mut w, 50.0, 10.0, 16.0);
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(w.get(b).unwrap().alive, "Step does not look ahead");
        fire(
            &g,
            &mut w,
            b,
            &EventKind::Collision {
                with: ClassMask::BLOCK,
                other: blk,
            },
        );
        assert!(!w.get(b).unwrap().alive);
        // The default mode declares a Step and no collision.
        let g = bullet_game(None);
        let k = builtins::event_set(Builtin::KidBullet, kind_of(&g, "bullet"));
        assert!(k.step && k.collisions.is_empty());
        assert_eq!(k.alarms, 1);
    }

    #[test]
    fn bullet_stop_and_die_moves_to_contact_then_dies_next_step() {
        let g = bullet_game(Some("stop_and_die"));
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.block, 64.0, 0.0);
        let b = shoot(&g, &mut w, 50.0, 10.0, 16.0);
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(w.get(b).unwrap().alive, "it stops, it does not die yet");
        let i = w.get(b).unwrap();
        assert_eq!(i.m.hspeed(), r(0.0));
        assert_eq!(
            i.m.x(),
            r(60.0),
            "a 4 px bullet at x 60 covers 60..63, next to the block at 64"
        );
        fire(&g, &mut w, b, &EventKind::Step);
        assert!(!w.get(b).unwrap().alive);
    }
}
