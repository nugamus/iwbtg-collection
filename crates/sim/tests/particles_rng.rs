//! Particle systems' draws from the gameplay RNG (N10, #556): `sim::particles` against the
//! recorder traces of CLONE_SPEC numbers "Particle draw rules" (research#158).

mod n10 {
    use iwbtg_sim::num::Real;
    use iwbtg_sim::particles::{
        PartChild, PartColor, PartRange, PartType, PartTypeId, Particles, Region, RegionDistr, RegionShape,
    };
    use iwbtg_sim::rng::Rng;

    /// rely-the-needle-4-2 num-rng-a.csv: frame, random_get_seed() and
    /// instance_number(bloodEmitter), copied from the research trace.
    const NUM_RNG_A: &str = include_str!("fixtures/rely_num_rng_a.csv");

    /// A seed as `random_get_seed()` prints it (signed) to our unsigned state.
    fn seed(signed: i64) -> u32 {
        u32::try_from(signed.rem_euclid(1 << 32)).expect("fits")
    }

    fn empty() -> PartType {
        PartType {
            life: PartRange::fixed(1000.0),
            ..PartType::default()
        }
    }

    /// The type of both room1 emitters, by its draws: size, speed, direction and life ranges
    /// non-empty, orientation empty (CLONE_SPEC numbers "Particle draws per tick": 7 draws per A
    /// spawn = 2 region + 5 type).
    fn rely_type() -> PartType {
        PartType {
            size: PartRange::new(1.0, 2.0),
            speed: PartRange::new(1.0, 2.0),
            direction: PartRange::new(0.0, 360.0),
            life: PartRange::new(80.0, 100.0),
            ..PartType::default()
        }
    }

    /// The 40 blood of one bloodEmitter Step: per blood choose(0,1,2), random(0.2),
    /// irandom(35), random(6) (num-rng-b, CLONE_SPEC numbers).
    fn blood_step(rng: &mut Rng) {
        for _ in 0..40 {
            rng.choose(3).expect("3 arguments");
            rng.random(Real::new(0.2));
            rng.irandom(Real::new(35.0)).expect("finite");
            rng.random(Real::new(6.0));
        }
    }

