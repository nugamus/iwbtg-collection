//! #288: the package supplies the length of files a game loads through a DLL
//! (`GameDef::dll_sound_length`; docs/format/v0.md, Sounds).

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{GameDef, RoomDef, SoundDef, SoundKind};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::{self, DllMode, gmfmod, supersound};
use iwbtg_sim::world::{Ctx, World};

fn def() -> GameDef {
    let mut b = GameDefBuilder::new("dll-length");
    b.room(RoomDef::new("r", 320, 240));
    let mut def = b.build().unwrap();
    let sd = |name: &str, kind, length_ticks| SoundDef {
        name: name.into(),
        kind,
        length_ticks,
        volume: Real::ONE,
        pan: Real::ZERO,
    };
    def.sounds = vec![
        sd("jump", SoundKind::Normal, 12),
        sd("stage1", SoundKind::Multimedia, 20),
    ];
    def
}

#[test]
fn a_file_name_finds_its_package_sound() {
    let d = def();
    assert_eq!(d.dll_sound_length("stage1.ogg"), 20);
    assert_eq!(d.dll_sound_length("music/stage1.ogg"), 20);
    assert_eq!(
        d.dll_sound_length("music\\Stage1.OGG"),
        20,
        "any separator, any case"
    );
    assert_eq!(d.dll_sound_length("stage1"), 20, "no extension");
    assert_eq!(
        d.dll_sound_length("sounds/jump.wav"),
        12,
        "the extension is not compared"
    );
    assert_eq!(
        d.dll_sound_length("elsewhere/unknown.ogg"),
        0,
        "no such sound: unknown"
    );
    assert_eq!(d.dll_sound_length(""), 0);
}

struct Rig {
    def: GameDef,
    rules: RunnerRules,
    w: World,
}

impl Rig {
    fn new() -> Rig {
        let def = def();
        let w = World::new(def.start_room, 1);
        Rig {
            def,
            rules: RunnerRules::default(),
            w,
        }
    }

    fn with<T>(&mut self, f: impl FnOnce(&mut Ctx) -> T) -> T {
        let scripts = ScriptRuntime::default();
        let mut ctx = Ctx {
            def: &self.def,
            rules: &self.rules,
            w: &mut self.w,
            scripts: &scripts,
            dll_mode: DllMode::Model,
        };
        f(&mut ctx)
    }

    fn tick(&mut self) {
        self.with(sound::tick);
        self.w.tick += 1;
    }
}

#[test]
fn a_one_shot_supersound_file_ends_at_its_package_length() {
    let mut g = Rig::new();
    g.with(supersound::init);
    let h = g.with(|c| {
        let n = c.def.dll_sound_length("music\\stage1.ogg");
        supersound::load(c, "music\\stage1.ogg", true, n)
    });
    g.with(|c| supersound::play(c, &h));
    for _ in 0..19 {
        g.tick();
    }
    assert_eq!(g.with(|c| supersound::is_playing(c, &h)), "1", "19 of 20 ticks");
    g.tick();
    g.tick();
    assert_eq!(
        g.with(|c| supersound::is_playing(c, &h)),
        "0",
        "ended at its 20 ticks"
    );
}

#[test]
fn a_file_with_no_package_length_plays_until_stopped() {
    let mut g = Rig::new();
    g.with(supersound::init);
    let h = g.with(|c| supersound::load(c, "loose.ogg", true, c.def.dll_sound_length("loose.ogg")));
    g.with(|c| supersound::play(c, &h));
    for _ in 0..500 {
        g.tick();
    }
    assert_eq!(g.with(|c| supersound::is_playing(c, &h)), "1");
}

#[test]
fn a_gmfmod_instance_of_a_package_sound_becomes_invalid_at_its_length() {
    let mut g = Rig::new();
    let one = Real::ONE;
    g.with(|c| gmfmod::init(c, Real::new(8.0), Real::ZERO));
    let sound = g.with(|c| {
        let n = c.def.dll_sound_length("jump.ogg");
        gmfmod::sound_add(c, "jump.ogg", false, false, n)
    });
    assert!(sound.get() > 0.0, "a loaded sound has a positive id");
    let inst = g.with(|c| gmfmod::sound_play(c, sound, false));
    assert_eq!(g.with(|c| gmfmod::instance_is_playing(c, inst)), one);
    for _ in 0..11 {
        g.tick();
    }
    assert_eq!(
        g.with(|c| gmfmod::instance_is_playing(c, inst)),
        one,
        "11 of 12 ticks"
    );
    g.tick();
    g.tick();
    assert_eq!(
        g.with(|c| gmfmod::instance_is_playing(c, inst)),
        Real::ZERO,
        "ended at 12"
    );
}
