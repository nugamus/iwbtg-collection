//! The bounce (#835; register C14): `collide::move_bounce_solid` and `move_bounce_all`, precise
//! and not, and the `killer` `bounce` param. Rule: research CLONE_SPEC, GM8 runner motion,
//! Confirmed (recorder), "Bounce" (traces `animus/tr-mech-04.md`). The trace-table checks need
//! IWBTG_RESEARCH set to the research repo and skip with a note when it is unset; the world
//! checks always run.
#![allow(
    clippy::disallowed_methods,
    reason = "tests read the research path and trace files; the sim itself never does"
)]

mod bounce {
    use std::cell::Cell;
    use std::path::PathBuf;

    use iwbtg_sim::builtins;
    use iwbtg_sim::collide::bounce::{plain_bounce, precise_direction};
    use iwbtg_sim::collide::{move_bounce_all, move_bounce_solid};
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{Builtin, GameDef, KindDef, KindId, ParamValue, RoomDef, RoomId};
    use iwbtg_sim::input::InputFrame;
    use iwbtg_sim::motion::Motion;
    use iwbtg_sim::num::Real;
    use iwbtg_sim::rules::{NumRules, RunnerRules};
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::tick;
    use iwbtg_sim::world::{Ctx, InstId, SpawnInit, World};

    fn r(v: f64) -> Real {
        Real::new(v)
    }

    // ---- world checks -----------------------------------------------------------------------

    struct G {
        def: GameDef,
        room: RoomId,
        block: KindId,
        thing: KindId,
        cherry: KindId,
        plain: KindId,
        probe: KindId,
    }

    /// Kinds, all with a 32 x 32 sprite at origin (0, 0): a solid block, a non-solid `thing`, a
    /// killer `cherry` with `bounce` and `vspeed` 8, a killer `plain` without `bounce`, and an
    /// inert `probe`.
    fn game() -> G {
        let mut b = GameDefBuilder::new("bounce");
        let spr = b.sprite_rect((32, 32), (0, 0));
        let mut kind = |name: &str, base: Builtin, solid: bool, params: &[(&str, ParamValue)]| {
            let mut k = KindDef::new(name, base);
            k.sprite = Some(spr);
            k.solid = solid;
            for (n, v) in params {
                k.params.0.insert((*n).to_owned(), v.clone());
            }
            b.kind(k)
        };
        let block = kind("block", Builtin::Block, true, &[]);
        let thing = kind("thing", Builtin::Inert, false, &[]);
        let cherry = kind(
            "cherry",
            Builtin::Killer,
            false,
            &[("bounce", ParamValue::Bool(true)), ("vspeed", ParamValue::Real(r(8.0)))],
        );
        let plain = kind("plain", Builtin::Killer, false, &[("vspeed", ParamValue::Real(r(8.0)))]);
        let probe = kind("probe", Builtin::Inert, false, &[]);
        let room = b.room(RoomDef::new("r", 800, 608));
        let mut def = b.build().unwrap();
        for k in &mut def.kinds {
            k.events = builtins::event_set(k.base, &k.clone());
        }
        G {
            def,
            room,
            block,
            thing,
            cherry,
            plain,
            probe,
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

    fn state(w: &World, id: InstId) -> (f64, f64, f64, f64) {
        let m = &w.get(id).unwrap().m;
        (m.x().get(), m.y().get(), m.hspeed().get(), m.vspeed().get())
    }

    /// tr-mech-04 cherry 161724 (rows 39-40): falling at 8 with 4 px left above the block, the
    /// cherry reverses in Step without moving to contact, and the tick's motion moves it up 8.
    #[test]
    fn killer_bounce_reverses_before_contact_and_moves_by_the_new_speed() {
        let g = game();
        assert!(g.def.kinds[usize::try_from(g.cherry.0).unwrap()].events.step);
        assert!(
            !g.def.kinds[usize::try_from(g.plain.0).unwrap()].events.step,
            "no Step without bounce"
        );
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.block, 96.0, 512.0);
        // bbox_bottom 476 + 31 = 507: 4 px free above the block's top 512.
        let c = spawn(&g, &mut w, g.cherry, 100.0, 476.0);
        assert_eq!(state(&w, c), (100.0, 476.0, 0.0, 8.0));
        with(&g, &mut w, |ctx| tick::run_tick(ctx, InputFrame::EMPTY).unwrap());
        assert_eq!(state(&w, c), (100.0, 468.0, 0.0, -8.0), "reversed and moved up 8");
        with(&g, &mut w, |ctx| tick::run_tick(ctx, InputFrame::EMPTY).unwrap());
        assert_eq!(state(&w, c), (100.0, 460.0, 0.0, -8.0), "free: no more bounce");
        // Without `bounce` the same killer falls into the block.
        let p = spawn(&g, &mut w, g.plain, 300.0, 476.0);
        spawn(&g, &mut w, g.block, 296.0, 512.0);
        with(&g, &mut w, |ctx| tick::run_tick(ctx, InputFrame::EMPTY).unwrap());
        assert_eq!(state(&w, p), (300.0, 484.0, 0.0, 8.0));
    }

    fn probe_at(g: &G, w: &mut World, x: f64, y: f64, h: f64, v: f64) -> InstId {
        let id = spawn(g, w, g.probe, x, y);
        let m = &mut w.get_mut(id).unwrap().m;
        m.set_hspeed(r(h)).unwrap();
        m.set_vspeed(r(v)).unwrap();
        id
    }

    /// Not precise: each axis tests from the current position; the diagonal only when both
    /// pass (the outside corner, bounce-probe tag 4); nothing moves.
    #[test]
    fn plain_bounce_flips_per_axis_and_on_the_outside_corner() {
        let g = game();
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.block, 200.0, 200.0);
        // Up-left of the block, moving (3, 3): only the diagonal is blocked.
        let a = probe_at(&g, &mut w, 166.0, 166.0, 3.0, 3.0);
        with(&g, &mut w, |c| move_bounce_solid(c, a, false).unwrap());
        assert_eq!(state(&w, a), (166.0, 166.0, -3.0, -3.0));
        // Left of the block, moving right and down: only h flips.
        let b = probe_at(&g, &mut w, 166.0, 200.0, 3.0, 2.0);
        with(&g, &mut w, |c| move_bounce_solid(c, b, false).unwrap());
        assert_eq!(state(&w, b), (166.0, 200.0, -3.0, 2.0));
        // Inside the block: every test is blocked, both flip (tag 7).
        let e = probe_at(&g, &mut w, 205.0, 205.0, 1.5, -2.5);
        with(&g, &mut w, |c| move_bounce_solid(c, e, false).unwrap());
        assert_eq!(state(&w, e), (205.0, 205.0, -1.5, 2.5));
    }