    /// Replays rely room1's seed column (num-rng-a) bit for bit: per frame the game code draws
    /// first (bloodEmitter Step on frames where the emitter lives through the frame), then the
    /// particle update of room1's two systems in creation order: A, stream -20 from a linear
    /// rectangle; B, stream -10 from an invgaussian rectangle (objEffect Create, CLONE_SPEC
    /// numbers "Particle draws per tick").
    #[test]
    fn rely_room1_seed_column_replays_bit_for_bit() {
        let rows: Vec<(u32, u32, bool)> = NUM_RNG_A
            .lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with("frame"))
            .map(|l| {
                let f: Vec<&str> = l.split(',').collect();
                (
                    f[0].parse().expect("frame"),
                    seed(f[1].parse().expect("seed")),
                    f[2] != "0",
                )
            })
            .collect();
        assert_eq!(rows.len(), 101, "frames 0 to 100");
        let mut p = Particles::default();
        let ty = p.add_type(rely_type());
        for distr in [RegionDistr::Linear, RegionDistr::InvGaussian] {
            let s = p.add_system();
            let region = Region {
                shape: RegionShape::Rectangle,
                distr,
            };
            let e = p.add_emitter(s, region).unwrap();
            let n = if distr == RegionDistr::Linear { -20 } else { -10 };
            p.stream(e, ty, n).unwrap();
        }
        let mut rng = Rng::new(rows[0].1);
        let mut blood_frames = 0;
        for pair in rows.windows(2) {
            let ((_, _, before), (frame, want, after)) = (pair[0], pair[1]);
            if before && after {
                blood_step(&mut rng);
                blood_frames += 1;
            }
            p.update(&mut rng).unwrap();
            assert_eq!(rng.seed, want, "seed after frame {frame}");
        }
        assert_eq!(blood_frames, 19, "the emitter's 19 Steps, frames 15-33");
    }

    /// Rule 5 against runs 5a-5p: draws per particle of a stream -1 type.
    #[test]
    fn type_draws_follow_rule_5() {
        const fn r(a: f64, b: f64) -> PartRange {
            PartRange::new(a, b)
        }
        let with = |f: fn(&mut PartType)| {
            let mut t = empty();
            f(&mut t);
            t.draws()
        };
        assert_eq!(empty().draws(), 1);
        assert_eq!(with(|t| t.size = r(0.5, 1.5)), 2, "5a");
        assert_eq!(with(|t| t.life = r(50.0, 100.0)), 2, "5e");
        assert_eq!(with(|t| t.color = PartColor::Rgb([r(0.0, 255.0); 3])), 4, "5f");
        assert_eq!(with(|t| t.color = PartColor::Mix), 2, "5h");
        assert_eq!(with(|t| t.random_subimage = true), 2, "5i");
        assert_eq!(with(|t| t.color = PartColor::Rgb([r(100.0, 100.0); 3])), 1, "5l");
        assert_eq!(
            with(|t| {
                t.size = r(0.5, 1.5);
                t.speed = r(1.0, 2.0);
                t.direction = r(0.0, 360.0);
                t.orientation = r(0.0, 360.0);
                t.life = r(50.0, 100.0);
            }),
            6,
            "5m"
        );
        assert_eq!(
            with(|t| t.color = PartColor::Rgb([r(0.0, 128.0), r(64.0, 0.0), r(128.0, 128.0)])),
            2,
            "5n: reversed and empty channels draw nothing"
        );
        assert_eq!(with(|t| t.size = r(1.5, 0.5)), 1, "5p: reversed size");
    }

    /// Runs 8a and 8b: a burst of 10 (life 20) before frame 3's update; death particles cost
    /// their type's draws in frame 22, step particles in frames 3 to 21. Seeds from
    /// num-part-rng-8a.csv and -8b.csv.
    #[test]
    fn death_and_step_particles_match_runs_8a_8b() {
        let frame_seeds = |death: bool| {
            let mut p = Particles::default();
            let t2 = p.add_type(empty());
            let child = Some(PartChild { count: 1, ty: t2 });
            let mut t = empty();
            t.life = PartRange::fixed(20.0);
            if death {
                t.death = child;
            } else {
                t.step = child;
            }
            let t = p.add_type(t);
            let s = p.add_system();
            let e = p.add_emitter(s, Region::default()).unwrap();
            let mut rng = Rng::new(seed(-669_280_590));
            let mut out = Vec::new();
            for frame in 3..=23 {
                if frame == 3 {
                    p.burst(&mut rng, e, t, 10).unwrap();
                }
                p.update(&mut rng).unwrap();
                out.push((frame, rng.seed));
            }
            out
        };
        let at = |v: &[(u32, u32)], f: u32| v.iter().find(|(x, _)| *x == f).unwrap().1;
        let a = frame_seeds(true);
        assert_eq!(at(&a, 3), seed(884_892_524));
        assert_eq!(at(&a, 21), seed(884_892_524));
        assert_eq!(at(&a, 22), seed(-977_870_646));
        assert_eq!(at(&a, 23), seed(-977_870_646));
        let b = frame_seeds(false);
        assert_eq!(at(&b, 3), seed(-977_870_646));
        assert_eq!(at(&b, 4), seed(-47_514_312));
        assert_eq!(at(&b, 21), seed(1_563_137_174));
        assert_eq!(at(&b, 22), seed(1_563_137_174));
    }

    /// Run 6t-x0 (stream -1, rectangle invgaussian): seeds after frames 3, 4, 21, 22, 23, 40.
    #[test]
    fn invgaussian_rectangle_matches_run_6t() {
        let mut p = Particles::default();
        let t = p.add_type(empty());
        let s = p.add_system();
        let region = Region {
            shape: RegionShape::Rectangle,
            distr: RegionDistr::InvGaussian,
        };
        let e = p.add_emitter(s, region).unwrap();
        p.stream(e, t, -1).unwrap();
        let mut rng = Rng::new(seed(-669_280_590));
        let want = [
            (3, -1_211_254_405i64),
            (4, -976_017_281),
            (21, -1_329_406_683),
            (22, -16_947_782),
            (23, -1_590_238_045),
            (40, -333_565_065),
        ];
        for frame in 3..=40 {
            p.update(&mut rng).unwrap();
            if let Some((_, s)) = want.iter().find(|(f, _)| *f == frame) {
                assert_eq!(rng.seed, seed(*s), "seed after frame {frame}");
            }
        }
    }

    /// The model refuses what the spec does not cover instead of guessing.
    #[test]
    fn untraced_settings_are_unsupported() {
        let mut p = Particles::default();
        let t2 = p.add_type(empty());
        let mut t = empty();
        t.life = PartRange::new(10.0, 20.0);
        t.death = Some(PartChild { count: 1, ty: t2 });
        let t = p.add_type(t);
        let s = p.add_system();
        let e = p.add_emitter(s, Region::default()).unwrap();
        let mut rng = Rng::new(1);
        assert!(p.stream(e, t2, 0).is_err(), "stream 0");
        assert!(p.burst(&mut rng, e, t2, 0).is_err(), "burst 0");
        assert!(
            p.create(&mut rng, s, t, 1).is_err(),
            "death particles and a random life"
        );
        let mut zero = empty();
        zero.death = Some(PartChild { count: 0, ty: t2 });
        let zero = p.add_type(zero);
        assert!(p.burst(&mut rng, e, zero, 1).is_err(), "death count 0");
        let mut step = empty();
        step.step = Some(PartChild { count: -1, ty: t2 });
        let step = p.add_type(step);
        assert!(p.burst(&mut rng, e, step, 1).is_err(), "negative step count");
        assert!(
            p.stream(e, PartTypeId(999), 1).is_err(),
            "stream of a missing type"
        );
        assert!(
            p.create(&mut rng, s, PartTypeId(999), 1).is_err(),
            "create of a missing type"
        );
    }
}

/// CLONE_SPEC numbers "Particle draw rules" item 8 (#769) against the 23 runs of NUM-PART-RNG-2
/// (research#167, `traces/rely-the-needle-4-2/num-part-rng-2.md`): system S with the point
/// emitter E (rectangle linear), seeds replayed bit for bit on every frame from 3 to 302.
mod n10_2 {
    use iwbtg_sim::particles::{
        EmitterId, PartChild, PartRange, PartSystemId, PartType, PartTypeId, Particles, Region,
    };
    use iwbtg_sim::rng::Rng;

    /// run, frame, random_get_seed() and the particle count of each run, copied from the
    /// research traces `num-part-rng-2-<run>.csv`.
    const RUNS: &str = include_str!("fixtures/rely_num_part_rng_2.csv");

    fn seed(signed: i64) -> u32 {
        u32::try_from(signed.rem_euclid(1 << 32)).expect("fits")
    }

