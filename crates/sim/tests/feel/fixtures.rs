//! Named public fixtures for feel contracts: small engine worlds built in code, with the same
//! [`GameDefBuilder`] and event wiring the golden courses use (`tests/golden/courses.rs`), so a
//! contract runs without the private repos and on every CI target.
//!
//! A fixture is a package in all but name: one play room with a floor, a kid start, and the
//! hazards a contract asks for. Nothing here is creator content.

use iwbtg_sim::builtins;
use iwbtg_sim::def::class::{default_classes, param_classes};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, ParamValue, Params, RoomDef, RoomId,
    RoomRole, SoundDef, SoundId, SoundKind, SpriteId,
};
use iwbtg_sim::kid::{self, presets};
use iwbtg_sim::num::Real;

/// The public fixtures a contract may name as `fixture:<name>`.
///
/// - `jump`: an empty floor room; the kid starts at its left end.
/// - `retry`: the same room with a save point under the kid (a retry restores it).
/// - `collision`: the same room with a spike and a killer overlapping on the floor, so a kid
///   walking right meets both on one tick.
/// - `lifecycle_retry`: the #1149 retry fixture: an engine Boot room, one play room with its
///   music object, and a checkpoint the contract supplies (`StartPoint::FromSave`).
/// - `rng_effects`: the same room with a one-Step blood emitter, so its Step draws the blood's
///   frame/gravity and the emitter's direction/speed (four draws on the shared `gm8` stream).
pub fn fixture(name: &str) -> Result<GameDef, String> {
    match name {
        "jump" => Ok(build("feel-jump", false, false, false, false)),
        "retry" => Ok(build("feel-retry", true, false, false, false)),
        "collision" => Ok(build("feel-collision", true, true, false, false)),
        "sound_events" => Ok(build("feel-sound", false, false, true, false)),
        "rng_effects" => Ok(build("feel-rng-effects", false, false, false, true)),
        "lifecycle_retry" => Ok(lifecycle_retry()),
        other => Err(format!(
            "no public fixture named {other:?} (known: jump, retry, collision, sound_events, rng_effects, lifecycle_retry)"
        )),
    }
}

/// The lifecycle retry fixture from `crates/sim/tests/lifecycle.rs` (#1149): the world boot
/// instance, an engine Boot room, a play room whose music object plays one track, and the A1
/// Kid. The contract's `save` supplies the checkpoint the run starts from.
fn lifecycle_retry() -> GameDef {
    let mut b = GameDefBuilder::new("lifecycle-retry");
    let mut kid = KindDef::new("kid", Builtin::Kid);
    kid.classes = ClassMask::KID;
    let kid_id = b.kind(kid);
    b.engine_kind(EngineKind::Kid, kid_id);
    let mut world = KindDef::new("world", Builtin::World);
    world.persistent = true;
    let world_id = b.kind(world);
    b.boot(world_id);
    let mut boot = RoomDef::new("init", 800, 608);
    boot.role = RoomRole::Boot;
    b.room(boot);
    let play = b.room(RoomDef::new("play", 800, 608));
    b.start_room(play);
    // The play room's music object: its Create plays the track on every entry, so a retry that
    // keeps the music behavior plays the same event when it re-enters the checkpoint's room.
    let music = b.kind({
        let mut k = KindDef::new("music", Builtin::Music);
        let mut p = Params::default();
        p.0.insert("track".into(), ParamValue::Sound(SoundId(0)));
        k.params = p;
        k
    });
    b.place(play, music, 0.0, 0.0).expect("feel fixture place");
    let physics = presets::defaults(KidFamily::A1);
    b.physics(physics.clone());
    let mut def = b.build().expect("lifecycle feel fixture builds");
    for k in &mut def.kinds {
        k.events = if k.base == Builtin::Kid {
            kid::event_set(&physics)
        } else {
            builtins::event_set(k.base, k)
        };
    }
    def.sounds = vec![SoundDef {
        name: "music".into(),
        kind: SoundKind::Multimedia,
        length_ticks: 1_000,
        volume: Real::ONE,
        pan: Real::ZERO,
    }];
    def
}

/// The name of the fixture's only room.
pub const ROOM: &str = "room_0";