    /// `move_bounce_solid` ignores non-solid instances; `move_bounce_all` does not (mode 8 and
    /// modes 4, 6 of bounce-bulk-all).
    #[test]
    fn bounce_all_tests_every_instance_and_solid_only_solids() {
        let g = game();
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.thing, 200.0, 240.0);
        let a = probe_at(&g, &mut w, 200.0, 204.0, 0.0, 8.0);
        with(&g, &mut w, |c| move_bounce_solid(c, a, false).unwrap());
        assert_eq!(state(&w, a), (200.0, 204.0, 0.0, 8.0), "solid only: no change");
        with(&g, &mut w, |c| move_bounce_all(c, a, false).unwrap());
        assert_eq!(state(&w, a), (200.0, 204.0, 0.0, -8.0));
    }

    /// Precise: a fall onto a floor goes straight back up; sliding along a floor at 0 turns to
    /// 10 (CLONE_SPEC "consequences"); speed is kept.
    #[test]
    fn precise_bounce_picks_a_multiple_of_ten() {
        let g = game();
        let mut w = World::new(g.room, 1);
        for x in [160.0, 192.0, 224.0, 256.0] {
            spawn(&g, &mut w, g.block, x, 300.0);
        }
        let a = probe_at(&g, &mut w, 200.0, 264.0, 0.0, 5.0);
        with(&g, &mut w, |c| move_bounce_solid(c, a, true).unwrap());
        let m = &w.get(a).unwrap().m;
        assert_eq!((m.direction().get(), m.speed().get()), (90.0, 5.0));
        assert_eq!((m.x().get(), m.y().get()), (200.0, 264.0), "no move");
        // Touching the floor (bbox_bottom 299), moving right.
        let b = probe_at(&g, &mut w, 200.0, 268.0, 3.0, 0.0);
        with(&g, &mut w, |c| move_bounce_solid(c, b, true).unwrap());
        let m = &w.get(b).unwrap().m;
        assert_eq!((m.direction().get(), m.speed().get()), (10.0, 3.0));
    }

    /// bounce-thin.csv: a block only the 0 degree sample reaches (speed 60, 91 px right). At
    /// directions 0, 3 and 357 the precise bounce changes nothing; at 6 and 354 it goes to 180.
    /// The not-precise bounce reverses `hspeed`.
    #[test]
    fn precise_bounce_ignores_a_thin_obstacle_between_samples() {
        let g = game();
        for (dir, want) in [(0.0, None), (3.0, None), (357.0, None), (6.0, Some(180.0)), (354.0, Some(180.0))] {
            let mut w = World::new(g.room, 1);
            spawn(&g, &mut w, g.block, 191.0, 100.0);
            let p = spawn(&g, &mut w, g.probe, 100.0, 100.0);
            w.get_mut(p).unwrap().m.motion_set(r(dir), r(60.0)).unwrap();
            let before = w.get(p).unwrap().m.clone();
            with(&g, &mut w, |c| move_bounce_solid(c, p, true).unwrap());
            let m = &w.get(p).unwrap().m;
            match want {
                None => assert_eq!(m, &before, "direction {dir}: no change"),
                Some(d) => assert_eq!(m.direction().get(), d, "direction {dir}"),
            }
        }
        let mut w = World::new(g.room, 1);
        spawn(&g, &mut w, g.block, 191.0, 100.0);
        let p = probe_at(&g, &mut w, 100.0, 100.0, 60.0, 0.0);
        with(&g, &mut w, |c| move_bounce_solid(c, p, false).unwrap());
        assert_eq!(state(&w, p).2, -60.0);
    }

    #[test]
    fn killer_rejects_a_bounce_that_is_not_a_bool() {
        let mut p = iwbtg_sim::def::Params::default();
        p.0.insert("bounce".to_owned(), ParamValue::Real(r(1.0)));
        assert!(builtins::init_state(Builtin::Killer, &p).is_err());
    }

    // ---- trace tables -----------------------------------------------------------------------

    /// One CSV: the header and the rows, split on commas outside quotes, quotes removed.
    struct Csv {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    }

    impl Csv {
        fn col(&self, name: &str) -> usize {
            self.header
                .iter()
                .position(|h| h == name)
                .unwrap_or_else(|| panic!("no column {name}"))
        }
    }

    fn split(line: &str) -> Vec<String> {
        let mut out = vec![String::new()];
        let mut quoted = false;
        for ch in line.chars() {
            match ch {
                '"' => quoted = !quoted,
                ',' if !quoted => out.push(String::new()),
                c => out.last_mut().unwrap().push(c),
            }
        }
        out
    }

    fn trace(case: &str) -> Option<Csv> {
        let Some(root) = std::env::var_os("IWBTG_RESEARCH") else {
            eprintln!("IWBTG_RESEARCH is unset; skipping animus/{case}");
            return None;
        };
        let path = PathBuf::from(root)
            .join("specs/engines/gm8/traces/animus")
            .join(case);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("IWBTG_RESEARCH is set but {} is unreadable: {e}", path.display()));
        let mut lines = text.lines().filter(|l| !l.is_empty() && !l.starts_with('#'));
        let header = split(lines.next().expect("header"));
        Some(Csv {
            header,
            rows: lines.map(split).collect(),
        })
    }

    fn num(s: &str) -> f64 {
        s.parse().unwrap_or_else(|e| panic!("not a number {s:?}: {e}"))
    }

    /// A "blocked" closure answering from a fixed list, in call order.
    fn answers(blocked: Vec<bool>) -> impl FnMut(Real, Real) -> Result<bool, iwbtg_sim::error::SimError> {
        let i = Cell::new(0);
        move |_, _| {
            let n = i.get();
            i.set(n + 1);
            Ok(blocked[n])
        }
    }

    /// Every bulk row: the not-precise rule gives the recorded `hspeed` and `vspeed` exactly;
    /// the precise rule gives the recorded direction, the speed is kept, and `hspeed` and
    /// `vspeed` match to the last bits that stay OPEN (research#193).
    #[test]
    fn bulk_tables_match_the_rule() {
        let nr = NumRules::default();
        for case in ["bounce-bulk.csv", "bounce-bulk-dnd.csv", "bounce-bulk-all.csv", "bounce-thin.csv"] {
            let Some(t) = trace(case) else {
                return;
            };
            let [mode, h0, v0, d0, speed, h1, v1, d1, nx, nh, nv, fm] = [
                "mode", "h0", "v0", "d0", "speed", "hspeed", "vspeed", "direction", "nx", "nh", "nv", "fm",
            ]
            .map(|c| t.col(c));
            let (mut plain, mut precise, mut exact) = (0, 0, 0);
            for (i, row) in t.rows.iter().enumerate() {
                let f = |c: usize| num(&row[c]);
                let m = f(mode);
                let at = format!("{case} row {}", i + 2);
                // gm-compare: ok (mode is a small integer tag from our own recorder)
                if m == 8.0 {
                    continue; // move_bounce_solid among non-solid obstacles: checked in the world test
                }
                // gm-compare: ok (mode is a small integer tag)
                let is_precise = [1.0, 3.0, 5.0, 7.0].contains(&m);
                let (h, v) = (f(h0), f(v0));
                if is_precise {
                    precise += 1;
                    let blocked: Vec<bool> = row[fm].chars().map(|c| c != '1').collect();
                    assert_eq!(blocked.len(), 36, "{at}");
                    let got = precise_direction((r(0.0), r(0.0)), r(f(speed)), r(f(d0)), &nr, answers(blocked))
                        .unwrap();
                    match got {
                        None => assert_eq!((f(h1), f(v1)), (h, v), "{at}: no change"),
                        Some(d) => {
                            let diff = (f(d1) - d.get() + 540.0).rem_euclid(360.0) - 180.0;
                            assert!(diff.abs() < 1e-9, "{at}: direction {} vs {}", d.get(), f(d1));
                            let mut mo = Motion::new_at(r(0.0), r(0.0));
                            mo.motion_set(r(f(d0)), r(f(speed))).unwrap();
                            mo.set_direction(d).unwrap();
                            let (gh, gv) = (mo.hspeed().get(), mo.vspeed().get());
                            let tol = 1e-12 * f(speed).abs().max(1.0);
                            assert!(
                                (gh - f(h1)).abs() <= tol && (gv - f(v1)).abs() <= tol,
                                "{at}: ({gh}, {gv}) vs ({}, {})",
                                f(h1),
                                f(v1)
                            );
                            exact += usize::from(gh.to_bits() == f(h1).to_bits() && gv.to_bits() == f(v1).to_bits());
                        }
                    }
                } else {
                    plain += 1;
                    // gm-compare: ok (recorder booleans are exactly 0 or 1)
                    let blocked = vec![f(nh) != 1.0, f(nv) != 1.0, f(nx) != 1.0];
                    let (fh, fv) = plain_bounce((r(0.0), r(0.0)), (r(h), r(v)), answers(blocked)).unwrap();
                    let want = (if fh { -h } else { h }, if fv { -v } else { v });
                    assert_eq!(
                        (want.0.to_bits(), want.1.to_bits()),
                        (f(h1).to_bits(), f(v1).to_bits()),
                        "{at}"
                    );
                }
            }
            eprintln!("{case}: {plain} not-precise rows, {precise} precise rows ({exact} with bit-equal h/v)");
            assert!(plain + precise > 0, "{case} has rows");
        }
    }

    /// tr-mech-04: every cherry and frame follows the not-precise rule and moves by the new
    /// speed (the last three columns are the next Step's tests).
    #[test]
    fn tr_mech_04_cherries_match_the_rule() {
        let Some(t) = trace("tr-mech-04.csv") else {
            return;
        };
        let n = t.header.len();
        let [id, x, y, h, v] = ["id", "x", "y", "hspeed", "vspeed"].map(|c| t.col(c));
        let mut bounces = 0;
        let mut steps = 0;
        for (i, a) in t.rows.iter().enumerate() {
            let Some(b) = t.rows[i + 1..].iter().find(|b| b[id] == a[id]) else {
                continue;
            };
            let f = |row: &Vec<String>, c: usize| num(&row[c]);
            // gm-compare: ok (recorder booleans are exactly 0 or 1)
            let blocked = vec![f(a, n - 3) != 1.0, f(a, n - 2) != 1.0, f(a, n - 1) != 1.0];
            let (h0, v0) = (f(a, h), f(a, v));
            let (fh, fv) = plain_bounce((r(f(a, x)), r(f(a, y))), (r(h0), r(v0)), answers(blocked)).unwrap();
            let (eh, ev) = (if fh { -h0 } else { h0 }, if fv { -v0 } else { v0 });
            bounces += usize::from(fh || fv);
            steps += 1;
            let at = format!("row {}", i + 2);
            assert_eq!((f(b, h), f(b, v)), (eh, ev), "{at}: speeds");
            assert_eq!((f(b, x), f(b, y)), (f(a, x) + eh, f(a, y) + ev), "{at}: position");
        }
        eprintln!("tr-mech-04: {steps} frame steps, {bounces} bounces");
        assert!(bounces > 100, "the trace has 129 bounces");
    }
}