    /// `(frame, seed, count)` of one run, frames 2 to 302.
    fn rows(run: &str) -> Vec<(u32, u32, i64)> {
        let rows: Vec<_> = RUNS
            .lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with("run,"))
            .map(|l| l.split(',').collect::<Vec<_>>())
            .filter(|f| f[0] == run)
            .map(|f| {
                let n = |i: usize| f[i].parse::<i64>().expect("number");
                (u32::try_from(n(1)).expect("frame"), seed(n(2)), n(3))
            })
            .collect();
        assert_eq!(rows.len(), 301, "{run}: frames 2 to 302");
        rows
    }

    /// The empty-range type: 1 type draw, life 1000.
    fn empty() -> PartType {
        PartType {
            life: PartRange::fixed(1000.0),
            ..PartType::default()
        }
    }

    /// eat-the-lemon's rocket death type: size, speed, direction and life ranges (5 type draws).
    fn firework() -> PartType {
        PartType {
            size: PartRange::new(0.1, 0.2),
            speed: PartRange::new(0.5, 4.0),
            direction: PartRange::new(0.0, 360.0),
            life: PartRange::new(20.0, 30.0),
            ..PartType::default()
        }
    }

    /// A type with a fixed life and death particles.
    fn dying(life: f64, count: i32, ty: PartTypeId) -> PartType {
        PartType {
            life: PartRange::fixed(life),
            death: Some(PartChild { count, ty }),
            ..PartType::default()
        }
    }

    const S: PartSystemId = PartSystemId(0);

    /// Replays `run`: `before(frame, p, rng)` is the game code before that frame's update.
    fn replay(
        run: &str,
        mut p: Particles,
        mut before: impl FnMut(u32, &mut Particles, &mut Rng),
    ) -> Particles {
        let rows = rows(run);
        let mut rng = Rng::new(rows[0].1);
        for &(frame, want, _) in &rows[1..] {
            before(frame, &mut p, &mut rng);
            p.update(&mut rng).unwrap();
            assert_eq!(rng.seed, want, "{run}: seed after frame {frame}");
        }
        p
    }

    /// System S with E: the point emitter (a rectangle, linear).
    fn system(p: &mut Particles) -> EmitterId {
        let s = p.add_system();
        assert_eq!(s, S);
        p.add_emitter(s, Region::default()).unwrap()
    }

    /// Runs a1-a7: a burst of 10 (life 20) before frame 3 whose type has part_type_death(T, k,
    /// T2). k = 1, 2, 10, 150 with the firework (frame 22 draws 50, 100, 500, 7500: no cap), 150
    /// with the empty type; k = -1, -5 a chance per dying particle.
    #[test]
    fn death_count_scales_and_negative_is_a_chance() {
        for (run, k, fw) in [
            ("a1", 1, true),
            ("a2", 2, true),
            ("a3", 10, true),
            ("a4", 150, true),
            ("a5", 150, false),
            ("a6", -1, false),
            ("a7", -5, false),
        ] {
            let mut p = Particles::default();
            let t2 = p.add_type(if fw { firework() } else { empty() });
            let t = p.add_type(dying(20.0, k, t2));
            let e = system(&mut p);
            replay(run, p, |frame, p, rng| {
                if frame == 3 {
                    p.burst(rng, e, t, 10).unwrap();
                }
            });
        }
    }

    /// Runs b1-b9: a burst before every frame 4..301 of n = -1, -10, 1 with a real type (b1-b3),
    /// type id 999 never created (b4-b6) and id 0 with no type at all (b7-b9). A missing type
    /// costs the chance draw and the region's 2 draws per would-be particle, nothing else.
    #[test]
    fn negative_bursts_and_missing_types() {
        for (run, n, ty) in [
            ("b1", -1, Some(0)),
            ("b2", -10, Some(0)),
            ("b3", 1, Some(0)),
            ("b4", -1, Some(999)),
            ("b5", -10, Some(999)),
            ("b6", 1, Some(999)),
            ("b7", -1, None),
            ("b8", -10, None),
            ("b9", 1, None),
        ] {
            let mut p = Particles::default();
            if ty.is_some() {
                p.add_type(empty());
            }
            let t = PartTypeId(ty.unwrap_or(0));
            let e = system(&mut p);
            replay(run, p, |frame, p, rng| {
                if (4..=301).contains(&frame) {
                    p.burst(rng, e, t, n).unwrap();
                }
            });
        }
    }

    /// Runs c1-c7: a burst of 10 on E before frame 3 (death 1 of the firework type); E2, made
    /// after E, streams -1 of the empty type. Before frame 20: part_particles_clear,
    /// part_system_destroy, part_emitter_destroy_all, part_emitter_destroy(E2), nothing (c1-c5),
    /// and with a burst of life 40 part_emitter_destroy_all and part_emitter_destroy(E) (c6,
    /// c7). None of the calls draws or fires death particles; the particles of a destroyed
    /// emitter still fire theirs in frame 42.
    #[test]
    fn clearing_and_destroying() {
        for (run, life) in [
            ("c1", 1000.0),
            ("c2", 1000.0),
            ("c3", 1000.0),
            ("c4", 1000.0),
            ("c5", 1000.0),
            ("c6", 40.0),
            ("c7", 40.0),
        ] {
            let mut p = Particles::default();
            let fw = p.add_type(firework());
            let t = p.add_type(dying(life, 1, fw));
            let dust = p.add_type(empty());
            let e = system(&mut p);
            let e2 = p.add_emitter(S, Region::default()).unwrap();
            p.stream(e2, dust, -1).unwrap();
            let p = replay(run, p, |frame, p, rng| match frame {
                3 => p.burst(rng, e, t, 10).unwrap(),
                20 => match run {
                    "c1" => p.clear(S).unwrap(),
                    "c2" => p.destroy_system(S).unwrap(),
                    "c3" | "c6" => p.destroy_all_emitters(S).unwrap(),
                    "c4" => p.destroy_emitter(e2).unwrap(),
                    "c7" => p.destroy_emitter(e).unwrap(),
                    _ => {}
                },
                _ => {}
            });
            // global.C is -1 once S no longer exists.
            let gone = rows(run).last().unwrap().2 == -1;
            assert_eq!(p.systems[0].destroyed, gone, "{run}: S destroyed");
        }
    }
}