fn build(id: &str, save: bool, overlap: bool, sound: bool, rng: bool) -> GameDef {
    let mut b = GameDefBuilder::new(id);
    let tile = b.sprite_rect((32, 32), (0, 0));
    let small = b.sprite_rect((16, 16), (8, 8));
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let room = b.room(RoomDef::new(ROOM, 800, 608));
    b.start_room(room);

    // Engine kinds. The world is the boot instance (the original's rInit): its Create resets
    // the run globals and its Step counts the play timer and runs R/F2.
    let world = kind(&mut b, "world", Builtin::World, &[], tile);
    let kid_kind = kind(&mut b, "kid", Builtin::Kid, &[], kid_sprite);
    let start = kind(&mut b, "kid_start", Builtin::KidStart, &[], small);
    let block = kind(&mut b, "block", Builtin::Block, &[], tile);
    b.def_mut().kinds[block.0 as usize].solid = true;
    b.engine_kind(EngineKind::World, world);
    b.engine_kind(EngineKind::Kid, kid_kind);
    b.boot(world);

    // Floor and walls. The kid start's default offset (17, 23) plus (79, 544) puts the kid on the
    // floor at (96, 567): the 21-high sprite's feet sit on the block top at y = 576.
    for i in 0..25 {
        place(&mut b, room, block, f64::from(i) * 32.0, 576.0);
    }
    for j in 0..18 {
        place(&mut b, room, block, 0.0, f64::from(j) * 32.0);
        place(&mut b, room, block, 768.0, f64::from(j) * 32.0);
    }
    place(&mut b, room, start, 79.0, 544.0);

    if save {
        let save = kind(
            &mut b,
            "save",
            Builtin::Save,
            &[("on_touch", ParamValue::Str("always".into()))],
            small,
        );
        place(&mut b, room, save, 96.0, 567.0);
    }
    if overlap {
        let spike = kind(
            &mut b,
            "spike",
            Builtin::Spike,
            &[("dir", ParamValue::Str("up".into()))],
            tile,
        );
        let killer = kind(&mut b, "killer", Builtin::Killer, &[], tile);
        // One hazard on top of the other: a kid walking right meets both on the same tick.
        place(&mut b, room, spike, 320.0, 544.0);
        place(&mut b, room, killer, 320.0, 544.0);
    }
    if sound {
        // The track plays on the first Step while it is not playing (SHO playMusic001-005).
        let music = kind(
            &mut b,
            "music",
            Builtin::Music,
            &[
                ("track", ParamValue::Sound(SoundId(0))),
                ("mode", ParamValue::Str("every_step_if_not_playing".into())),
            ],
            small,
        );
        place(&mut b, room, music, 0.0, 0.0);
    }
    if rng {
        // One Step of `random_dir`: each drop's Create draws the frame and gravity, then the
        // emitter draws the direction and speed (four draws with `per_step` 1).
        let blood = kind(&mut b, "blood", Builtin::Blood, &[], small);
        let emitter = kind(
            &mut b,
            "blood_emitter",
            Builtin::BloodEmitter,
            &[("per_step", ParamValue::Real(Real::new(1.0)))],
            small,
        );
        b.engine_kind(EngineKind::Blood, blood);
        b.engine_kind(EngineKind::BloodEmitter, emitter);
        place(&mut b, room, emitter, 200.0, 400.0);
    }

    let mut physics = presets::defaults(KidFamily::A1);
    physics.sprites.idle = Some(kid_sprite);
    physics.sprites.running = Some(kid_sprite);
    physics.sprites.jump = Some(kid_sprite);
    physics.sprites.fall = Some(kid_sprite);
    physics.sprites.sliding = Some(kid_sprite);
    physics.sprites.back = Some(kid_sprite);
    physics.sprites.ladder_up_down = Some(kid_sprite);
    b.physics(physics.clone());

    let bad: Vec<String> = {
        let def = b.def_mut();
        def.kinds
            .iter()
            .filter_map(|k| {
                builtins::init_state(k.base, &k.params)
                    .err()
                    .map(|e| format!("{}: {e:?}", k.name))
            })
            .collect()
    };
    assert!(bad.is_empty(), "feel fixture params: {bad:#?}");

    // The loader's event wiring: the kid's set comes from its physics profile, every other
    // built-in's from `builtins::event_set`.
    for k in &mut b.def_mut().kinds {
        k.events = if k.base == Builtin::Kid {
            kid::event_set(&physics)
        } else {
            builtins::event_set(k.base, k)
        };
    }
    let mut def = b.build().expect("feel fixture builds");
    if sound {
        def.sounds = vec![SoundDef {
            name: "track".into(),
            kind: SoundKind::Multimedia,
            length_ticks: 90,
            volume: Real::ONE,
            pan: Real::ZERO,
        }];
    }
    def
}

/// A kind with the loader's classes: param-derived first, else the built-in's defaults.
fn kind(
    b: &mut GameDefBuilder,
    name: &str,
    base: Builtin,
    params: &[(&str, ParamValue)],
    sprite: SpriteId,
) -> iwbtg_sim::def::KindId {
    let mut k = KindDef::new(name, base);
    for (key, value) in params {
        k.params.0.insert((*key).to_owned(), value.clone());
    }
    k.classes = param_classes(base, &k.params).unwrap_or_else(|| default_classes(base));
    k.sprite = Some(sprite);
    b.kind(k)
}

fn place(b: &mut GameDefBuilder, room: RoomId, kind: iwbtg_sim::def::KindId, x: f64, y: f64) {
    b.place(room, kind, x, y).expect("feel fixture place");
}

/// The fixture a contract's `package = "fixture:<name>"` names, or `None` for a package id.
pub fn from_package(package: &str) -> Option<Result<GameDef, String>> {
    package.strip_prefix("fixture:").map(fixture)
}
