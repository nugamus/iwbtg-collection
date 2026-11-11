//! WP4a: room entry and the room change sequence (docs/plans/sim-core.md 4.2, 12).
//!
//! The sequence runs on a recording [`Probe`] host that stands in for the WP1 event layer:
//! every kind is a `Builtin::Probe` kind, every event an instance receives is logged as
//! `"<Event> <kind name>"`, dispatch order is rank then creation (the R2 default), and a test can
//! react to an event (spawn, request a room, halt). The real host (`room::EngineHost`) calls
//! `events::dispatch`, which lands with WP1.

mod wp4a {
    use iwbtg_sim::collide::Target;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{
        Builtin, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomAction, RoomDef, RoomId,
        RoomRole, VarId, ViewDef,
    };
    use iwbtg_sim::error::SimError;
    use iwbtg_sim::events::EventKind;
    use iwbtg_sim::motion::Motion;
    use iwbtg_sim::num::Real;
    use iwbtg_sim::output::OutputEvent;
    use iwbtg_sim::room::{
        RoomHost, RoomOutput, RoomRequest, StartPoint, perform_change_with, start_game_with,
    };
    use iwbtg_sim::rules::room::{NestedRequest, RoomEndOrder, RoomStartOrder};
    use iwbtg_sim::rules::{RunnerRules, TickRules};
    use iwbtg_sim::save::SaveSlot;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::trace::{TraceTable, Value as TV};
    use iwbtg_sim::world::{Ctx, Flow, InstId, Instance, SimStatus, SpawnInit, Value, World};

    type React = Box<dyn FnMut(&mut Probe, &mut Ctx, InstId, &str, &EventKind) -> Flow>;

    /// The recording host.
    #[derive(Default)]
    struct Probe {
        log: Vec<String>,
        /// When set, events are logged with `+g` while this global is set.
        watch: Option<VarId>,
        react: Option<React>,
        /// Kind name and params of every spawn that had params.
        params: Vec<(String, Params)>,
    }