/// The particle state in `World` and its tick slot (#764).
mod world {
    use crate::wp1_support::*;

    use iwbtg_sim::def::RoomId;
    use iwbtg_sim::events::{ProbeAct, ProbeOn, ProbeStep};
    use iwbtg_sim::hash::state_hash;
    use iwbtg_sim::input::InputFrame;
    use iwbtg_sim::particles::{PartChild, PartRange, PartType, Particles, Region, RegionDistr, RegionShape};
    use iwbtg_sim::rng::Rng;
    use iwbtg_sim::room::RoomRequest;

    /// Two systems: a linear rectangle stream of 2 and an invgaussian ellipse stream of -3, of a
    /// type with three random ranges.
    fn particles() -> Particles {
        let mut p = Particles::default();
        let ty = p.add_type(PartType {
            size: PartRange::new(1.0, 2.0),
            speed: PartRange::new(1.0, 3.0),
            direction: PartRange::new(0.0, 360.0),
            life: PartRange::fixed(40.0),
            ..PartType::default()
        });
        for (distr, shape, n) in [
            (RegionDistr::Linear, RegionShape::Rectangle, 2),
            (RegionDistr::InvGaussian, RegionShape::Ellipse, -3),
        ] {
            let s = p.add_system();
            let e = p.add_emitter(s, Region { shape, distr }).unwrap();
            p.stream(e, ty, n).unwrap();
        }
        p
    }

    /// A world with [`particles`] and one probe that logs the RNG seed in End Step and Draw
    /// (and, with `request`, requests a room change in Step).
    fn fixture(on: bool, request: bool) -> Fx {
        let mut b = builder();
        let k = probe_kind(&mut b, "a");
        let mut fx = Fx::new(b.build().unwrap());
        fx.rules.num.particles_draw_rng = on;
        fx.w.particles = particles();
        let mut st = p("A")
            .with(ProbeStep::on(ProbeOn::EndStep, ProbeAct::LogSeed))
            .with(ProbeStep::on(ProbeOn::Late, ProbeAct::LogSeed));
        if request {
            st = st.with(ProbeStep::on(
                ProbeOn::Step,
                ProbeAct::RequestRoom(RoomRequest::Goto(RoomId(1))),
            ));
        }
        fx.probe(k, 0.0, 0.0, st);
        fx
    }

    fn seeds(log: &str) -> Vec<u32> {
        log.split(' ')
            .filter_map(|t| t.strip_prefix('r'))
            .map(|s| s.parse().expect("seed"))
            .collect()
    }

    /// Stepping the world runs `Particles::update` once per tick, between End Step and Draw: the
    /// seed logged in Draw is the End Step seed after the model's update, every tick.
    #[test]
    fn world_update_runs_after_end_step_and_matches_the_model() {
        let mut fx = fixture(true, false);
        let mut model = particles();
        let mut rng = fx.w.rng.clone();
        let mut expected = Vec::new();
        for _ in 0..20 {
            expected.push(rng.seed);
            model.update(&mut rng).unwrap();
            expected.push(rng.seed);
        }
        fx.idle(20);
        assert_eq!(seeds(&fx.log()), expected);
        assert_eq!(fx.w.rng, rng);
        assert_eq!(fx.w.particles, model);
        assert_ne!(expected[0], expected[1], "the update draws");
    }

    /// With `particles_draw_rng` off (engine_rev 0), nothing draws.
    #[test]
    fn switch_off_draws_nothing() {
        let mut fx = fixture(false, false);
        fx.idle(5);
        assert_eq!(fx.w.rng, Rng::new(1));
        assert_eq!(fx.w.particles, particles());
    }

    /// A room request in Step ends the tick before End Step, so the update does not run on that
    /// tick (Confirmed (recorder), CLONE_SPEC "Particle draw rules" item 9); the next tick's does.
    #[test]
    fn room_request_tick_skips_the_update() {
        let mut fx = fixture(true, true);
        // Live particles before the request tick: a type with a death particle is tracked, so a
        // model update on a scratch generator leaves live particles (life 3) in the system.
        let mut live = Particles::default();
        let dust = live.add_type(PartType::default());
        let spark = live.add_type(PartType {
            life: PartRange::fixed(3.0),
            death: Some(PartChild { ty: dust, count: 1 }),
            ..PartType::default()
        });
        let sys = live.add_system();
        let em = live
            .add_emitter(
                sys,
                Region {
                    shape: RegionShape::Rectangle,
                    distr: RegionDistr::Linear,
                },
            )
            .unwrap();
        live.stream(em, spark, 2).unwrap();
        let mut scratch = Rng::new(7);
        live.update(&mut scratch).unwrap();
        let before = live.clone();
        live.update(&mut scratch).unwrap();
        assert_ne!(live, before, "an update ages the live particles");
        let live = before;
        fx.w.particles = live.clone();
        fx.tick(InputFrame::EMPTY);
        assert_eq!(fx.w.room, RoomId(1));
        assert_eq!(
            fx.w.particles, live,
            "no new particle and no ageing on the request tick"
        );
        assert_eq!(fx.w.rng, Rng::new(1), "no draws on the request tick");
        assert!(seeds(&fx.log()).is_empty());
    }

    /// The particle state is hashed.
    #[test]
    fn particle_state_is_hashed() {
        let a = fixture(true, false);
        let mut b = fixture(true, false);
        assert_eq!(state_hash(&a.w), state_hash(&b.w));
        b.w.particles.add_system();
        assert_ne!(state_hash(&a.w), state_hash(&b.w));
    }
}

