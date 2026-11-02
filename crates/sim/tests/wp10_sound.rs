//! WP10 (#20): the sound model and the DLL adapters (docs/plans/sim-core.md 7, register S1-S5).

mod wp10 {
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{GameDef, RoomDef, SoundDef, SoundId, SoundKind};
    use iwbtg_sim::hash::state_hash;
    use iwbtg_sim::num::Real;
    use iwbtg_sim::output::OutputEvent;
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::rules::sound::{LoopPlaying, MultimediaSecond};
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::{self, DllMode, SoundOutput, SoundTarget, cleanmem, gmfmod, supersound};
    use iwbtg_sim::world::{Ctx, World};

    const FX: SoundId = SoundId(0);
    const FX2: SoundId = SoundId(1);
    const BGM: SoundId = SoundId(2);
    const BGM2: SoundId = SoundId(3);

    fn sd(name: &str, kind: SoundKind, length_ticks: u32) -> SoundDef {
        SoundDef {
            name: name.into(),
            kind,
            length_ticks,
            volume: Real::ONE,
            pan: Real::ZERO,
        }
    }

    struct Rig {
        def: GameDef,
        rules: RunnerRules,
        w: World,
        mode: DllMode,
    }

    fn rig(mode: DllMode) -> Rig {
        let mut b = GameDefBuilder::new("wp10");
        let room = b.room(RoomDef::new("r", 800, 608));
        let mut def = b.build().unwrap();
        def.sounds = vec![
            sd("fx", SoundKind::Normal, 10),
            sd("fx2", SoundKind::Normal, 5),
            sd("bgm", SoundKind::Multimedia, 100),
            sd("bgm2", SoundKind::Multimedia, 50),
        ];
        Rig {
            def,
            rules: RunnerRules::default(),
            w: World::new(room, 1),
            mode,
        }
    }

    impl Rig {
        fn with<T>(&mut self, f: impl FnOnce(&mut Ctx) -> T) -> T {
            let scripts = ScriptRuntime::default();
            let mut ctx = Ctx {
                def: &self.def,
                rules: &self.rules,
                w: &mut self.w,
                scripts: &scripts,
                dll_mode: self.mode,
            };
            f(&mut ctx)
        }

        fn play(&mut self, s: SoundId, looping: bool) {
            self.with(|c| sound::play(c, s, looping).unwrap());
        }

        fn playing(&self, s: SoundId) -> bool {
            sound::is_playing(&self.w, &self.def, s, &self.rules.sound)
        }

        /// One tick: row 16, then the tick counter.
        fn tick(&mut self) {
            self.with(sound::tick);
            self.w.tick += 1;
        }

        fn sound_events(&mut self) -> Vec<SoundOutput> {
            std::mem::take(&mut self.w.output)
                .into_iter()
                .filter_map(|e| match e {
                    OutputEvent::Sound(s) => Some(s),
                    _ => None,
                })
                .collect()
        }
    }

    fn r(v: f64) -> Real {
        Real::new(v)
    }

    // ---- built-in model ------------------------------------------------------------------------

    #[test]
    fn wp10_normal_sound_plays_for_its_length_then_ends() {
        let mut g = rig(DllMode::Model);
        assert!(!g.playing(FX));
        g.play(FX, false);
        assert!(g.playing(FX), "visible in the same tick (S1 default 0)");
        for _ in 0..9 {
            g.tick();
            assert!(g.playing(FX));
        }
        g.tick(); // tick 10: 10 ticks have passed
        assert!(!g.playing(FX));
        assert!(g.w.sound.copies.is_empty(), "row 16 dropped it");
    }

    #[test]
    fn wp10_ended_copy_is_not_playing_even_if_row_16_did_not_run() {
        let mut g = rig(DllMode::Model);
        g.play(FX2, false);
        g.w.tick += 5;
        assert!(!g.playing(FX2));
    }