    fn event_name(ev: &EventKind) -> &'static str {
        match ev {
            EventKind::Create => "Create",
            EventKind::RoomStart => "RoomStart",
            EventKind::RoomEnd => "RoomEnd",
            EventKind::GameStart => "GameStart",
            _ => "Other",
        }
    }

    impl RoomHost for Probe {
        fn dispatch_order(
            &mut self,
            w: &World,
            _rules: &TickRules,
            filter: &dyn Fn(&Instance) -> bool,
        ) -> Vec<InstId> {
            let mut v: Vec<(u16, InstId)> = w
                .instances()
                .iter()
                .filter(|i| filter(i))
                .map(|i| (i.rank, i.id))
                .collect();
            v.sort();
            v.into_iter().map(|(_, id)| id).collect()
        }

        fn dispatch(&mut self, ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow {
            let kind = ctx
                .w
                .get(me)
                .map(|i| i.kind)
                .expect("dispatch to a stored instance");
            let name = ctx.def.kind(kind).expect("known kind").name.clone();
            let g = match self.watch {
                Some(v) if ctx.w.globals.contains_key(&v) => "+g",
                _ => "",
            };
            self.log.push(format!("{} {name}{g}", event_name(ev)));
            match self.react.take() {
                Some(mut react) => {
                    let flow = react(self, ctx, me, &name, ev);
                    self.react = Some(react);
                    flow
                }
                None => Flow::Continue,
            }
        }

        fn spawn(
            &mut self,
            ctx: &mut Ctx,
            kind: KindId,
            x: Real,
            y: Real,
            init: SpawnInit,
        ) -> Result<InstId, SimError> {
            let id = ctx.w.alloc_id()?;
            let k = ctx.def.kind(kind).expect("known kind");
            let rank = init
                .rank
                .or_else(|| ctx.def.rank_for(kind, init.variant.as_ref()))
                .expect("ranked kind");
            if !init.params.0.is_empty() {
                self.params.push((k.name.clone(), init.params.clone()));
            }
            let mut inst = Instance::new(id, kind, rank, x, y);
            inst.persistent = k.persistent;
            inst.depth = k.depth;
            inst.classes = k.classes;
            inst.name = init.name;
            inst.group = init.group;
            ctx.w.insert(inst)?;
            if init.run_create && self.dispatch(ctx, id, &EventKind::Create) == Flow::Halt {
                return Err(match &ctx.w.status {
                    SimStatus::Halted(e) => e.clone(),
                    _ => SimError::invariant("halt without status"),
                });
            }
            Ok(id)
        }

        fn load_save(&mut self, ctx: &mut Ctx, slot: &SaveSlot) -> Result<InstId, SimError> {
            self.log.push("Load".into());
            let kid = ctx.def.engine_kind(EngineKind::Kid).expect("kid kind");
            let id = self.spawn(ctx, kid, slot.x, slot.y, SpawnInit::default())?;
            // A load may also request the saved room, as saveExe does.
            ctx.w.pending = Some(RoomRequest::Goto(slot.room));
            Ok(id)
        }
    }

    /// A probe kind that handles Room Start and Room End.
    fn probe_kind(b: &mut GameDefBuilder, name: &str, persistent: bool) -> KindId {
        let mut k = KindDef::new(name, Builtin::Probe);
        k.persistent = persistent;
        k.events.room_start = true;
        k.events.room_end = true;
        b.kind(k)
    }

    fn room(name: &str, speed: u32) -> RoomDef {
        let mut r = RoomDef::new(name, 800, 608);
        r.file_speed = speed;
        r
    }

    fn with_ctx<T>(def: &GameDef, rules: &RunnerRules, w: &mut World, f: impl FnOnce(&mut Ctx) -> T) -> T {
        let scripts = ScriptRuntime::default();
        let mut ctx = Ctx {
            def,
            rules,
            w,
            scripts: &scripts,
            dll_mode: DllMode::Stub,
        };
        f(&mut ctx)
    }

    fn start(
        def: &GameDef,
        rules: &RunnerRules,
        host: &mut Probe,
        at: StartPoint,
    ) -> (World, Result<(), SimError>) {
        let mut w = World::new(def.start_room, 1);
        let r = with_ctx(def, rules, &mut w, |ctx| start_game_with(ctx, &at, host));
        (w, r)
    }

    fn change(
        def: &GameDef,
        rules: &RunnerRules,
        w: &mut World,
        host: &mut Probe,
        req: RoomRequest,
    ) -> Result<(), SimError> {
        with_ctx(def, rules, w, |ctx| perform_change_with(ctx, req, host))
    }

    fn ids_of(w: &World, def: &GameDef, name: &str) -> Vec<InstId> {
        w.instances()
            .iter()
            .filter(|i| i.alive && def.kind(i.kind).is_some_and(|k| k.name == name))
            .map(|i| i.id)
            .collect()
    }

    fn slot(room: RoomId) -> SaveSlot {
        SaveSlot {
            room,
            x: Real::new(49.0),
            y: Real::new(119.0),
            flipped: false,
            facing: 1,
            difficulty: 0,
            deaths: 3,
            time: 7,
            bosses: 0,
            items: 0,
            clear: false,
        }
    }

    /// The order game: kinds ranked kid, blk, world, ps, spawned, so dispatch order differs
    /// from file and creation order.
    struct Order {
        def: GameDef,
        a: RoomId,
        b: RoomId,
        g: VarId,
    }

    fn order_game() -> Order {
        let mut b = GameDefBuilder::new("order");
        let world = probe_kind(&mut b, "world", true);
        let ps = probe_kind(&mut b, "ps", false);
        let blk = probe_kind(&mut b, "blk", false);
        let spawned = probe_kind(&mut b, "spawned", false);
        let kid = probe_kind(&mut b, "kid", true);
        b.order(&[kid, blk, world, ps, spawned]);
        b.boot(world);
        b.engine_kind(EngineKind::Kid, kid);
        let g = VarId(0);
        let mut ra = room("a", 30);
        ra.on_create = vec![
            RoomAction::SetGlobal {
                var: g,
                value: Value::Real(Real::ONE),
                difficulty: None,
            },
            RoomAction::Spawn {
                kind: spawned,
                x: Real::ZERO,
                y: Real::ZERO,
                params: Default::default(),
            },
        ];
        let a = b.room(ra);
        let rb = b.room(room("b", 50));
        b.place(a, ps, 32.0, 96.0).unwrap();
        b.place(a, blk, 0.0, 0.0).unwrap();
        b.place(rb, blk, 0.0, 0.0).unwrap();
        Order {
            def: b.build().unwrap(),
            a,
            b: rb,
            g,
        }
    }

    /// playerStart: creates the kid in its Room Start when none exists.
    fn ps_spawns_kid() -> React {
        Box::new(|host, ctx, _me, name, ev| {
            if name == "ps" && *ev == EventKind::RoomStart && ids_of(ctx.w, ctx.def, "kid").is_empty() {
                let kid = ctx.def.engine_kind(EngineKind::Kid).unwrap();
                host.spawn(ctx, kid, Real::new(49.0), Real::new(119.0), SpawnInit::default())
                    .unwrap();
            }
            Flow::Continue
        })
    }

    #[test]
    fn wp4a_seven_step_order_with_probes() {
        let o = order_game();
        let rules = RunnerRules::default();
        let mut host = Probe {
            watch: Some(o.g),
            react: Some(ps_spawns_kid()),
            ..Probe::default()
        };
        let (mut w, r) = start(
            &o.def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: o.a,
                difficulty: 2,
            },
        );
        r.unwrap();
        assert_eq!(
            host.log,
            [
                // Boot instance, before any room.
                "Create world",
                // Step 4: placed instances in file order.
                "Create ps",
                "Create blk",
                // Step 5: first room only, dispatch order (rank: blk, world, ps).
                "GameStart blk",
                "GameStart world",
                "GameStart ps",
                // Step 6: creation actions; the global is set before the spawn.
                "Create spawned+g",
                // Step 7: the room's own instances in creation order, then the carried world.
                // The kid created in playerStart's Room Start gets no Room Start (R10).
                "RoomStart ps+g",
                "Create kid+g",
                "RoomStart blk+g",
                "RoomStart spawned+g",
                "RoomStart world+g",
            ]
        );
        assert_eq!(w.room, o.a);
        assert_eq!(w.room_speed, Real::new(30.0), "file speed on entry (R14)");
        assert_eq!(w.engine.run.difficulty, 2);
        let world_id = ids_of(&w, &o.def, "world");
        let kid_id = ids_of(&w, &o.def, "kid");
        assert_eq!(world_id.len(), 1);
        assert!(
            w.get(world_id[0]).unwrap().persistent,
            "boot instances are persistent"
        );

        host.log.clear();
        w.room_tick = 99;
        change(&o.def, &rules, &mut w, &mut host, RoomRequest::Goto(o.b)).unwrap();
        assert_eq!(
            host.log,
            [
                // Step 1: every instance, dispatch order (kid, blk, world, ps, spawned).
                "RoomEnd kid+g",
                "RoomEnd blk+g",
                "RoomEnd world+g",
                "RoomEnd ps+g",
                "RoomEnd spawned+g",
                // Steps 2-4: the non-persistent ones are gone without events; b's block.
                "Create blk+g",
                // No Game Start; no creation actions; Room Start: own, then carried by id.
                "RoomStart blk+g",
                "RoomStart world+g",
                "RoomStart kid+g",
            ]
        );
        assert_eq!(w.room, o.b);
        assert_eq!(w.room_tick, 0);
        assert_eq!(w.room_speed, Real::new(50.0));
        assert_eq!(ids_of(&w, &o.def, "world"), world_id, "persistent ids are kept");
        assert_eq!(ids_of(&w, &o.def, "kid"), kid_id);
        assert!(ids_of(&w, &o.def, "ps").is_empty());
        assert_eq!(w.instances().len(), 3);
        assert!(w.pending.is_none());
        assert!(w.output.contains(&OutputEvent::Room(RoomOutput::Entered {
            from: Some(o.a),
            to: o.b
        })));
        assert!(
            w.output
                .contains(&OutputEvent::Room(RoomOutput::Entered { from: None, to: o.a }))
        );
    }

    #[test]
    fn wp4a_room_start_dispatch_switch() {
        let o = order_game();
        let mut rules = RunnerRules::default();
        rules.room.room_start_order = RoomStartOrder::Dispatch;
        let mut host = Probe::default();
        let (_, r) = start(
            &o.def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: o.a,
                difficulty: 0,
            },
        );
        r.unwrap();
        let starts: Vec<&str> = host
            .log
            .iter()
            .filter(|l| l.starts_with("RoomStart"))
            .map(String::as_str)
            .collect();
        assert_eq!(
            starts,
            [
                "RoomStart blk",
                "RoomStart world",
                "RoomStart ps",
                "RoomStart spawned"
            ]
        );
    }

    #[test]
    fn wp4a_room_end_depth_switch() {
        let mut b = GameDefBuilder::new("depth");
        let deep = probe_kind(&mut b, "deep", false);
        let shallow = probe_kind(&mut b, "shallow", false);
        b.def_mut().kinds[0].depth = Real::new(-10.0);
        b.def_mut().kinds[1].depth = Real::new(100.0);
        let r0 = b.room(room("r0", 50));
        b.room(room("r1", 50));
        b.place(r0, deep, 0.0, 0.0).unwrap();
        b.place(r0, shallow, 0.0, 0.0).unwrap();
        let def = b.build().unwrap();
        for (order, want) in [
            (RoomEndOrder::Dispatch, ["RoomEnd deep", "RoomEnd shallow"]),
            (RoomEndOrder::Depth, ["RoomEnd shallow", "RoomEnd deep"]),
        ] {
            let mut rules = RunnerRules::default();
            rules.room.room_end_order = order;
            let mut host = Probe::default();
            let (mut w, r) = start(
                &def,
                &rules,
                &mut host,
                StartPoint::Room {
                    room: r0,
                    difficulty: 0,
                },
            );
            r.unwrap();
            host.log.clear();
            change(&def, &rules, &mut w, &mut host, RoomRequest::Next).unwrap();
            assert_eq!(host.log, want, "{order:?}");
        }
    }

    /// Two rooms, `p` persistent or not, a persistent `keeper` kind in boot and a persistent
    /// `sleeper` placed in the first room.
    fn two_rooms(first_persistent: bool) -> (GameDef, RoomId, RoomId) {
        let mut b = GameDefBuilder::new("persist");
        let keeper = probe_kind(&mut b, "keeper", true);
        let sleeper = probe_kind(&mut b, "sleeper", true);
        let local = probe_kind(&mut b, "local", false);
        b.boot(keeper);
        let mut r0 = room("p", 50);
        r0.persistent = first_persistent;
        let p = b.room(r0);
        let q = b.room(room("q", 60));
        b.place(p, sleeper, 0.0, 0.0).unwrap();
        b.place(p, local, 5.0, 5.0).unwrap();
        (b.build().unwrap(), p, q)
    }

    fn deactivate(w: &mut World, def: &GameDef, name: &str) -> InstId {
        let id = ids_of(w, def, name)[0];
        w.get_mut(id).unwrap().active = false;
        id
    }

    #[test]
    fn wp4a_deactivated_persistent_stays_behind() {
        let (def, p, q) = two_rooms(false);
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (mut w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: p,
                difficulty: 0,
            },
        );
        r.unwrap();
        let sleeper = deactivate(&mut w, &def, "sleeper");
        host.log.clear();
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(q)).unwrap();
        assert!(
            w.get(sleeper).is_none(),
            "a deactivated persistent instance does not move"
        );
        assert!(
            !host.log.iter().any(|l| l.contains("sleeper")),
            "inactive: no events"
        );
        assert_eq!(ids_of(&w, &def, "keeper").len(), 1);
        assert!(
            w.persistent_rooms.is_empty(),
            "a non-persistent room stores nothing"
        );
    }

    #[test]
    fn wp4a_persistent_room_restores_without_create() {
        let (def, p, q) = two_rooms(true);
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (mut w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: p,
                difficulty: 0,
            },
        );
        r.unwrap();
        let sleeper = deactivate(&mut w, &def, "sleeper");
        let local = ids_of(&w, &def, "local")[0];
        w.get_mut(local).unwrap().m = Motion::new_at(Real::new(77.0), Real::new(5.0));
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(q)).unwrap();
        assert!(w.get(sleeper).is_none() && w.get(local).is_none());
        assert_eq!(
            w.persistent_rooms.get(&p).map(Vec::len),
            Some(2),
            "left behind and stored"
        );

        host.log.clear();
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(p)).unwrap();
        assert_eq!(
            host.log,
            ["RoomEnd keeper", "RoomStart local", "RoomStart keeper"],
            "no Create on a revisit; Room Start yes; the inactive sleeper gets nothing (R18)"
        );
        let back = w.get(local).expect("restored with its id");
        assert_eq!(back.m.x(), Real::new(77.0), "restored state is kept");
        assert!(
            w.get(sleeper).is_some_and(|s| !s.active),
            "restored, still inactive"
        );
        assert!(w.persistent_rooms.is_empty());
        let ids: Vec<InstId> = w.instances().iter().map(|i| i.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "the store stays in creation order");
        assert_eq!(w.room_speed, Real::new(50.0));
    }

    #[test]
    fn wp4a_goto_current_and_restart_rebuild() {
        let (def, p, _) = two_rooms(false);
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (mut w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: p,
                difficulty: 0,
            },
        );
        r.unwrap();
        let before = ids_of(&w, &def, "local");
        for req in [RoomRequest::Restart, RoomRequest::Goto(p)] {
            host.log.clear();
            change(&def, &rules, &mut w, &mut host, req).unwrap();
            assert!(host.log.contains(&"Create local".to_string()), "{req:?} rebuilds");
            let now = ids_of(&w, &def, "local");
            assert_eq!(now.len(), 1);
            assert_ne!(now, before, "{req:?} makes a fresh instance");
            assert_eq!(w.room, p);
        }
    }

    #[test]
    fn wp4a_restart_of_persistent_room_is_unsupported() {
        let (def, p, _) = two_rooms(true);
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: p,
                difficulty: 0,
            },
        );
        r.unwrap();
        for req in [RoomRequest::Restart, RoomRequest::Goto(p)] {
            let mut w = w.clone();
            let err = change(&def, &rules, &mut w, &mut host, req).unwrap_err();
            assert!(matches!(err, SimError::Unsupported { .. }), "{req:?}");
            assert_eq!(w.status, SimStatus::Halted(err));
            assert!(w.persistent_rooms.is_empty(), "{req:?}: nothing was stored");
        }
    }

    #[test]
    fn wp4a_next_previous_and_past_the_end() {
        let (def, p, q) = two_rooms(false);
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (mut w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: p,
                difficulty: 0,
            },
        );
        r.unwrap();
        change(&def, &rules, &mut w, &mut host, RoomRequest::Next).unwrap();
        assert_eq!(w.room, q);
        change(&def, &rules, &mut w, &mut host, RoomRequest::Previous).unwrap();
        assert_eq!(w.room, p);
        let err = change(&def, &rules, &mut w, &mut host, RoomRequest::Previous).unwrap_err();
        assert!(matches!(err, SimError::Script { .. }));
        assert!(matches!(w.status, SimStatus::Halted(_)));
        assert_eq!(w.room, p, "nothing changed");
        // A halted sim does nothing more.
        host.log.clear();
        assert!(change(&def, &rules, &mut w, &mut host, RoomRequest::Next).is_err());
        assert!(host.log.is_empty());
        assert_eq!(w.room, p);
    }

    /// kaenbyou's load path: rSelectStage -> rLoad (creation code loadGame) -> room14.
    fn load_game() -> (GameDef, RoomId, RoomId, RoomId) {
        let mut b = GameDefBuilder::new("load");
        let world = probe_kind(&mut b, "world", true);
        let kid = probe_kind(&mut b, "kid", true);
        let marker = probe_kind(&mut b, "marker", false);
        b.boot(world);
        b.engine_kind(EngineKind::Kid, kid);
        let select = b.room(room("rSelectStage", 50));
        let mut rl = room("rLoad", 30);
        rl.on_create = vec![RoomAction::LoadGame];
        let load = b.room(rl);
        let saved = b.room(room("room14", 30));
        b.place(load, marker, 0.0, 0.0).unwrap();
        b.place(saved, marker, 1.0, 1.0).unwrap();
        (b.build().unwrap(), select, load, saved)
    }

    /// A room's `set_global difficulty` also sets the engine difficulty built-ins read (#568).
    #[test]
    fn set_global_difficulty_sets_the_engine_difficulty() {
        let mut b = GameDefBuilder::new("difficulty");
        let g = VarId(0);
        let mut ra = room("a", 30);
        ra.on_create = vec![RoomAction::SetGlobal {
            var: g,
            value: Value::Real(Real::new(7.0)),
            difficulty: Some(7),
        }];
        let a = b.room(ra);
        let def = b.build().unwrap();
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: a,
                difficulty: 2,
            },
        );
        r.unwrap();
        assert_eq!(w.engine.run.difficulty, 7);
        assert_eq!(w.globals.get(&g), Some(&Value::Real(Real::new(7.0))));
    }

    #[test]
    fn wp4a_nested_request_chains_after_the_sequence() {
        let (def, select, load, saved) = load_game();
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (mut w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: select,
                difficulty: 1,
            },
        );
        r.unwrap();
        w.save = Some(slot(saved));
        host.log.clear();
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(load)).unwrap();
        assert_eq!(
            host.log,
            [
                "RoomEnd world",
                "Create marker",
                "Load",
                "Create kid",
                // rLoad finishes all 7 steps (Design default; the recorder cannot see this).
                // The kid made by the creation code is in the room at step 7, so he gets it.
                "RoomStart marker",
                "RoomStart kid",
                "RoomStart world",
                // Then the chained change, before any tick.
                "RoomEnd world",
                "RoomEnd kid",
                "RoomEnd marker",
                "Create marker",
                "RoomStart marker",
                "RoomStart world",
                "RoomStart kid",
            ]
        );
        assert_eq!(
            w.room, saved,
            "0 ticks in rLoad (kaenbyou tr-room-12 rows 136-137)"
        );
        assert!(w.pending.is_none());
        assert_eq!(ids_of(&w, &def, "kid").len(), 1);
        assert_eq!(
            w.engine.run.difficulty, 0,
            "LoadGame applies the save's difficulty (tr-room-12 row 137; it was 1 before)"
        );
    }

    #[test]
    fn wp4a_nested_request_next_tick() {
        let (def, select, load, saved) = load_game();
        let mut rules = RunnerRules::default();
        rules.room.nested_request = NestedRequest::NextTick;
        let mut host = Probe::default();
        let (mut w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: select,
                difficulty: 1,
            },
        );
        r.unwrap();
        w.save = Some(slot(saved));
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(load)).unwrap();
        assert_eq!(w.room, load, "rLoad gets a tick");
        assert_eq!(w.pending, Some(RoomRequest::Goto(saved)), "left for tick row 15");
        // Tick row 15 of the next tick.
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(saved)).unwrap();
        assert_eq!(w.room, saved);
        assert!(w.pending.is_none());
    }

    #[test]
    fn wp4a_load_game_without_save_is_an_error() {
        let (def, select, load, _) = load_game();
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (mut w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: select,
                difficulty: 1,
            },
        );
        r.unwrap();
        let err = change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(load)).unwrap_err();
        assert!(matches!(err, SimError::Unsupported { .. }));
        assert!(matches!(w.status, SimStatus::Halted(_)));
    }

    /// A room whose marker requests the same room again in every Room Start.
    fn looping(max: u8) -> (Result<(), SimError>, usize, World) {
        let mut b = GameDefBuilder::new("loop");
        let marker = probe_kind(&mut b, "marker", false);
        let r0 = b.room(room("start", 50));
        let lp = b.room(room("loop", 50));
        b.place(lp, marker, 0.0, 0.0).unwrap();
        let def = b.build().unwrap();
        let mut rules = RunnerRules::default();
        rules.room.max_chained_changes = max;
        let mut host = Probe {
            react: Some(Box::new(move |_, ctx, _, _, ev| {
                if *ev == EventKind::RoomStart {
                    ctx.w.pending = Some(RoomRequest::Goto(lp));
                    return Flow::AbortTick;
                }
                Flow::Continue
            })),
            ..Probe::default()
        };
        let (mut w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: r0,
                difficulty: 0,
            },
        );
        r.unwrap();
        let res = change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(lp));
        let entries = host.log.iter().filter(|l| *l == "Create marker").count();
        (res, entries, w)
    }

    #[test]
    fn wp4a_chained_changes_are_bounded() {
        let (res, entries, w) = looping(8);
        let err = res.unwrap_err();
        assert!(matches!(err, SimError::Unsupported { .. }), "{err:?}");
        assert_eq!(entries, 9, "the requested change plus 8 chained ones");
        assert_eq!(w.status, SimStatus::Halted(err));
        let (res, entries, _) = looping(2);
        assert!(res.is_err());
        assert_eq!(entries, 3);
    }

    #[test]
    fn wp4a_destroy_self_in_creation_code_is_a_no_op() {
        let mut b = GameDefBuilder::new("destroy");
        let mut r = room("stage", 50);
        r.on_create = vec![RoomAction::DestroySelf];
        let stage = b.room(r);
        let def = b.build().unwrap();
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: stage,
                difficulty: 0,
            },
        );
        // R24: nothing is destroyed and the start goes on.
        r.expect("the start succeeds");
        assert_eq!(w.status, SimStatus::Running);
        assert_eq!(w.room, stage);
    }

    #[test]
    fn wp4a_halt_inside_the_sequence_stops_it() {
        let o = order_game();
        let rules = RunnerRules::default();
        let mut host = Probe {
            react: Some(Box::new(|_, ctx, _, name, ev| {
                if name == "blk" && *ev == EventKind::Create {
                    return ctx.halt(SimError::non_finite("x"));
                }
                Flow::Continue
            })),
            ..Probe::default()
        };
        let (w, r) = start(
            &o.def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: o.a,
                difficulty: 0,
            },
        );
        assert_eq!(r.unwrap_err(), SimError::non_finite("x"));
        assert_eq!(host.log, ["Create world", "Create ps", "Create blk"]);
        assert_eq!(w.status, SimStatus::Halted(SimError::non_finite("x")));
    }

    #[test]
    fn wp4a_game_end_and_game_restart() {
        let mut b = GameDefBuilder::new("restart");
        let world = probe_kind(&mut b, "world", true);
        let kid = probe_kind(&mut b, "kid", true);
        b.boot(world);
        b.engine_kind(EngineKind::Kid, kid);
        let title = b.room(room("title", 50));
        let mut boot = room("rInit", 50);
        boot.role = RoomRole::Boot;
        let init = b.room(boot);
        let mut stage = room("stage", 30);
        stage.persistent = true;
        let stage = b.room(stage);
        b.start_room(stage);
        let def = b.build().unwrap();
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (mut w, r) = start(&def, &rules, &mut host, StartPoint::FromSave(slot(stage)));
        r.unwrap();
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(title)).unwrap();
        assert!(w.persistent_rooms.contains_key(&stage));
        w.globals.insert(VarId(3), Value::Real(Real::ONE));
        w.engine.run.difficulty = 2;
        let old_world = ids_of(&w, &def, "world");

        host.log.clear();
        change(&def, &rules, &mut w, &mut host, RoomRequest::GameRestart).unwrap();
        assert_eq!(host.log, ["Create world", "GameStart world", "RoomStart world"]);
        assert_eq!(
            w.room, title,
            "room_first (index 0), not the first Boot-role room or start_room"
        );
        assert!(
            ids_of(&w, &def, "kid").is_empty(),
            "persistent instances are removed too"
        );
        let new_world = ids_of(&w, &def, "world");
        assert_eq!(new_world.len(), 1);
        assert_ne!(new_world, old_world);
        assert!(
            w.globals.is_empty() && w.engine.run.difficulty == 0,
            "globals cleared (R16)"
        );
        assert!(w.persistent_rooms.is_empty());
        assert_eq!(w.save, Some(slot(stage)), "the save file survives");

        change(&def, &rules, &mut w, &mut host, RoomRequest::GameEnd).unwrap();
        assert_eq!(w.status, SimStatus::Ended);
        assert_eq!(w.room, title);
        // An ended sim changes nothing.
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(init)).unwrap();
        assert_eq!(w.room, title);
    }

    #[test]
    fn wp4a_start_points() {
        let (def, _select, _load, saved) = load_game();
        let mut def = def;
        def.new_game_room = saved;
        let rules = RunnerRules::default();

        let mut host = Probe::default();
        let mut w = World::new(def.start_room, 1);
        w.save = Some(slot(saved));
        with_ctx(&def, &rules, &mut w, |ctx| {
            start_game_with(ctx, &StartPoint::NewGame { difficulty: 3 }, &mut host)
        })
        .unwrap();
        assert_eq!(w.room, saved);
        assert_eq!(w.save, None, "a new game deletes the save");
        assert_eq!(w.engine.run.difficulty, 3);

        let mut host = Probe {
            react: Some(Box::new(|host, ctx, _, name, ev| {
                if name == "kid" && *ev == EventKind::Create {
                    let room = &ctx.def.room(ctx.w.room).unwrap().name;
                    host.log.push(format!(
                        "kid sees {room} at {} difficulty {}",
                        ctx.w.room_speed.get(),
                        ctx.w.engine.run.difficulty
                    ));
                }
                Flow::Continue
            })),
            ..Probe::default()
        };
        let mut s = slot(saved);
        s.difficulty = 1;
        let (w, r) = start(&def, &rules, &mut host, StartPoint::FromSave(s.clone()));
        r.unwrap();
        assert_eq!(
            host.log,
            [
                "Create world",
                "Load",
                "Create kid",
                // R14: the saved room's file speed, not 0 or the start room.
                "kid sees room14 at 30 difficulty 1",
                "Create marker",
                "GameStart world",
                "GameStart kid",
                "GameStart marker",
                "RoomStart marker",
                "RoomStart world",
                "RoomStart kid",
            ],
            "the loaded kid is carried in like a persistent instance"
        );
        assert_eq!(w.room, saved);
        assert!(w.pending.is_none(), "the load's own request is this entry");
        assert_eq!(w.save, Some(s));
        assert_eq!(w.engine.run.difficulty, 1);
        let kid = ids_of(&w, &def, "kid");
        assert_eq!(
            w.get(kid[0]).map(|k| (k.m.x(), k.m.y())),
            Some((Real::new(49.0), Real::new(119.0)))
        );
    }

    /// A persistent room whose creation code spawns a `made` instance with params.
    fn revisit_game() -> (GameDef, RoomId, RoomId) {
        let mut b = GameDefBuilder::new("revisit");
        let made = probe_kind(&mut b, "made", false);
        let mut r0 = room("p", 50);
        r0.persistent = true;
        r0.on_create = vec![RoomAction::Spawn {
            kind: made,
            x: Real::ZERO,
            y: Real::ZERO,
            params: num_param(),
        }];
        let p = b.room(r0);
        let q = b.room(room("q", 50));
        (b.build().unwrap(), p, q)
    }

    fn num_param() -> Params {
        let mut params = Params::default();
        params.0.insert("num".into(), ParamValue::Real(Real::new(2.0)));
        params
    }

    #[test]
    fn wp4a_creation_code_on_revisit_switch() {
        let (def, p, q) = revisit_game();
        for (on_revisit, want) in [(true, 2), (false, 1)] {
            let mut rules = RunnerRules::default();
            rules.room.creation_code_on_revisit = on_revisit;
            let mut host = Probe::default();
            let (mut w, r) = start(
                &def,
                &rules,
                &mut host,
                StartPoint::Room {
                    room: p,
                    difficulty: 0,
                },
            );
            r.unwrap();
            change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(q)).unwrap();
            change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(p)).unwrap();
            assert_eq!(ids_of(&w, &def, "made").len(), want, "on_revisit {on_revisit}");
            assert_eq!(host.params.len(), want);
            assert_eq!(
                host.params[0],
                ("made".to_string(), num_param()),
                "Spawn passes its params"
            );
        }
    }

    #[test]
    fn wp4a_game_restart_switches() {
        let o = order_game();
        let mut rules = RunnerRules::default();
        rules.room.restart_events = true;
        rules.room.globals_survive_restart = true;
        let mut host = Probe::default();
        let (mut w, r) = start(
            &o.def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: o.b,
                difficulty: 2,
            },
        );
        r.unwrap();
        w.globals.insert(VarId(3), Value::Real(Real::ONE));
        host.log.clear();
        change(&o.def, &rules, &mut w, &mut host, RoomRequest::GameRestart).unwrap();
        assert_eq!(
            host.log[..2],
            ["RoomEnd blk", "RoomEnd world"],
            "restart_events: Room End before removal"
        );
        assert_eq!(w.room, o.a, "room_first");
        assert_eq!(w.globals.get(&VarId(3)), Some(&Value::Real(Real::ONE)));
        assert_eq!(
            w.engine.run.difficulty, 2,
            "globals_survive_restart keeps the engine globals"
        );
    }

    #[test]
    fn wp4a_views_reset_on_entry() {
        let mut b = GameDefBuilder::new("views");
        let follow = probe_kind(&mut b, "follow", false);
        let r0 = b.room(room("plain", 50));
        let mut rv = room("viewed", 50);
        rv.views_enabled = true;
        rv.views[0] = ViewDef {
            visible: true,
            view: (Real::ZERO, Real::ZERO, Real::new(800.0), Real::new(608.0)),
            port: (Real::ZERO, Real::ZERO, Real::new(800.0), Real::new(608.0)),
            follow: Some(follow),
            hborder: Real::new(400.0),
            vborder: Real::new(304.0),
            hspeed: Real::new(-1.0),
            vspeed: Real::new(-1.0),
        };
        let viewed = b.room(rv);
        let def = b.build().unwrap();
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (mut w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: r0,
                difficulty: 0,
            },
        );
        r.unwrap();
        assert!(w.views.iter().all(|v| !v.enabled));
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(viewed)).unwrap();
        let v = &w.views[0];
        assert!(v.enabled && v.visible);
        assert_eq!(
            (v.w, v.h, v.hborder),
            (Real::new(800.0), Real::new(608.0), Real::new(400.0))
        );
        assert_eq!(v.follow, Some(Target::Kind(follow)));
        assert!(
            w.views[1].enabled && !w.views[1].visible,
            "views_enabled is per room"
        );
        // Moved by a camera, then the room is entered again: back to the file's view.
        w.views[0].x = Real::new(1600.0);
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(viewed)).unwrap();
        assert_eq!(w.views[0].x, Real::ZERO);
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(r0)).unwrap();
        assert!(w.views.iter().all(|v| !v.enabled && v.follow.is_none()));
    }

    /// kaenbyou tr-room-12 (rSelectStage -> rLoad, whose creation code loads the save and calls
    /// room_goto -> room14) against the sim on the same room chain: the room and difficulty of
    /// the first row after the change are what the sim has once tick row 15 finishes, and no
    /// row ever shows rLoad.
    #[test]
    #[allow(clippy::disallowed_methods, reason = "test-only: reads research trace files")]
    fn wp4a_tr_room_12_spends_no_tick_in_rload() {
        let Some(root) = std::env::var_os("IWBTG_RESEARCH") else {
            eprintln!("skipped: IWBTG_RESEARCH is not set");
            return;
        };
        let path = std::path::Path::new(&root).join("specs/engines/gm8/traces/kaenbyou/tr-room-12.csv");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let table = TraceTable::parse(&text).expect("trace parses");
        let col = |name: &str| table.column(name).unwrap_or_else(|| panic!("no column {name}"));
        let (frame, room_col, diff) = (col("frame"), col("room"), col("global.difficulty"));
        let row = |f: i64| {
            let r = table
                .rows
                .iter()
                .find(|r| r[frame] == TV::Int(f))
                .unwrap_or_else(|| panic!("frame {f} missing"));
            let TV::Str(room) = &r[room_col] else {
                panic!("frame {f}: room is not a string")
            };
            let TV::Real(d) = r[diff] else {
                panic!("frame {f}: difficulty is not a real")
            };
            (room.clone(), d)
        };
        assert!(
            !table.rows.iter().any(|r| r[room_col] == TV::Str("rLoad".into())),
            "no tick in rLoad"
        );
        let (before_room, before_diff) = row(136);
        let (after_room, after_diff) = row(137);
        // Part 1 saved on Medium: row 52 is the first room14 tick after the save.
        let saved_diff = row(52).1;
        let as_u8 = |d: f64| {
            (0..=3u8)
                .find(|&n| Real::new(f64::from(n)) == Real::new(d))
                .expect("difficulty 0-3")
        };

        let (def, select, load, saved) = load_game();
        assert_eq!(def.room(select).unwrap().name, before_room);
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (mut w, r) = start(
            &def,
            &rules,
            &mut host,
            StartPoint::Room {
                room: select,
                difficulty: as_u8(before_diff),
            },
        );
        r.unwrap();
        let mut s = slot(saved);
        s.difficulty = as_u8(saved_diff);
        w.save = Some(s);
        // Tick 136, row 15: the warp's request.
        change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(load)).unwrap();
        assert!(w.pending.is_none(), "nothing left for tick 137");
        assert_eq!(def.room(w.room).unwrap().name, after_room, "row 137's room");
        assert_eq!(w.engine.run.difficulty, as_u8(after_diff), "row 137's difficulty");
    }

    // -- Ranked (stripped) mode (#561) ---------------------------------------------------------

    use iwbtg_sim::def::NameId;
    use iwbtg_sim::stripped::{PlayMode, RankedStart};

    /// The order game with a third room `c`, the `ps` placed instance named `tut`.
    fn stripped_game() -> (Order, RoomId) {
        let mut o = order_game();
        let c = RoomId(u32::try_from(o.def.rooms.len()).unwrap());
        o.def.rooms.push(room("c", 50));
        o.def.names.push("tut".into());
        o.def.rooms[0].instances[0].name = Some(NameId(0));
        (o, c)
    }

    fn start_in(
        def: &GameDef,
        mode: PlayMode,
        host: &mut Probe,
        at: StartPoint,
    ) -> (World, Result<(), SimError>) {
        let rules = RunnerRules::default();
        let mut w = World::new(def.start_room, 1);
        w.mode = mode;
        let r = with_ctx(def, &rules, &mut w, |ctx| start_game_with(ctx, &at, host));
        (w, r)
    }

    #[test]
    fn stripped_new_game_starts_in_the_ranked_room_with_its_globals_set_first() {
        let (mut o, _) = stripped_game();
        o.def.stripped.start = RankedStart {
            room: Some(o.b),
            globals: vec![(o.g, Value::Real(Real::new(3.0)))],
            difficulty: Some(2),
        };
        let mut host = Probe {
            watch: Some(o.g),
            ..Probe::default()
        };
        let at = StartPoint::NewGame { difficulty: 0 };
        let (w, r) = start_in(&o.def, PlayMode::Stripped, &mut host, at.clone());
        r.unwrap();
        assert_eq!(w.room, o.b, "the ranked start room, not new_game_room");
        assert_eq!(w.globals.get(&o.g), Some(&Value::Real(Real::new(3.0))));
        assert_eq!(
            w.engine.run.difficulty, 2,
            "the `difficulty` global sets the engine's"
        );
        assert_eq!(
            host.log,
            [
                "Create world",
                "Create blk+g",
                "GameStart blk+g",
                "GameStart world+g",
                "RoomStart blk+g",
                "RoomStart world+g"
            ],
            "the globals are set after boot, before the start room's first Create"
        );

        // Play original ignores [stripped]: new_game_room, no globals.
        let mut host = Probe::default();
        let (w, r) = start_in(&o.def, PlayMode::Original, &mut host, at);
        r.unwrap();
        assert_eq!(w.room, o.def.new_game_room);
        assert_eq!(w.engine.run.difficulty, 0);
    }

    #[test]
    fn stripped_start_without_a_room_uses_new_game_room() {
        let (mut o, _) = stripped_game();
        o.def.new_game_room = o.b;
        let mut host = Probe::default();
        let (w, r) = start_in(
            &o.def,
            PlayMode::Stripped,
            &mut host,
            StartPoint::NewGame { difficulty: 1 },
        );
        r.unwrap();
        assert_eq!(w.room, o.b);
        assert_eq!(
            w.engine.run.difficulty, 1,
            "no `difficulty` global keeps the start's"
        );
    }

    #[test]
    fn stripped_cut_rooms_redirect_every_room_change() {
        let (mut o, c) = stripped_game();
        o.def.stripped.cut_rooms.insert(o.b, c);
        let rules = RunnerRules::default();
        let at = StartPoint::Room {
            room: o.a,
            difficulty: 0,
        };
        for (mode, want) in [(PlayMode::Stripped, c), (PlayMode::Original, o.b)] {
            for req in [RoomRequest::Goto(o.b), RoomRequest::Next] {
                let mut host = Probe::default();
                let (mut w, r) = start_in(&o.def, mode, &mut host, at.clone());
                r.unwrap();
                change(&o.def, &rules, &mut w, &mut host, req).unwrap();
                assert_eq!(w.room, want, "{mode:?} {req:?}");
            }
        }
        // A start point and a save load into a cut room land in its `to` room too.
        let mut host = Probe::default();
        let (w, r) = start_in(
            &o.def,
            PlayMode::Stripped,
            &mut host,
            StartPoint::Room {
                room: o.b,
                difficulty: 0,
            },
        );
        r.unwrap();
        assert_eq!(w.room, c);
        let mut host = Probe::default();
        let (w, r) = start_in(
            &o.def,
            PlayMode::Stripped,
            &mut host,
            StartPoint::FromSave(slot(o.b)),
        );
        r.unwrap();
        assert_eq!(w.room, c);
    }

    #[test]
    fn stripped_cut_kinds_skip_placed_instances_and_spawn_actions() {
        let (mut o, _) = stripped_game();
        let blk = o.def.kinds.iter().position(|k| k.name == "blk").unwrap();
        let spawned = o.def.kinds.iter().position(|k| k.name == "spawned").unwrap();
        for k in [blk, spawned] {
            o.def.stripped.cut_kinds.insert(KindId(u32::try_from(k).unwrap()));
        }
        let at = StartPoint::Room {
            room: o.a,
            difficulty: 0,
        };
        let mut host = Probe::default();
        let (w, r) = start_in(&o.def, PlayMode::Stripped, &mut host, at.clone());
        r.unwrap();
        assert!(ids_of(&w, &o.def, "blk").is_empty(), "placed blk is cut");
        assert!(
            ids_of(&w, &o.def, "spawned").is_empty(),
            "the spawn action is cut"
        );
        assert_eq!(ids_of(&w, &o.def, "ps").len(), 1, "other kinds stay");
        assert_eq!(
            w.globals.get(&o.g),
            Some(&Value::Real(Real::ONE)),
            "the room's other actions still run"
        );
        let mut host = Probe::default();
        let (w, r) = start_in(&o.def, PlayMode::Original, &mut host, at);
        r.unwrap();
        assert_eq!(ids_of(&w, &o.def, "blk").len(), 1);
        assert_eq!(ids_of(&w, &o.def, "spawned").len(), 1);
    }

    #[test]
    fn stripped_cut_instances_skip_one_named_placed_instance() {
        let (mut o, _) = stripped_game();
        o.def.stripped.cut_instances.insert((o.a, NameId(0)));
        // The same name in another room is not cut.
        o.def.rooms[1].instances[0].name = Some(NameId(0));
        let at = StartPoint::Room {
            room: o.a,
            difficulty: 0,
        };
        let rules = RunnerRules::default();
        let mut host = Probe::default();
        let (mut w, r) = start_in(&o.def, PlayMode::Stripped, &mut host, at.clone());
        r.unwrap();
        assert!(ids_of(&w, &o.def, "ps").is_empty(), "`tut` in room a is cut");
        assert_eq!(ids_of(&w, &o.def, "blk").len(), 1);
        change(&o.def, &rules, &mut w, &mut host, RoomRequest::Goto(o.b)).unwrap();
        assert_eq!(ids_of(&w, &o.def, "blk").len(), 1, "`tut` in room b stays");
        let mut host = Probe::default();
        let (w, r) = start_in(&o.def, PlayMode::Original, &mut host, at);
        r.unwrap();
        assert_eq!(ids_of(&w, &o.def, "ps").len(), 1);
    }
}