/// The `particle_system` built-in (#765): Create builds its `particles/<id>.toml` system in
/// `World::particles` as `Particles::build` does, the tick streams it, and the lifecycle params
/// make their call.
mod particle_system {
    use crate::wp1_support::*;

    use iwbtg_sim::builtins;
    use iwbtg_sim::def::RoomId;
    use iwbtg_sim::def::{
        Builtin, KindDef, KindId, ParamValue, PartCreateDef, PartEmitterDef, PartSystemDef, PartSystemDefId,
        PartTypeDef,
    };
    use iwbtg_sim::events::{EventKind, dispatch};
    use iwbtg_sim::particles::{
        PartChild, PartRange, PartType, PartTypeId, Particles, Region, RegionDistr, RegionShape,
    };
    use iwbtg_sim::rng::Rng;
    use iwbtg_sim::world::{InstId, SpawnInit};

    /// Two types (the second is the first's death type), one emitter with a stream of 2 and a
    /// burst of 3, and one `part_particles_create` of 2.
    fn system() -> PartSystemDef {
        let spark = PartType {
            size: PartRange::new(1.0, 2.0),
            direction: PartRange::new(0.0, 360.0),
            life: PartRange::fixed(5.0),
            death: Some(PartChild {
                ty: PartTypeId(1),
                count: 2,
            }),
            ..PartType::default()
        };
        let dust = PartType {
            speed: PartRange::new(0.5, 1.0),
            life: PartRange::fixed(3.0),
            ..PartType::default()
        };
        PartSystemDef {
            name: "fx".into(),
            types: vec![
                PartTypeDef {
                    name: "spark".into(),
                    draws: spark,
                    ..PartTypeDef::default()
                },
                PartTypeDef {
                    name: "dust".into(),
                    draws: dust,
                    ..PartTypeDef::default()
                },
            ],
            emitters: vec![PartEmitterDef {
                region: Region {
                    shape: RegionShape::Ellipse,
                    distr: RegionDistr::Gaussian,
                },
                stream: Some((0, 2)),
                burst: Some((0, 3)),
                ..PartEmitterDef::default()
            }],
            creates: vec![PartCreateDef {
                ty: 1,
                number: 2,
                ..PartCreateDef::default()
            }],
            ..PartSystemDef::default()
        }
    }

    fn fixture(on: bool, on_destroy: &str) -> (Fx, KindId) {
        let mut b = builder();
        let mut k = KindDef::new("fx", Builtin::ParticleSystem);
        k.params
            .0
            .insert("system".into(), ParamValue::Particles(PartSystemDefId(0)));
        k.params
            .0
            .insert("on_destroy".into(), ParamValue::Str(on_destroy.into()));
        k.events = builtins::event_set(Builtin::ParticleSystem, &k);
        let kind = b.kind(k);
        b.def_mut().particle_systems.push(system());
        let mut fx = Fx::new(b.build().unwrap());
        fx.rules.num.particles_draw_rng = on;
        (fx, kind)
    }

    fn spawn(fx: &mut Fx, kind: KindId) -> InstId {
        fx.ctx()
            .spawn(kind, r(0.0), r(0.0), SpawnInit::default())
            .expect("spawn")
    }

    /// The model: a fresh `Particles` with the system built from seed 1.
    fn model() -> (Particles, Rng) {
        let mut p = Particles::default();
        let mut rng = Rng::new(1);
        p.build(&mut rng, &system()).unwrap();
        (p, rng)
    }

    /// Create builds the system at once (types, emitter with its stream, burst, create, which
    /// draw), then each tick's update streams it, the same as the model.
    #[test]
    fn create_builds_the_system_and_the_tick_streams_it() {
        let (mut fx, kind) = fixture(true, "none");
        spawn(&mut fx, kind);
        let (mut p, mut rng) = model();
        assert_ne!(rng, Rng::new(1), "the burst and the create draw");
        assert_eq!(fx.w.particles, p);
        assert_eq!(fx.w.rng, rng);
        fx.idle(8);
        for _ in 0..8 {
            p.update(&mut rng).unwrap();
        }
        assert_eq!(fx.w.particles, p);
        assert_eq!(fx.w.rng, rng);
    }

    /// With `particles_draw_rng` off the built-in builds nothing and draws nothing.
    #[test]
    fn switch_off_builds_nothing() {
        let (mut fx, kind) = fixture(false, "destroy");
        let id = spawn(&mut fx, kind);
        fx.idle(3);
        fx.ctx().destroy(id);
        assert_eq!(fx.w.particles, Particles::default());
        assert_eq!(fx.w.rng, Rng::new(1));
    }

    /// `on_destroy` makes its call when the instance is destroyed; none of them draws.
    #[test]
    fn on_destroy_makes_its_call_without_drawing() {
        for word in ["none", "clear", "destroy_emitters", "destroy"] {
            let (mut fx, kind) = fixture(true, word);
            let id = spawn(&mut fx, kind);
            let (mut p, rng) = model();
            fx.ctx().destroy(id);
            let s = iwbtg_sim::particles::PartSystemId(0);
            match word {
                "clear" => p.clear(s).unwrap(),
                "destroy_emitters" => p.destroy_all_emitters(s).unwrap(),
                "destroy" => p.destroy_system(s).unwrap(),
                _ => {}
            }
            assert_eq!(fx.w.particles, p, "{word}");
            assert_eq!(fx.w.rng, rng, "{word}");
        }
    }

