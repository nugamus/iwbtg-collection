//! WP6b.4 (#270): the `music` built-in against the real sound model (WP10).

mod wp6b {
    use iwbtg_sim::builtins;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{
        Builtin, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, SoundDef, SoundId, SoundKind,
    };
    use iwbtg_sim::events::EventKind;
    use iwbtg_sim::num::Real;
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::{self, DllMode};
    use iwbtg_sim::world::{Ctx, InstId, SpawnInit, World};

    const TRACK: SoundId = SoundId(0);
    const FX: SoundId = SoundId(1);

    fn params(mode: &str, off: bool) -> Params {
        let mut p = Params::default();
        p.0.insert("track".into(), ParamValue::Sound(TRACK));
        p.0.insert("mode".into(), ParamValue::Str(mode.into()));
        p.0.insert("respects_music_off".into(), ParamValue::Bool(off));
        p
    }

    struct G {
        def: GameDef,
        world: World,
        kind: KindId,
    }

    fn game(mode: &str, respects_off: bool) -> G {
        let mut b = GameDefBuilder::new("wp6b-music");
        let mut k = KindDef::new("music", Builtin::Music);
        k.params = params(mode, respects_off);
        let kind = b.kind(k);
        let room = b.room(RoomDef::new("r", 800, 608));
        let mut def = b.build().unwrap();
        for k in &mut def.kinds {
            k.events = builtins::event_set(k.base, &k.clone());
        }
        def.sounds = vec![
            SoundDef {
                name: "track".into(),
                kind: SoundKind::Multimedia,
                length_ticks: 100,
                volume: Real::ONE,
                pan: Real::ZERO,
            },
            SoundDef {
                name: "fx".into(),
                kind: SoundKind::Normal,
                length_ticks: 100,
                volume: Real::ONE,
                pan: Real::ZERO,
            },
        ];
        G {
            def,
            world: World::new(room, 1),
            kind,
        }
    }

    impl G {
        fn with<T>(&mut self, f: impl FnOnce(&mut Ctx) -> T) -> T {
            let rules = RunnerRules::default();
            let scripts = ScriptRuntime::default();
            let mut ctx = Ctx {
                def: &self.def,
                rules: &rules,
                w: &mut self.world,
                scripts: &scripts,
                dll_mode: DllMode::Model,
            };
            f(&mut ctx)
        }

        fn spawn(&mut self) -> InstId {
            let kind = self.kind;
            self.with(|c| {
                c.spawn(kind, Real::ZERO, Real::ZERO, SpawnInit::default())
                    .unwrap()
            })
        }

        fn step(&mut self, id: InstId) {
            self.with(|c| builtins::on_event(c, id, &EventKind::Step).unwrap());
        }

        fn playing(&self, s: SoundId) -> bool {
            sound::is_playing(&self.world, &self.def, s, &RunnerRules::default().sound)
        }

        fn fx(&mut self) {
            self.with(|c| sound::play(c, FX, false).unwrap());
        }
    }

    #[test]
    fn wp6b_music_on_create_plays_the_track_looping() {
        let mut g = game("on_create", false);
        g.spawn();
        assert!(g.playing(TRACK));
        assert!(g.world.sound.copies[0].looping);
    }

    #[test]
    fn wp6b_music_every_step_starts_it_once_and_cuts_effects_only_then() {
        let mut g = game("every_step_if_not_playing", false);
        let id = g.spawn();
        assert!(!g.playing(TRACK), "Create does not play in this mode");
        g.fx();
        g.step(id);
        assert!(g.playing(TRACK));
        assert!(!g.playing(FX), "the stop-all cut the effect");
        g.fx();
        g.step(id);
        assert!(g.playing(FX), "already playing: nothing is stopped");
        assert_eq!(g.world.sound.copies.len(), 2);
    }

    #[test]
    fn wp6b_music_comes_back_after_a_stop_all() {
        let mut g = game("every_step_if_not_playing", false);
        let id = g.spawn();
        g.step(id);
        g.with(sound::stop_all);
        assert!(!g.playing(TRACK));
        g.step(id);
        assert!(g.playing(TRACK));
    }

    #[test]
    fn wp6b_music_respects_music_off_stays_silent_while_muted() {
        let mut g = game("every_step_if_not_playing", true);
        let id = g.spawn();
        g.world.engine.mech.music_muted = true;
        g.step(id);
        assert!(!g.playing(TRACK));
        g.world.engine.mech.music_muted = false;
        g.step(id);
        assert!(g.playing(TRACK));
        let mut c = game("on_create", true);
        c.world.engine.mech.music_muted = true;
        c.spawn();
        assert!(!c.playing(TRACK));
    }

    #[test]
    fn wp6b_music_without_respects_music_off_plays_while_muted() {
        let mut g = game("every_step_if_not_playing", false);
        let id = g.spawn();
        g.world.engine.mech.music_muted = true;
        g.step(id);
        assert!(g.playing(TRACK));
    }
}