    #[test]
    fn wp10_normal_copies_overlap_and_loop_runs_until_stopped() {
        let mut g = rig(DllMode::Model);
        g.play(FX, false);
        g.play(FX, false);
        assert_eq!(g.w.sound.copies.len(), 2);
        g.play(FX2, true);
        for _ in 0..200 {
            g.tick();
        }
        assert!(!g.playing(FX));
        assert!(g.playing(FX2), "a looping copy never ends");
        g.with(|c| sound::stop(c, FX2));
        assert!(!g.playing(FX2), "a stop is visible at once");
    }

    #[test]
    fn wp10_stop_ends_every_copy_and_stop_all_ends_all() {
        let mut g = rig(DllMode::Model);
        g.play(FX, false);
        g.play(FX, false);
        g.play(BGM, true);
        g.sound_events();
        g.with(|c| sound::stop(c, FX));
        assert!(!g.playing(FX) && g.playing(BGM));
        assert_eq!(
            g.sound_events(),
            vec![SoundOutput::Stop {
                target: SoundTarget::Builtin(FX)
            }]
        );
        g.with(sound::stop_all);
        assert!(!g.playing(BGM));
        assert_eq!(g.sound_events(), vec![SoundOutput::StopAll { adapter: false }]);
        // A stop of a sound that is not playing emits nothing.
        g.with(|c| sound::stop(c, FX));
        assert!(g.sound_events().is_empty());
    }

    #[test]
    fn wp10_second_multimedia_stops_the_first_by_default() {
        let mut g = rig(DllMode::Model);
        g.play(BGM, true);
        g.play(BGM2, false);
        assert!(!g.playing(BGM) && g.playing(BGM2));
        assert_eq!(g.w.sound.copies.iter().filter(|c| c.multimedia).count(), 1);
    }

    #[test]
    fn wp10_second_multimedia_ignored_when_s2_says_so() {
        let mut g = rig(DllMode::Model);
        g.rules.sound.multimedia_second = MultimediaSecond::Ignore;
        g.play(BGM, true);
        g.play(BGM2, false);
        assert!(g.playing(BGM) && !g.playing(BGM2));
    }

    #[test]
    fn wp10_same_multimedia_again_is_ignored_or_restarts_per_s3() {
        let mut g = rig(DllMode::Model);
        g.play(BGM, false);
        g.w.tick = 30;
        g.play(BGM, false);
        assert_eq!(g.w.sound.copies[0].start_tick, 0, "ignored (S3 default)");
        g.rules.sound.loop_playing = LoopPlaying::Restart;
        g.play(BGM, false);
        assert_eq!(g.w.sound.copies.len(), 1);
        assert_eq!(g.w.sound.copies[0].start_tick, 30, "restarted");
    }

    #[test]
    fn wp10_normal_sound_does_not_disturb_a_multimedia_copy() {
        let mut g = rig(DllMode::Model);
        g.play(BGM, true);
        g.play(FX, false);
        assert!(g.playing(BGM) && g.playing(FX));
    }

    #[test]
    fn wp10_isplaying_latency_s1_delays_the_start_only() {
        let mut g = rig(DllMode::Model);
        g.rules.sound.isplaying_latency = 2;
        g.play(FX, true);
        assert!(!g.playing(FX));
        g.tick();
        assert!(!g.playing(FX));
        g.tick();
        assert!(g.playing(FX));
        g.with(|c| sound::stop(c, FX));
        assert!(!g.playing(FX));
    }

    #[test]
    fn wp10_unknown_sound_is_an_error_and_not_playing() {
        let mut g = rig(DllMode::Model);
        let bad = SoundId(99);
        assert!(g.with(|c| sound::play(c, bad, false)).is_err());
        assert!(!g.playing(bad));
    }

    #[test]
    fn wp10_sound_volume_is_output_only_and_skips_multimedia() {
        let mut g = rig(DllMode::Model);
        let before = state_hash(&g.w);
        g.with(|c| sound::volume(c, FX, r(0.5)).unwrap());
        g.with(|c| sound::volume(c, BGM, r(0.5)).unwrap());
        assert_eq!(state_hash(&g.w), before);
        assert_eq!(
            g.sound_events(),
            vec![SoundOutput::Volume {
                target: SoundTarget::Builtin(FX),
                value: r(0.5)
            }]
        );
        assert!(g.with(|c| sound::volume(c, FX, r(f64::NAN))).is_err());
    }