    /// A kind with `system` set to definitions 0 and 1 (`system()` and a copy at another depth)
    /// and `extra` params.
    fn listed(extra: &[(&str, ParamValue)]) -> (Fx, KindId) {
        let mut b = builder();
        let mut k = KindDef::new("fx", Builtin::ParticleSystem);
        k.params.0.insert(
            "system".into(),
            ParamValue::List(vec![
                ParamValue::Particles(PartSystemDefId(1)),
                ParamValue::Particles(PartSystemDefId(0)),
            ]),
        );
        for (name, v) in extra {
            k.params.0.insert((*name).into(), v.clone());
        }
        k.events = builtins::event_set(Builtin::ParticleSystem, &k);
        let kind = b.kind(k);
        b.def_mut().particle_systems.push(system());
        let mut second = system();
        second.name = "fx2".into();
        second.depth = r(-100000.0);
        second.emitters[0].burst = Some((0, 7));
        b.def_mut().particle_systems.push(second);
        let mut fx = Fx::new(b.build().unwrap());
        fx.rules.num.particles_draw_rng = true;
        (fx, kind)
    }

    /// A list builds each system in list order, all of it before the next (client#826), so the
    /// draws are those of building them one after the other; each tick updates both.
    #[test]
    fn a_list_builds_each_system_in_order() {
        let (mut fx, kind) = listed(&[]);
        spawn(&mut fx, kind);
        let mut p = Particles::default();
        let mut rng = Rng::new(1);
        p.build(&mut rng, &fx.def.particle_systems[1]).unwrap();
        p.build(&mut rng, &fx.def.particle_systems[0]).unwrap();
        assert_eq!(fx.w.particles, p);
        assert_eq!(fx.w.rng, rng);
        fx.idle(4);
        for _ in 0..4 {
            p.update(&mut rng).unwrap();
        }
        assert_eq!(fx.w.particles, p);
        assert_eq!(fx.w.rng, rng);
    }

    /// Room Start's call reaches every system; in an excepted room Room Start does nothing at
    /// all, and elsewhere `on_room_start_destroys_instance` destroys the instance after the call,
    /// so its `on_destroy` runs too (go-the-spook-house oRainSys, client#826).
    #[test]
    fn room_start_skips_excepted_rooms_and_may_destroy_the_instance() {
        let except = ParamValue::List(vec![ParamValue::Room(RoomId(3))]);
        let (mut fx, kind) = listed(&[
            ("on_room_start", ParamValue::Str("destroy_emitters".into())),
            ("on_room_start_except", except),
            ("on_room_start_destroys_instance", ParamValue::Bool(true)),
            ("on_destroy", ParamValue::Str("destroy".into())),
        ]);
        let id = spawn(&mut fx, kind);
        let built = fx.w.particles.clone();
        fx.w.room = RoomId(3);
        dispatch(&mut fx.ctx(), id, &EventKind::RoomStart);
        assert!(fx.w.exists(id), "an excepted room keeps the instance");
        assert_eq!(fx.w.particles, built, "and makes no call");
        fx.w.room = RoomId(4);
        dispatch(&mut fx.ctx(), id, &EventKind::RoomStart);
        assert!(!fx.w.exists(id), "destroyed after the call");
        assert!(
            fx.w.particles.systems.iter().all(|s| s.destroyed),
            "on_destroy destroyed both systems"
        );
    }
}

/// `blood_emitter` mode `particles` (#766): with the switch on, Create builds the blood system
/// (one type of 4 draws, one linear rectangle emitter streaming 15) and the tick streams it until
/// the emitter goes; kaenbyou's NUM-PART-RNG-3 run k1 (research#170) replays bit for bit.
mod blood_emitter {
    use crate::wp1_support::*;

    use iwbtg_sim::builtins::{self, BuiltinOutput};
    use iwbtg_sim::def::{Builtin, KindDef, KindId, ParamValue};
    use iwbtg_sim::events::EventKind;
    use iwbtg_sim::input::{Action, InputFrame};
    use iwbtg_sim::output::OutputEvent;
    use iwbtg_sim::particles::{PartRange, PartType, Particles, Region, RegionDistr, RegionShape};
    use iwbtg_sim::rng::Rng;
    use iwbtg_sim::world::{InstId, SpawnInit};

    /// kaenbyou num-part-rng-3-k1.csv: frame, seed, bloodEmitter count, systems, emitters and
    /// live particles, copied from the research trace.
    const K1: &str = include_str!("fixtures/kaenbyou_num_part_rng_3_k1.csv");

