//! WP4b: restart models, saves, views (docs/plans/sim-core.md 4.3 to 4.5, 12).
//!
//! Hand-built worlds with inert kinds standing in for the kid, the bow and the world.

mod wp4b {
    use iwbtg_sim::collide::Target;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{
        Builtin, ClassMask, DeathOnRestart, EngineKind, GameDef, KindDef, KindId, ResetKind, RestartKind,
        RoleSet, RoomDef, RoomId, RoomRole,
    };
    use iwbtg_sim::num::Real;
    use iwbtg_sim::restart::{boot_step, run_reset, run_restart};
    use iwbtg_sim::room::{RoomRequest, StartPoint, perform_change, start_game};
    use iwbtg_sim::rules::{NumRules, RunnerRules};
    use iwbtg_sim::run_globals::BootLoad;
    use iwbtg_sim::save::{SaveSlot, SavedPosition, load_into, quantize, save_counters, save_now};
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::view::follow_update;
    use iwbtg_sim::world::{Ctx, Flow, InstId, SimStatus, SpawnInit, World};

    fn r(v: f64) -> Real {
        Real::new(v)
    }

    struct G {
        def: GameDef,
        boot: RoomId,
        play: RoomId,
        menu: RoomId,
        kid: KindId,
        bow: KindId,
        block: KindId,
    }