    #[test]
    fn wp10_what_plays_is_hashed() {
        let mut g = rig(DllMode::Model);
        let idle = state_hash(&g.w);
        g.play(FX, false);
        let playing = state_hash(&g.w);
        assert_ne!(idle, playing);
        g.with(sound::stop_all);
        assert_eq!(state_hash(&g.w), idle);
    }

    #[test]
    fn wp10_builtin_model_runs_in_stub_mode_too() {
        let mut g = rig(DllMode::Stub);
        g.play(FX, false);
        assert!(g.playing(FX));
    }

    // ---- SuperSound ----------------------------------------------------------------------------

    #[test]
    fn wp10_supersound_getters_return_exactly_the_last_value_set() {
        let mut g = rig(DllMode::Model);
        assert_eq!(
            g.with(|c| supersound::load(c, "a.ogg", true, 0)),
            "0",
            "before init"
        );
        assert_eq!(g.with(supersound::init), "Yes");
        let h = g.with(|c| supersound::load(c, "a.ogg", true, 0));
        assert_eq!(h, "1");
        assert_eq!(
            g.with(|c| supersound::get_volume(c, &h)),
            "0",
            "new handle: full volume"
        );
        for v in [-2000.0, -1234.0, -10000.0, 0.0] {
            assert_eq!(g.with(|c| supersound::set_volume(c, &h, r(v))), "1");
            assert_eq!(g.with(|c| supersound::get_volume(c, &h)), format!("{v}"));
        }
        // Invalid handle.
        assert_eq!(g.with(|c| supersound::is_playing(c, "7")), "-1");
        assert_eq!(g.with(|c| supersound::is_playing(c, "x")), "-1");
        assert_eq!(g.with(|c| supersound::set_volume(c, "7", r(1.0))), "0");
    }

    #[test]
    fn wp10_supersound_play_loop_pause_resume_stop_and_length() {
        let mut g = rig(DllMode::Model);
        g.with(supersound::init);
        let h = g.with(|c| supersound::load(c, "a.ogg", true, 20));
        assert_eq!(g.with(|c| supersound::is_playing(c, &h)), "0");
        g.with(|c| supersound::play(c, &h));
        assert_eq!(g.with(|c| supersound::is_playing(c, &h)), "1");
        for _ in 0..5 {
            g.tick();
        }
        assert_eq!(g.with(|c| supersound::pause(c, &h)), "1");
        assert_eq!(g.with(|c| supersound::is_paused(c, &h)), "1");
        assert_eq!(g.with(|c| supersound::is_playing(c, &h)), "0");
        for _ in 0..100 {
            g.tick();
        }
        assert_eq!(g.with(|c| supersound::resume(c, &h)), "1");
        assert_eq!(
            g.with(|c| supersound::is_playing(c, &h)),
            "1",
            "the pause did not count"
        );
        for _ in 0..14 {
            g.tick();
        }
        assert_eq!(g.with(|c| supersound::is_playing(c, &h)), "1");
        g.tick();
        g.tick();
        assert_eq!(g.with(|c| supersound::is_playing(c, &h)), "0", "ran its 20 ticks");
        // Loop, then stop; a freed handle is invalid.
        g.with(|c| supersound::loop_sound(c, &h));
        for _ in 0..500 {
            g.tick();
        }
        assert_eq!(g.with(|c| supersound::is_playing(c, &h)), "1");
        assert_eq!(g.with(|c| supersound::is_looping(c, &h)), "1");
        g.with(|c| supersound::stop(c, &h));
        assert_eq!(g.with(|c| supersound::is_playing(c, &h)), "0");
        g.with(|c| supersound::free(c, &h));
        assert_eq!(g.with(|c| supersound::is_playing(c, &h)), "-1");
    }