    /// (frame, seed) rows of [`K1`].
    fn k1() -> Vec<(u32, u32)> {
        K1.lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with("frame"))
            .map(|l| {
                let f: Vec<&str> = l.split(',').collect();
                let signed: i64 = f[1].parse().expect("seed");
                (
                    f[0].parse().expect("frame"),
                    u32::try_from(signed.rem_euclid(1 << 32)).expect("fits"),
                )
            })
            .collect()
    }

    fn fixture(on: bool) -> (Fx, KindId) {
        fixture_with(on, &[])
    }

    /// [`fixture`] with extra kind params.
    fn fixture_with(on: bool, extra: &[(&str, ParamValue)]) -> (Fx, KindId) {
        let mut b = builder();
        let mut k = KindDef::new("blood_emitter", Builtin::BloodEmitter);
        for (name, v) in extra {
            k.params.0.insert((*name).into(), v.clone());
        }
        k.params
            .0
            .insert("mode".into(), ParamValue::Str("particles".into()));
        k.params
            .0
            .insert("destroyed_by_restart".into(), ParamValue::Bool(true));
        k.events = builtins::event_set(Builtin::BloodEmitter, &k);
        let kind = b.kind(k);
        let mut fx = Fx::new(b.build().unwrap());
        fx.rules.num.particles_draw_rng = on;
        (fx, kind)
    }

    fn spawn(fx: &mut Fx, kind: KindId) -> InstId {
        fx.ctx()
            .spawn(kind, r(49.0), r(519.1), SpawnInit::default())
            .expect("spawn")
    }

    /// The model of one emitter: the blood type's draws, a linear rectangle, a stream of 15.
    fn model() -> Particles {
        let mut p = Particles::default();
        let ty = p.add_type(PartType {
            size: PartRange::new(0.0, 1.0),
            speed: PartRange::new(1.0, 5.0),
            direction: PartRange::new(0.0, 360.0),
            life: PartRange::fixed(50.0),
            ..PartType::default()
        });
        assert_eq!(p.types[0].draws(), 4);
        let s = p.add_system();
        let e = p
            .add_emitter(
                s,
                Region {
                    shape: RegionShape::Rectangle,
                    distr: RegionDistr::Linear,
                },
            )
            .unwrap();
        p.stream(e, ty, 15).unwrap();
        p
    }

    /// k1: the bloodEmitter made before frame 45's events streams 15 particles of 6 draws in each
    /// update from frame 45 to 50; R on frame 51 destroys it, and the seed stays put after that.
    #[test]
    fn k1_seed_column_replays_bit_for_bit() {
        k1_replay(fixture(true));
    }

    /// k1 with family B's `outlives_stream` (#877): the same seeds to frame 90, the system gone
    /// with the R press.
    #[test]
    fn k1_replays_with_outlives_stream() {
        k1_replay(fixture_with(true, &[("outlives_stream", ParamValue::Bool(true))]));
    }

    /// Replays k1's seed and `instance_number(bloodEmitter)` columns from frame 45 to 90.
    fn k1_replay((mut fx, kind): (Fx, KindId)) {
        let rows = k1();
        let emitters: Vec<usize> = K1
            .lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with("frame"))
            .filter_map(|l| {
                let f: Vec<&str> = l.split(',').collect();
                (f[0].parse::<u32>().ok()? >= 45).then(|| f[2].parse().expect("blood_emitters"))
            })
            .collect();
        fx.w.rng = Rng::new(0);
        let mut frame = 44;
        for (f, seed) in &rows {
            if *f <= 44 {
                assert_eq!(*seed, 0, "frame {f}: nothing draws before the emitter");
            }
        }
        spawn(&mut fx, kind);
        assert_eq!(fx.w.rng, Rng::new(0), "Create draws nothing");
        for ((f, seed), want) in rows.iter().filter(|(f, _)| *f >= 45).zip(&emitters) {
            frame += 1;
            assert_eq!(*f, frame);
            let input = if *f == 51 {
                let bit = Action::Restart.bit();
                InputFrame {
                    held: bit,
                    pressed: bit,
                    released: 0,
                }
            } else {
                InputFrame::EMPTY
            };
            fx.tick(input);
            assert_eq!(fx.w.rng.seed, *seed, "frame {f}");
            // Row 51 still shows the emitter, as its systems, emitters and particles columns
            // still show frame 50's: the trace records it before the R press is handled.
            if *f != 51 {
                let n = fx.w.instances().iter().filter(|i| i.kind == kind).count();
                assert_eq!(n, *want, "frame {f}: blood emitters");
            }
        }
        assert_eq!(frame, 90);
        assert!(
            fx.w.particles.systems.iter().all(|s| s.destroyed),
            "R destroyed the system"
        );
    }

    /// The stream runs in every update until alarm 0 (20) destroys the emitter and its system;
    /// then nothing draws. Made before a tick's events (as k1's probe), alarm 0 fires in the 20th
    /// tick's alarm phase, so 19 updates stream; a death in a collision or key press event (after
    /// the alarm phase) streams in its own tick's update too, so 20 ("removed after 20 ticks",
    /// kid/B.md Death).
    #[test]
    fn streams_until_alarm_zero_then_stops() {
        let (mut fx, kind) = fixture(true);
        let id = spawn(&mut fx, kind);
        let mut p = model();
        let mut rng = Rng::new(1);
        assert_eq!(fx.w.particles, p);
        for _ in 0..19 {
            p.update(&mut rng).unwrap();
        }
        p.destroy_system(iwbtg_sim::particles::PartSystemId(0)).unwrap();
        fx.idle(30);
        assert!(!fx.w.exists(id));
        assert_eq!(fx.w.rng, rng);
        assert_eq!(fx.w.particles, p);
    }

    /// `outlives_stream` (family B, #877): alarm 0 destroys only the emitter, so the stream and
    /// its 90 draws a tick stop after the same 19 updates as without it, while the instance
    /// stays (kid/B.md Death step 2; famb-death-timing-b: the bloodEmitter made on frame 12 is
    /// still there on frame 80, 68 ticks later). Its live particles age out with no draws; the
    /// Step stops sending the client particles. Restart then destroys it and its system.
    #[test]
    fn outlives_stream_stops_the_stream_and_keeps_the_instance() {
        let (mut fx, kind) = fixture_with(true, &[("outlives_stream", ParamValue::Bool(true))]);
        let id = spawn(&mut fx, kind);
        let mut p = model();
        let mut rng = Rng::new(1);
        for _ in 0..19 {
            p.update(&mut rng).unwrap();
        }
        fx.idle(20);
        assert!(fx.w.exists(id), "alarm 0 keeps the instance");
        p.destroy_all_emitters(iwbtg_sim::particles::PartSystemId(0))
            .unwrap();
        p.update(&mut rng).unwrap();
        assert_eq!(fx.w.particles, p, "the emitter is gone, the system stays");
        fx.w.output.clear();
        for _ in 0..48 {
            p.update(&mut rng).unwrap();
        }
        fx.idle(48);
        assert!(fx.w.exists(id), "still there 68 ticks after Create");
        assert_eq!(fx.w.rng, rng, "no draws after the stream ended");
        assert_eq!(fx.w.particles, p);
        assert!(
            !fx.w
                .output
                .iter()
                .any(|o| matches!(o, OutputEvent::Builtin(BuiltinOutput::DeathParticles { .. }))),
            "the client gets no new particles"
        );
        let bit = Action::Restart.bit();
        fx.tick(InputFrame {
            held: bit,
            pressed: bit,
            released: 0,
        });
        assert!(!fx.w.exists(id), "Restart destroys it");
        assert!(fx.w.particles.systems[0].destroyed);
        assert_eq!(fx.w.rng, rng);
    }

    /// A placed (or spawned) instance that sets `outlives_stream` on a plain `particles` kind
    /// outlives its stream: the instance's own params (the kind's, overridden by the placement's,
    /// `Ctx::spawn`, which room placement uses too) build its state, so no derived kind is needed.
    #[test]
    fn a_placed_instance_param_outlives_its_stream() {
        let (mut fx, kind) = fixture(true);
        let mut params = iwbtg_sim::def::Params::default();
        params.0.insert("outlives_stream".into(), ParamValue::Bool(true));
        let id = fx
            .ctx()
            .spawn(
                kind,
                r(49.0),
                r(519.1),
                SpawnInit {
                    params,
                    ..SpawnInit::default()
                },
            )
            .expect("spawn");
        let mut p = model();
        let mut rng = Rng::new(1);
        for _ in 0..19 {
            p.update(&mut rng).unwrap();
        }
        p.destroy_all_emitters(iwbtg_sim::particles::PartSystemId(0))
            .unwrap();
        for _ in 0..49 {
            p.update(&mut rng).unwrap();
        }
        fx.idle(68);
        assert!(fx.w.exists(id));
        assert_eq!(fx.w.rng, rng);
        assert_eq!(fx.w.particles, p);
    }

    /// `life = -1` (GameMaker's "alarm off") is accepted and sets no alarm: the instance and its
    /// stream never end by themselves.
    #[test]
    fn life_minus_one_sets_no_alarm() {
        let (mut fx, kind) = fixture_with(true, &[("life", ParamValue::Real(r(-1.0)))]);
        let id = spawn(&mut fx, kind);
        let mut p = model();
        let mut rng = Rng::new(1);
        for _ in 0..80 {
            p.update(&mut rng).unwrap();
        }
        fx.idle(80);
        assert!(fx.w.exists(id));
        assert_eq!(fx.w.rng, rng, "the stream still draws on tick 80");
    }

    /// Only `particles` mode can outlive its stream; the loader refuses it in the other modes.
    #[test]
    fn outlives_stream_needs_particles_mode() {
        let mut k = KindDef::new("e", Builtin::BloodEmitter);
        k.params
            .0
            .insert("outlives_stream".into(), ParamValue::Bool(true));
        assert!(builtins::init_state(Builtin::BloodEmitter, &k.params).is_err());
        k.params
            .0
            .insert("mode".into(), ParamValue::Str("particles".into()));
        assert!(builtins::init_state(Builtin::BloodEmitter, &k.params).is_ok());
    }

    /// A second emitter shares the type and adds a system after the first.
    #[test]
    fn a_second_emitter_shares_the_type() {
        let (mut fx, kind) = fixture(true);
        spawn(&mut fx, kind);
        spawn(&mut fx, kind);
        assert_eq!(fx.w.particles.types.len(), 1);
        assert_eq!(fx.w.particles.systems.len(), 2);
    }

    /// Room End destroys the system, without drawing.
    #[test]
    fn room_end_destroys_the_system() {
        let (mut fx, kind) = fixture(true);
        let id = spawn(&mut fx, kind);
        let mut ctx = fx.ctx();
        builtins::on_event(&mut ctx, id, &EventKind::RoomEnd).unwrap();
        assert!(fx.w.particles.systems[0].destroyed);
        assert_eq!(fx.w.rng, Rng::new(1));
        fx.idle(3);
        assert_eq!(fx.w.rng, Rng::new(1));
    }

    /// With the switch off nothing is built and nothing draws; the client still gets its
    /// particles each Step.
    #[test]
    fn switch_off_builds_nothing() {
        let (mut fx, kind) = fixture(false);
        spawn(&mut fx, kind);
        fx.idle(30);
        assert_eq!(fx.w.particles, Particles::default());
        assert_eq!(fx.w.rng, Rng::new(1));
    }

    /// The Step sends `DeathParticles` with the switch on too.
    #[test]
    fn step_still_sends_the_client_its_particles() {
        let (mut fx, kind) = fixture(true);
        let id = spawn(&mut fx, kind);
        let mut ctx = fx.ctx();
        builtins::on_event(&mut ctx, id, &EventKind::Step).unwrap();
        assert!(
            fx.w.output
                .contains(&OutputEvent::Builtin(BuiltinOutput::DeathParticles {
                    x: r(49.0),
                    y: r(519.1),
                    count: 15,
                    flipped: false,
                }))
        );
        assert_eq!(fx.w.rng, Rng::new(1), "the Step draws nothing");
    }

    /// A stream of 0 is not traced, so the loader refuses `per_step = 0` in particles mode.
    #[test]
    fn per_step_zero_is_refused() {
        let mut k = KindDef::new("e", Builtin::BloodEmitter);
        k.params
            .0
            .insert("mode".into(), ParamValue::Str("particles".into()));
        k.params.0.insert("per_step".into(), ParamValue::Real(r(0.0)));
        assert!(builtins::init_state(Builtin::BloodEmitter, &k.params).is_err());
    }
}
