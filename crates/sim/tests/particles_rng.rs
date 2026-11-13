//! Particle systems' draws from the gameplay RNG (N10, #556): `sim::particles` against the
//! recorder traces of CLONE_SPEC numbers "Particle draw rules" (research#158).

mod n10 {
    use iwbtg_sim::num::Real;
    use iwbtg_sim::particles::{
        PartChild, PartColor, PartRange, PartType, Particles, Region, RegionDistr, RegionShape,
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
        assert!(p.burst(&mut rng, e, t2, -3).is_err(), "negative burst");
        assert!(
            p.create(&mut rng, s, t, 1).is_err(),
            "death particles and a random life"
        );
    }
}

/// The particle state in `World` and its tick slot (#764).
mod world {
    use crate::wp1_support::*;

    use iwbtg_sim::def::RoomId;
    use iwbtg_sim::events::{ProbeAct, ProbeOn, ProbeStep};
    use iwbtg_sim::hash::state_hash;
    use iwbtg_sim::input::InputFrame;
    use iwbtg_sim::particles::{PartRange, PartType, Particles, Region, RegionDistr, RegionShape};
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
    /// tick (OPEN, research#170); the next tick's does.
    #[test]
    fn room_request_tick_skips_the_update() {
        let mut fx = fixture(true, true);
        fx.tick(InputFrame::EMPTY);
        assert_eq!(fx.w.room, RoomId(1));
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