    #[test]
    fn wp10_supersound_unknown_length_plays_until_stopped_and_stop_all_leaves_it() {
        let mut g = rig(DllMode::Model);
        g.with(supersound::init);
        let h = g.with(|c| supersound::load(c, "a.ogg", true, 0));
        g.with(|c| supersound::play(c, &h));
        for _ in 0..300 {
            g.tick();
        }
        g.with(sound::stop_all);
        assert_eq!(g.with(|c| supersound::is_playing(c, &h)), "1");
    }

    // ---- GMFMODSimple --------------------------------------------------------------------------

    #[test]
    fn wp10_gmfmod_ids_groups_and_exact_getters() {
        let mut g = rig(DllMode::Model);
        assert_eq!(
            g.with(|c| gmfmod::sound_add(c, "a.ogg", false, false, 0)),
            Real::ZERO,
            "before init"
        );
        g.with(|c| gmfmod::init(c, r(100.0), r(0.0)));
        let s1 = g.with(|c| gmfmod::sound_add(c, "a.ogg", false, false, 0));
        let s2 = g.with(|c| gmfmod::sound_add(c, "b.wav", false, false, 10));
        assert_eq!((s1.get(), s2.get()), (1.0, 2.0));
        assert_eq!(
            g.with(|c| gmfmod::sound_play(c, Real::ZERO, false)),
            Real::ZERO,
            "sound 0 errors"
        );
        assert_eq!(g.with(|c| gmfmod::sound_play(c, r(9.0), false)), Real::ZERO);
        g.with(|c| gmfmod::sound_set_group(c, s1, r(1.0)));
        let music = g.with(|c| gmfmod::sound_loop(c, s1, false));
        let fx = g.with(|c| gmfmod::sound_play(c, s2, false));
        assert_eq!((music.get(), fx.get()), (1.0, 2.0));
        // Volume and pitch: defaults 1, then exactly what was set.
        assert_eq!(g.with(|c| gmfmod::group_get_volume(c, r(1.0))), Real::ONE);
        let v = 0.123_456_789_012_345_67;
        g.with(|c| gmfmod::group_set_volume(c, r(1.0), r(v)));
        assert_eq!(
            g.with(|c| gmfmod::group_get_volume(c, r(1.0))).get().to_bits(),
            v.to_bits()
        );
        let p = 1.0 / 76.0;
        g.with(|c| gmfmod::group_set_pitch(c, r(1.0), r(p)));
        assert_eq!(
            g.with(|c| gmfmod::group_get_pitch(c, r(1.0))).get().to_bits(),
            p.to_bits()
        );
        assert_eq!(
            g.with(|c| gmfmod::group_get_pitch(c, r(9.0))),
            Real::ZERO,
            "bad group"
        );
        // A group stop ends the group's instances only.
        g.with(|c| gmfmod::group_stop(c, r(1.0)));
        assert_eq!(g.with(|c| gmfmod::instance_is_playing(c, music)), Real::ZERO);
        assert_eq!(g.with(|c| gmfmod::instance_is_playing(c, fx)), Real::ONE);
        // One-shot ends after its length; update is a no-op.
        for _ in 0..10 {
            g.with(gmfmod::update);
            g.tick();
        }
        assert_eq!(
            g.with(|c| gmfmod::instance_is_playing(c, fx)),
            Real::ZERO,
            "invalid when over"
        );
        // Instance volume and stop.
        let again = g.with(|c| gmfmod::sound_loop(c, s1, true));
        assert_eq!(g.with(|c| gmfmod::instance_get_volume(c, again)), Real::ONE);
        g.with(|c| gmfmod::instance_set_volume(c, again, r(0.25)));
        assert_eq!(g.with(|c| gmfmod::instance_get_volume(c, again)), r(0.25));
        assert_eq!(g.with(|c| gmfmod::instance_stop(c, again)), Real::ONE);
        assert_eq!(g.with(|c| gmfmod::instance_stop(c, again)), Real::ZERO);
        g.with(gmfmod::all_stop);
        assert!(g.w.sound.gmfmod.instances.is_empty());
    }