    fn game(edit: impl FnOnce(&mut GameDef)) -> G {
        let mut b = GameDefBuilder::new("wp4b");
        let spr = b.sprite_rect((4, 4), (0, 0));
        let big = b.sprite_rect((10, 10), (0, 0));
        let mut k = KindDef::new("kid", Builtin::Inert);
        k.persistent = true;
        k.classes = ClassMask::KID;
        k.sprite = Some(spr);
        let kid = b.kind(k);
        let mut kf = KindDef::new("kid2", Builtin::Inert);
        kf.persistent = true;
        kf.classes = ClassMask::KID_FLIPPED;
        kf.sprite = Some(spr);
        let kid2 = b.kind(kf);
        let bow = b.kind(KindDef::new("bow", Builtin::Inert));
        let mut bl = KindDef::new("block", Builtin::Inert);
        bl.solid = true;
        bl.classes = ClassMask::BLOCK;
        bl.sprite = Some(big);
        let block = b.kind(bl);
        let mut w = KindDef::new("world", Builtin::Inert);
        w.persistent = true;
        let world = b.kind(w);
        b.engine_kind(EngineKind::Kid, kid);
        b.engine_kind(EngineKind::KidFlipped, kid2);
        b.engine_kind(EngineKind::Bow, bow);
        b.boot(world);
        let mut rb = RoomDef::new("boot", 800, 608);
        rb.role = RoomRole::Boot;
        let boot = b.room(rb);
        let play = b.room(RoomDef::new("play", 3200, 608));
        let mut rm = RoomDef::new("menu", 800, 608);
        rm.role = RoomRole::Menu;
        let menu = b.room(rm);
        let mut def = b.build().unwrap();
        edit(&mut def);
        G {
            def,
            boot,
            play,
            menu,
            kid,
            bow,
            block,
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

    fn slot(room: RoomId) -> SaveSlot {
        SaveSlot {
            room,
            x: r(49.0),
            y: r(119.0),
            flipped: false,
            facing: -1,
            difficulty: 2,
            deaths: 5,
            time: 9,
            bosses: 3,
            items: 4,
            clear: true,
        }
    }

    fn alive(w: &World, kind: KindId) -> Vec<InstId> {
        w.iter_alive().filter(|i| i.kind == kind).map(|i| i.id).collect()
    }

    // ---- quantize -------------------------------------------------------------------------

    fn q(model: SavedPosition, v: f64) -> f64 {
        quantize(r(v), r(v), model, &NumRules::default()).0.get()
    }

    #[test]
    fn base100_rounds_the_last_byte_half_to_even() {
        // Confirmed (recorder): 567.4 gives 567, 538.93 gives 539, 48.5 gives 48, 51.6 gives 52.
        for (v, want) in [
            (567.4, 567.0),
            (538.93, 539.0),
            (48.5, 48.0),
            (51.59999999999991, 52.0),
        ] {
            assert!((q(SavedPosition::XyBase100Bytes, v) - want).abs() < 1e-9, "{v}");
        }
        // Digits above 99 keep their place values: 12345.6 is 1, 23, 45.6 -> 46.
        assert!((q(SavedPosition::XyBase100Bytes, 12345.6) - 12346.0).abs() < 1e-9);
    }

    #[test]
    fn base256_truncates() {
        // Confirmed (recorder): 936.6 gives 936, 263.70000000000005 gives 263.
        assert!((q(SavedPosition::XyBase256Bytes, 936.6) - 936.0).abs() < 1e-9);
        assert!((q(SavedPosition::XyBase256Bytes, 263.700_000_000_000_05) - 263.0).abs() < 1e-9);
    }

    #[test]
    fn text_keeps_two_decimals() {
        // Confirmed (recorder): 551.70 and 542.55 survive; an integer stays whole.
        assert!((q(SavedPosition::XyText2Dec, 551.7) - 551.7).abs() < 1e-12);
        assert!((q(SavedPosition::XyText2Dec, 183.123_456) - 183.12).abs() < 1e-12);
        assert!((q(SavedPosition::XyText2Dec, 48.0) - 48.0).abs() < 1e-12);
    }

    #[test]
    fn previous_fixed_and_floor() {
        assert!((q(SavedPosition::XyPrevious, 12.345_678) - 12.345_678).abs() < 1e-12);
        assert!((q(SavedPosition::FixedPoint, 7.25) - 7.25).abs() < 1e-12);
        assert!((q(SavedPosition::FloorNudge, 7.9) - 7.0).abs() < 1e-12);
    }

    // ---- save_now, save_counters, load_into --------------------------------------------------

    #[test]
    fn save_now_stores_room_position_counters_and_flags() {
        let g = game(|_| {});
        let mut w = World::new(g.play, 1);
        let kid = spawn(&g, &mut w, g.kid, 100.0, 200.0);
        with(&g, &mut w, |c| {
            c.w.engine.run.difficulty = 1;
            c.w.engine.run.deaths = 7;
            c.w.engine.run.time = 11;
            c.w.engine.run.clear = true;
            c.w.engine.mech.bosses = 5;
            c.w.engine.mech.items = 6;
            c.w.get_mut(kid).unwrap().m.set_x(r(567.4)).unwrap();
            c.w.get_mut(kid).unwrap().img.set_xscale(r(-1.0)).unwrap();
            save_now(c, kid, SavedPosition::XyBase100Bytes, None).unwrap();
        });
        let s = w.save.clone().unwrap();
        assert_eq!((s.room, s.x, s.y), (g.play, r(567.0), r(200.0)));
        assert_eq!((s.facing, s.flipped, s.difficulty), (-1, false, 1));
        assert_eq!(
            (s.deaths, s.time, s.bosses, s.items, s.clear),
            (7, 11, 5, 6, true)
        );
    }

    #[test]
    fn save_now_previous_fixed_and_flipped_class() {
        let g = game(|_| {});
        let mut w = World::new(g.play, 1);
        let kid = spawn(&g, &mut w, g.kid, 10.0, 20.0);
        with(&g, &mut w, |c| {
            let m = &mut c.w.get_mut(kid).unwrap().m;
            m.set_xprevious(r(1.5)).unwrap();
            m.set_yprevious(r(2.5)).unwrap();
            save_now(c, kid, SavedPosition::XyPrevious, None).unwrap();
            assert_eq!(
                (c.w.save.as_ref().unwrap().x, c.w.save.as_ref().unwrap().y),
                (r(1.5), r(2.5))
            );
            assert!(save_now(c, kid, SavedPosition::FixedPoint, None).is_err());
            save_now(c, kid, SavedPosition::FixedPoint, Some((r(3.0), r(4.0)))).unwrap();
            assert_eq!(
                (c.w.save.as_ref().unwrap().x, c.w.save.as_ref().unwrap().y),
                (r(3.0), r(4.0))
            );
        });
        let kid2 = spawn(
            &g,
            &mut w,
            g.def.engine_kind(EngineKind::KidFlipped).unwrap(),
            0.0,
            0.0,
        );
        with(&g, &mut w, |c| {
            save_now(c, kid2, SavedPosition::XyBase100Bytes, None).unwrap();
            assert!(c.w.save.as_ref().unwrap().flipped);
        });
    }

    #[test]
    fn floor_nudge_follows_the_three_checks() {
        let g = game(|_| {});
        let mut w = World::new(g.play, 1);
        // A solid 10x10 block at (10, 0); the kid is a 4x4 box.
        spawn(&g, &mut w, g.block, 10.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        with(&g, &mut w, |c| {
            c.w.get_mut(kid).unwrap().m.set_x(r(7.5)).unwrap();
            c.w.get_mut(kid).unwrap().m.set_y(r(0.0)).unwrap();
            // The 4x4 box at x 7 reaches pixel 10, which the block holds, so every check
            // fails: step 1 gives x 8.5, step 2 gives y 1, step 3 adds 1 to both, then floor.
            save_now(c, kid, SavedPosition::FloorNudge, None).unwrap();
            let s = c.w.save.clone().unwrap();
            assert_eq!((s.x, s.y), (r(9.0), r(2.0)));
        });
    }

    #[test]
    fn counters_only_save_keeps_the_spot() {
        let g = game(|_| {});
        let mut w = World::new(g.play, 1);
        w.save = Some(slot(g.play));
        with(&g, &mut w, |c| {
            c.w.engine.run.deaths = 6;
            c.w.engine.run.time = 10;
            save_counters(c).unwrap();
        });
        let s = w.save.unwrap();
        assert_eq!(
            (s.deaths, s.time, s.x, s.y, s.items),
            (6, 10, r(49.0), r(119.0), 4)
        );
    }

    #[test]
    fn load_into_creates_a_persistent_kid_and_restores_globals() {
        let g = game(|d| d.restart.flipped_load_dy = Real::new(4.0));
        let mut w = World::new(g.play, 1);
        let mut s = slot(g.play);
        let id = with(&g, &mut w, |c| load_into(c, &s).unwrap());
        let inst = w.get(id).unwrap();
        assert!(inst.persistent);
        assert_eq!(
            (inst.m.x(), inst.m.y(), inst.m.xprevious()),
            (r(49.0), r(119.0), r(0.0))
        );
        assert_eq!(inst.img.xscale(), r(-1.0));
        let e = &w.engine;
        assert_eq!(
            (e.run.difficulty, e.run.deaths, e.run.time, e.run.clear),
            (2, 5, 9, true)
        );
        assert_eq!((e.mech.bosses, e.mech.items), (3, 4));
        // Flipped: the flipped kind and the +4 load offset.
        s.flipped = true;
        let id = with(&g, &mut w, |c| load_into(c, &s).unwrap());
        let inst = w.get(id).unwrap();
        assert!(inst.classes.intersects(ClassMask::KID_FLIPPED));
        assert_eq!(inst.m.y(), r(123.0));
    }

    // ---- restart and reset ------------------------------------------------------------------

    #[test]
    fn restart_via_boot_runs_the_n_n1_n2_timeline() {
        let g = game(|d| d.restart.death_on_restart = DeathOnRestart::SaveCounters);
        let mut w = World::new(g.play, 1);
        w.save = Some(slot(g.play));
        w.engine.run.deaths = 5;
        let kid = spawn(&g, &mut w, g.kid, 300.0, 300.0);
        // Tick N: the R handler.
        let flow = with(&g, &mut w, |c| run_restart(c, kid).unwrap());
        assert_eq!(flow, Flow::AbortTick);
        assert_eq!(w.pending, Some(RoomRequest::GameRestart));
        assert_eq!(w.engine.run.deaths, 6, "a retry counts a death");
        assert_eq!(
            w.save.as_ref().unwrap().deaths,
            6,
            "and writes the death-only save"
        );
        assert_eq!(w.engine.run.boot_load, Some(BootLoad { ticks_left: 1 }));
        let k = w.get(kid).unwrap();
        assert_eq!(
            (k.m.x(), k.m.y()),
            (r(49.0), r(119.0)),
            "the old kid already stands at the saved spot"
        );
        // The restart: everything goes, the temp marker stays, boot instances come back.
        with(&g, &mut w, |c| {
            perform_change(c, RoomRequest::GameRestart).unwrap()
        });
        assert_eq!(w.room, g.boot);
        assert!(alive(&w, g.kid).is_empty());
        assert_eq!(w.engine.run.deaths, 0, "globals do not survive game_restart");
        assert_eq!(w.engine.run.boot_load, Some(BootLoad { ticks_left: 1 }));
        // Tick N+1: the boot tick loads.
        let flow = with(&g, &mut w, |c| boot_step(c).unwrap());
        assert_eq!(flow, Flow::AbortTick);
        assert_eq!(w.pending, Some(RoomRequest::Goto(g.play)));
        assert_eq!(w.engine.run.boot_load, None);
        let kids = alive(&w, g.kid);
        assert_eq!(kids.len(), 1);
        assert_eq!(w.get(kids[0]).unwrap().m.xprevious(), r(0.0));
        assert_eq!(
            w.engine.run.deaths, 6,
            "the load restores the counters from the save"
        );
        // Tick N+2: the saved room, kid carried in.
        with(&g, &mut w, |c| {
            perform_change(c, RoomRequest::Goto(g.play)).unwrap()
        });
        assert_eq!(w.room, g.play);
        assert_eq!(alive(&w, g.kid), kids);
    }

    #[test]
    fn boot_ticks_count_down_and_zero_is_unsupported() {
        let g = game(|d| d.restart.boot_ticks = 3);
        let mut w = World::new(g.play, 1);
        w.save = Some(slot(g.play));
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        with(&g, &mut w, |c| run_restart(c, kid).unwrap());
        for left in [2, 1] {
            let flow = with(&g, &mut w, |c| boot_step(c).unwrap());
            assert_eq!(flow, Flow::Continue);
            assert_eq!(w.engine.run.boot_load, Some(BootLoad { ticks_left: left }));
        }
        assert_eq!(with(&g, &mut w, |c| boot_step(c).unwrap()), Flow::AbortTick);

        let g0 = game(|d| d.restart.boot_ticks = 0);
        let mut w = World::new(g0.play, 1);
        w.save = Some(slot(g0.play));
        let kid = spawn(&g0, &mut w, g0.kid, 0.0, 0.0);
        assert!(with(&g0, &mut w, |c| run_restart(c, kid)).is_err());
    }

    #[test]
    fn restart_without_a_save_is_a_plain_game_restart() {
        let g = game(|_| {});
        let mut w = World::new(g.play, 1);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        with(&g, &mut w, |c| run_restart(c, kid).unwrap());
        assert_eq!(w.pending, Some(RoomRequest::GameRestart));
        assert_eq!(w.engine.run.boot_load, None);
    }

    #[test]
    fn restart_does_nothing_in_an_excluded_role() {
        let g = game(|d| d.restart.excluded_roles = RoleSet(RoomRole::Menu.bit()));
        let mut w = World::new(g.menu, 1);
        w.save = Some(slot(g.play));
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        let flow = with(&g, &mut w, |c| run_restart(c, kid).unwrap());
        assert_eq!((flow, w.pending), (Flow::Continue, None));
    }

    #[test]
    fn room_goto_saved_recreates_the_kid_and_drops_the_bow() {
        let g = game(|d| d.restart.kind = RestartKind::RoomGotoSaved);
        let mut w = World::new(g.play, 1);
        w.save = Some(slot(g.boot));
        let old = spawn(&g, &mut w, g.kid, 300.0, 300.0);
        spawn(&g, &mut w, g.bow, 300.0, 300.0);
        with(&g, &mut w, |c| run_restart(c, old).unwrap());
        assert_eq!(w.pending, Some(RoomRequest::Goto(g.boot)));
        assert!(alive(&w, g.bow).is_empty());
        let kids = alive(&w, g.kid);
        assert_eq!(kids.len(), 1);
        assert_ne!(kids[0], old);
        assert_eq!(w.get(kids[0]).unwrap().m.x(), r(49.0));
    }

    #[test]
    fn room_goto_saved_skips_the_goto_in_a_menu_role() {
        let g = game(|d| d.restart.kind = RestartKind::RoomGotoSaved);
        let mut w = World::new(g.menu, 1);
        w.save = Some(slot(g.play));
        let old = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        with(&g, &mut w, |c| run_restart(c, old).unwrap());
        assert_eq!(w.pending, None);
        assert_eq!(alive(&w, g.kid).len(), 1);
    }

    #[test]
    fn showdown_load_never_restarts_the_game() {
        // R26 (settled): showdown's R has no game_restart.
        let g = game(|d| d.restart.kind = RestartKind::ShowdownLoad);
        let mut w = World::new(g.play, 1);
        w.save = Some(slot(g.play));
        let old = spawn(&g, &mut w, g.kid, 5.0, 5.0);
        with(&g, &mut w, |c| run_restart(c, old).unwrap());
        assert_eq!(w.pending, Some(RoomRequest::Goto(g.play)));
        assert_eq!(w.engine.run.boot_load, None);
        assert_eq!(alive(&w, g.kid).len(), 1);
        assert_ne!(alive(&w, g.kid)[0], old);
    }

    #[test]
    fn recreate_in_place_puts_the_kid_at_the_saved_spot_now() {
        let g = game(|d| d.restart.kind = RestartKind::RecreateInPlace);
        let mut w = World::new(g.play, 1);
        w.save = Some(slot(g.boot));
        let old = spawn(&g, &mut w, g.kid, 5.0, 5.0);
        with(&g, &mut w, |c| run_restart(c, old).unwrap());
        let kids = alive(&w, g.kid);
        assert_eq!(kids.len(), 1);
        assert_eq!(w.room, g.play, "still in the current room");
        assert_eq!(w.get(kids[0]).unwrap().m.x(), r(49.0));
        assert_eq!(w.pending, Some(RoomRequest::Goto(g.boot)));
    }

    #[test]
    fn reset_deletes_the_marker_and_restarts_or_goes_to_the_boot_room() {
        let g = game(|_| {});
        let mut w = World::new(g.play, 1);
        w.engine.run.boot_load = Some(BootLoad { ticks_left: 1 });
        let any = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        with(&g, &mut w, |c| run_reset(c, any).unwrap());
        assert_eq!(
            (w.pending, w.engine.run.boot_load),
            (Some(RoomRequest::GameRestart), None)
        );

        let g = game(|d| d.reset.kind = ResetKind::GameReset);
        let mut w = World::new(g.play, 1);
        w.engine.run.deaths = 9;
        let any = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        with(&g, &mut w, |c| run_reset(c, any).unwrap());
        assert_eq!(w.pending, Some(RoomRequest::Goto(g.boot)));
        assert_eq!(w.engine.run.deaths, 0);
    }

    #[test]
    fn start_from_save_still_works_with_the_real_load() {
        let g = game(|_| {});
        let mut w = World::new(g.boot, 1);
        let s = slot(g.play);
        let r = with(&g, &mut w, |c| start_game(c, &StartPoint::FromSave(s.clone())));
        r.unwrap();
        assert_eq!(w.status, SimStatus::Running);
        assert_eq!(w.room, g.play);
        assert_eq!(alive(&w, g.kid).len(), 1);
    }

    // ---- views ------------------------------------------------------------------------------

    /// A view 800 wide on a 3200 room, following the kid kind.
    fn follow_setup(g: &G, hborder: f64, hspeed: f64) -> World {
        let mut w = World::new(g.play, 1);
        let v = &mut w.views[0];
        v.enabled = true;
        v.visible = true;
        v.w = r(800.0);
        v.h = r(608.0);
        v.hborder = r(hborder);
        v.vborder = r(32.0);
        v.hspeed = r(hspeed);
        v.vspeed = r(-1.0);
        v.follow = Some(Target::Kind(g.kid));
        w
    }

    fn view_x_after(g: &G, w: &mut World, kid: InstId, x: f64) -> f64 {
        with(g, w, |c| {
            c.w.get_mut(kid).unwrap().m.set_x(r(x)).unwrap();
            follow_update(c).unwrap();
        });
        w.views[0].x.get()
    }

    #[test]
    fn view_follow_matches_the_animus_traces() {
        // Confirmed (recorder): hborder 600 acts as 400 on an 800 view; view = clamp(x - 400).
        let g = game(|_| {});
        let mut w = follow_setup(&g, 600.0, -1.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        for (x, want) in [(403.0, 3.0), (1342.0, 942.0), (100.0, 0.0), (3100.0, 2400.0)] {
            // Walk toward x so the view follows the border, as the trace does.
            let mut cur = w.get(kid).unwrap().m.x().get();
            let step = if x > cur { 1.0 } else { -1.0 };
            while (cur - x).abs() > 0.5 {
                cur += step
                    * if (x - cur).abs() < 50.0 {
                        (x - cur).abs()
                    } else {
                        50.0
                    };
                view_x_after(&g, &mut w, kid, cur);
            }
            assert!(
                (w.views[0].x.get() - want).abs() < 1e-9,
                "x {x}: {}",
                w.views[0].x.get()
            );
        }
    }

    #[test]
    fn view_speed_limits_and_freezes() {
        let g = game(|_| {});
        let mut w = follow_setup(&g, 32.0, 4.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        // Far right: the target jumps, the view moves 4 a tick.
        assert!((view_x_after(&g, &mut w, kid, 1000.0) - 4.0).abs() < 1e-9);
        assert!((view_x_after(&g, &mut w, kid, 1000.0) - 8.0).abs() < 1e-9);
        // Speed 0 freezes the axis.
        let mut w = follow_setup(&g, 32.0, 0.0);
        let kid = spawn(&g, &mut w, g.kid, 0.0, 0.0);
        assert!((view_x_after(&g, &mut w, kid, 1000.0)).abs() < 1e-9);
    }

    #[test]
    fn view_follow_ignores_disabled_views_and_missing_targets() {
        let g = game(|_| {});
        let mut w = follow_setup(&g, 32.0, -1.0);
        // No kid: nothing moves, no error.
        with(&g, &mut w, |c| follow_update(c).unwrap());
        assert_eq!(w.views[0].x, r(0.0));
        let kid = spawn(&g, &mut w, g.kid, 2000.0, 0.0);
        w.views[0].enabled = false;
        with(&g, &mut w, |c| follow_update(c).unwrap());
        assert_eq!(w.views[0].x, r(0.0));
        w.views[0].enabled = true;
        w.views[0].visible = false;
        with(&g, &mut w, |c| follow_update(c).unwrap());
        assert_eq!(w.views[0].x, r(0.0));
        w.views[0].visible = true;
        with(&g, &mut w, |c| follow_update(c).unwrap());
        assert!(w.views[0].x.get() > 0.0);
        let _ = kid;
    }
}