    #[test]
    fn wp10_gmfmod_streamed_sound_restarts_instead_of_overlapping() {
        let mut g = rig(DllMode::Model);
        g.with(|c| gmfmod::init(c, r(100.0), r(0.0)));
        let streamed = g.with(|c| gmfmod::sound_add(c, "s.mp3", false, true, 0));
        let plain = g.with(|c| gmfmod::sound_add(c, "p.wav", false, false, 0));
        let a = g.with(|c| gmfmod::sound_play(c, streamed, false));
        let b = g.with(|c| gmfmod::sound_play(c, streamed, false));
        assert_ne!(a, b);
        assert_eq!(g.w.sound.gmfmod.instances.len(), 1, "the first was halted");
        g.with(|c| gmfmod::sound_play(c, plain, false));
        g.with(|c| gmfmod::sound_play(c, plain, false));
        assert_eq!(g.w.sound.gmfmod.instances.len(), 3, "a plain sound overlaps");
    }

    #[test]
    fn wp10_gmfmod_instance_limit_and_pause() {
        let mut g = rig(DllMode::Model);
        g.with(|c| gmfmod::init(c, r(2.0), r(0.0)));
        let s = g.with(|c| gmfmod::sound_add(c, "p.wav", false, false, 10));
        let a = g.with(|c| gmfmod::sound_play(c, s, true));
        assert_ne!(a, Real::ZERO);
        assert_eq!(
            g.with(|c| gmfmod::instance_is_playing(c, a)),
            Real::ZERO,
            "started paused"
        );
        g.with(|c| gmfmod::instance_set_paused(c, a, false));
        assert_eq!(g.with(|c| gmfmod::instance_is_playing(c, a)), Real::ONE);
        g.with(|c| gmfmod::sound_play(c, s, false));
        assert_eq!(
            g.with(|c| gmfmod::sound_play(c, s, false)),
            Real::ZERO,
            "limit of 2 reached"
        );
    }

    // ---- Stub mode and CleanMem ----------------------------------------------------------------

    #[test]
    fn wp10_stub_mode_returns_zero_or_empty_and_changes_nothing() {
        let mut g = rig(DllMode::Stub);
        let before = state_hash(&g.w);
        assert_eq!(g.with(supersound::init), "");
        assert_eq!(g.with(|c| supersound::load(c, "a.ogg", true, 0)), "");
        assert_eq!(g.with(|c| supersound::get_volume(c, "1")), "");
        assert_eq!(g.with(|c| supersound::set_volume(c, "1", r(-5.0))), "");
        assert_eq!(g.with(|c| supersound::is_playing(c, "1")), "");
        assert_eq!(g.with(|c| gmfmod::init(c, r(100.0), r(0.0))), Real::ZERO);
        assert_eq!(
            g.with(|c| gmfmod::sound_add(c, "a.ogg", false, false, 0)),
            Real::ZERO
        );
        assert_eq!(g.with(|c| gmfmod::sound_play(c, r(1.0), false)), Real::ZERO);
        assert_eq!(g.with(|c| gmfmod::group_get_volume(c, r(1.0))), Real::ZERO);
        assert_eq!(g.with(|c| gmfmod::group_set_pitch(c, r(1.0), r(0.5))), Real::ZERO);
        assert_eq!(g.with(|c| cleanmem::clean(c)), Real::ZERO);
        assert_eq!(g.with(|c| cleanmem::get_mem(c)), Real::ZERO);
        assert_eq!(state_hash(&g.w), before);
        assert!(g.sound_events().is_empty());
    }

    #[test]
    fn wp10_cleanmem_is_a_no_op_that_reports_success() {
        let mut g = rig(DllMode::Model);
        let before = state_hash(&g.w);
        assert_eq!(g.with(|c| cleanmem::clean(c)), Real::ONE);
        assert!(g.with(|c| cleanmem::get_mem(c)).get() > 0.0);
        assert_eq!(state_hash(&g.w), before);
    }
}
