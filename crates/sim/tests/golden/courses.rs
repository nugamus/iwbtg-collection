//! Kid courses (#786): one synthetic room per kid family, with the engine kinds, a floor, a
//! kid start and a set of built-ins the kid runs into under a scripted input, plus the
//! script API case. Every `Builtin` and every `KidFamily` appears in at least one case; the
//! coverage lines of the report (`builtin <name> <case>`, `family <name> <case>`) and the
//! `golden_corpus` coverage test keep it that way.
//!
//! Built in code from `iwbtg_sim`'s public API like the rest of the corpus: no package files,
//! no game assets. Sprites are plain rectangles and one triangle.

use iwbtg_sim::builtins;
use iwbtg_sim::def::class::{default_classes, param_classes};
use iwbtg_sim::def::ids::{PartSystemDefId, SoundId};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, KindId, ParamValue, PartEmitterDef,
    PartSystemDef, PartTypeDef, PathDef, RoomDef, RoomId, ScriptDef, ScriptId, SoundDef, SoundKind, SpriteId,
    VarId,
};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::{self, presets};
use iwbtg_sim::particles::{PartRange, PartType};
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;

use super::{Case, Fixture, r};

/// The kid course cases and the script API case, in report order.
pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "kid_a1_basics",
            ticks: 3_000,
            build: || course(KidFamily::A1, a1_objects),
            input: course_input,
        },
        Case {
            name: "kid_a2_water_vines",
            ticks: 3_000,
            build: || course(KidFamily::A2, a2_objects),
            input: course_input,
        },
        Case {
            name: "kid_a3_traps",
            ticks: 3_000,
            build: || course(KidFamily::A3, a3_objects),
            input: course_input,
        },
        Case {
            name: "kid_a4_slopes",
            ticks: 3_000,
            build: || course(KidFamily::A4, a4_objects),
            input: course_input,
        },
        Case {
            name: "kid_a5_gravity",
            ticks: 3_000,
            build: || course(KidFamily::A5, a5_objects),
            input: course_input,
        },
        Case {
            name: "kid_b_items",
            ticks: 3_000,
            build: || course(KidFamily::B, b_objects),
            input: course_input,
        },
        Case {
            name: "kid_c_particles",
            ticks: 3_000,
            build: || course(KidFamily::C, c_objects),
            input: course_input,
        },
        Case {
            name: "script_api",
            ticks: 2_000,
            build: || course(KidFamily::A1, script_objects),
            input: course_input,
        },
    ]
}

// ---------------------------------------------------------------------------------------------
// The scripted input

/// A 64-bit mix of `n` (SplitMix64's finalizer), so each segment gets its own bits.
fn mix(n: u64) -> u64 {
    let mut z = n.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The held actions of tick `t`, in 700-tick cycles that end with R (a retry while alive, the
/// way back after a death). A cycle starts with a 260-tick run to the right that jumps every 28 to
/// 40 ticks, so the kid crosses the room to its far objects and warps; then come 40-tick segments
/// that walk left, right or stand, jump for part of the segment and sometimes hold Up or Down.
/// The kid shoots every 7th tick throughout.
fn course_held(t: u64) -> u32 {
    let cycle = t % 700;
    let mut held = 0;
    if cycle < 260 {
        held |= Action::Right.bit();
        // Each cycle jumps on its own beat, so a spike that kills one run is cleared by another.
        let n = t / 700;
        let period = 28 + 4 * (n % 4);
        if (cycle + n * 11) % period < 14 {
            held |= Action::Jump.bit();
        }
    } else {
        let h = mix(t / 40);
        let at = t % 40;
        match h % 6 {
            0..=2 => held |= Action::Right.bit(),
            3 | 4 => held |= Action::Left.bit(),
            _ => {}
        }
        let jump_from = (h >> 8) % 20;
        let jump_len = 1 + (h >> 16) % 22;
        if (h >> 24) & 1 == 1 && at >= jump_from && at < jump_from + jump_len {
            held |= Action::Jump.bit();
        }
        match (h >> 32) % 6 {
            0 => held |= Action::Up.bit(),
            1 => held |= Action::Down.bit(),
            _ => {}
        }
    }
    if t % 7 == 3 {
        held |= Action::Shoot.bit();
    }
    if cycle == 699 {
        held |= Action::Restart.bit();
    }
    held
}

fn course_input(t: u64) -> InputFrame {
    InputFrame::from_held(if t == 0 { 0 } else { course_held(t - 1) }, course_held(t))
}

// ---------------------------------------------------------------------------------------------
// The course builder

/// What the per-family object lists add to.
pub struct Course {
    b: GameDefBuilder,
    /// The main room and the warp target.
    pub rooms: [RoomId; 2],
    tile: SpriteId,
    small: SpriteId,
    slope: SpriteId,
    /// Scripts to compile, in id order.
    scripts: Vec<ScriptDef>,
}

impl Course {
    /// Adds a kind with params; classes from its base and params (as the loader gives them).
    fn kind(&mut self, name: &str, base: Builtin, params: &[(&str, ParamValue)]) -> KindId {
        let mut k = KindDef::new(name, base);
        for (key, v) in params {
            k.params.0.insert((*key).to_owned(), v.clone());
        }
        k.classes = param_classes(base, &k.params).unwrap_or_else(|| default_classes(base));
        k.sprite = Some(self.tile);
        self.b.kind(k)
    }

    /// A kind and one instance of it in `room` at (`x`, `y`).
    fn put(
        &mut self,
        room: usize,
        x: f64,
        y: f64,
        name: &str,
        base: Builtin,
        params: &[(&str, ParamValue)],
    ) -> KindId {
        let k = self.kind(name, base, params);
        self.at(room, k, x, y);
        k
    }

    /// One more instance of `kind`.
    fn at(&mut self, room: usize, kind: KindId, x: f64, y: f64) {
        self.b.place(self.rooms[room], kind, x, y).expect("golden place");
    }

    fn def_mut(&mut self) -> &mut GameDef {
        self.b.def_mut()
    }
}

fn real(v: f64) -> ParamValue {
    ParamValue::Real(r(v))
}

fn word(s: &str) -> ParamValue {
    ParamValue::Str(s.to_owned())
}

fn sound(def: &mut GameDef, name: &str, kind: SoundKind) -> ParamValue {
    def.sounds.push(SoundDef {
        name: name.to_owned(),
        kind,
        length_ticks: 90,
        volume: r(1.0),
        pan: r(0.0),
    });
    ParamValue::Sound(SoundId(u32::try_from(def.sounds.len() - 1).expect("few sounds")))
}

/// A 32 x 32 right triangle rising to the right, as a precise mask.
fn triangle_rows() -> Vec<String> {
    (0..32)
        .map(|y| {
            let solid = y + 1;
            format!("{}{}", ".".repeat(32 - solid), "#".repeat(solid))
        })
        .collect()
}

/// A family's course: engine kinds, kid sprites, two rooms with a floor, walls and a kid start,
/// then `objects` adds the family's built-ins; events and scripts are wired last.
fn course(family: KidFamily, objects: fn(&mut Course)) -> Fixture {
    let mut b = GameDefBuilder::new("golden-course");
    let tile = b.sprite_rect((32, 32), (0, 0));
    let small = b.sprite_rect((16, 16), (8, 8));
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let rows = triangle_rows();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let slope = b.sprite_bits(&rows, (0, 0));
    let room0 = b.room(RoomDef::new("course", 800, 608));
    let room1 = b.room(RoomDef::new("annex", 800, 608));
    b.start_room(room0);
    let mut c = Course {
        b,
        rooms: [room0, room1],
        tile,
        small,
        slope,
        scripts: Vec::new(),
    };

    // Engine kinds, in the order a package's object_order usually starts with.
    let world = c.kind("world", Builtin::World, &[]);
    let kid = c.kind("kid", Builtin::Kid, &[]);
    let bullet = c.kind("kid_bullet", Builtin::KidBullet, &[]);
    let bow = c.kind("bow", Builtin::Bow, &[]);
    let blood = c.kind("blood", Builtin::Blood, &[]);
    // Fewer drops than a game's 40 per Step for 20 Steps: a death still runs every blood path,
    // and the corpus stays fast in a debug build. Family C's course streams particle blood.
    let emitter_params = if family == KidFamily::C {
        vec![("mode", word("particles")), ("per_step", real(1.0))]
    } else {
        vec![("per_step", real(3.0)), ("life", real(10.0))]
    };
    let emitter = c.kind("blood_emitter", Builtin::BloodEmitter, &emitter_params);
    let game_over = c.kind("game_over", Builtin::GameOver, &[]);
    let face = c.kind("playerface", Builtin::Playerface, &[]);
    let death_sound = c.kind("death_sound", Builtin::DeathSound, &[]);
    {
        let def = c.def_mut();
        def.kinds[kid.0 as usize].sprite = Some(kid_sprite);
        for k in [bullet, blood, bow] {
            def.kinds[k.0 as usize].sprite = Some(small);
        }
    }
    for (role, k) in [
        (EngineKind::World, world),
        (EngineKind::Kid, kid),
        (EngineKind::KidBullet, bullet),
        (EngineKind::Bow, bow),
        (EngineKind::Blood, blood),
        (EngineKind::BloodEmitter, emitter),
        (EngineKind::GameOver, game_over),
        (EngineKind::Playerface, face),
        (EngineKind::DeathSound, death_sound),
    ] {
        c.b.engine_kind(role, k);
    }
    c.b.boot(world);

    // Floor, walls, a ceiling gap, and the kid start in both rooms.
    let block = c.kind("block", Builtin::Block, &[]);
    c.def_mut().kinds[block.0 as usize].solid = true;
    let start = c.kind("kid_start", Builtin::KidStart, &[]);
    for room in 0..2 {
        for i in 0..25 {
            c.at(room, block, f64::from(i) * 32.0, 576.0);
        }
        for j in 0..18 {
            c.at(room, block, 0.0, f64::from(j) * 32.0);
            c.at(room, block, 768.0, f64::from(j) * 32.0);
        }
        // Ledges to land on and bump into.
        for (x, y) in [(192.0, 480.0), (224.0, 480.0), (448.0, 416.0), (608.0, 480.0)] {
            c.at(room, block, x, y);
        }
        c.at(room, start, 48.0, 544.0);
    }

    objects(&mut c);

    let mut physics = presets::defaults(family);
    physics.sprites.idle = Some(kid_sprite);
    physics.sprites.running = Some(kid_sprite);
    physics.sprites.jump = Some(kid_sprite);
    physics.sprites.fall = Some(kid_sprite);
    physics.sprites.sliding = Some(kid_sprite);
    physics.sprites.back = Some(kid_sprite);
    physics.sprites.ladder_up_down = Some(kid_sprite);
    c.b.physics(physics.clone());
    let scripts = std::mem::take(&mut c.scripts);
    let mut def = c.b.build().expect("golden course def");
    // Every kind's params, checked at once (the loader's check), so a bad case names them all.
    let bad: Vec<String> = def
        .kinds
        .iter()
        .filter_map(|k| {
            builtins::init_state(k.base, &k.params)
                .err()
                .map(|e| format!("{}: {e:?}", k.name))
        })
        .collect();
    assert!(bad.is_empty(), "golden course params: {bad:#?}");
    def.scripts = scripts;
    for k in &mut def.kinds {
        k.events = if k.base == Builtin::Kid {
            kid::event_set(&physics)
        } else {
            builtins::event_set(k.base, &k.clone())
        };
    }
    if !def.scripts.is_empty() {
        let mut rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
        rules.num.float_model = def.float_model;
        let runtime = ScriptRuntime::new(&def, &rules).expect("golden scripts compile");
        for k in &mut def.kinds {
            let Some(sid) = k.script else { continue };
            let s = runtime.event_set(sid);
            let e = &mut k.events;
            e.begin_step |= s.begin_step;
            e.step |= s.step;
            e.end_step |= s.end_step;
            e.late |= s.late;
            e.alarms |= s.alarms;
            e.user |= s.user;
            e.outside_room |= s.outside_room;
            e.intersect_boundary |= s.intersect_boundary;
            e.animation_end |= s.animation_end;
            e.room_start |= s.room_start;
            e.room_end |= s.room_end;
            // The package's `collides` list: the mover bounces off blocks.
            if k.name == "mover" {
                e.collisions.push(ClassMask::BLOCK);
            }
        }
    }
    Fixture::started(def, 0x786)
}

// ---------------------------------------------------------------------------------------------
// Objects per family

/// A1: spikes in four directions, a killer that moves, a save, a warp to the annex and back,
/// a platform, the camera and decoration.
fn a1_objects(c: &mut Course) {
    // One spike on the floor; the other directions hang from the ledges and walls.
    for (dir, x, y) in [
        ("up", 320.0, 544.0),
        ("down", 448.0, 448.0),
        ("left", 736.0, 256.0),
        ("right", 32.0, 256.0),
    ] {
        c.put(
            0,
            x,
            y,
            &format!("spike_{dir}"),
            Builtin::Spike,
            &[("dir", word(dir))],
        );
    }
    c.put(
        0,
        520.0,
        300.0,
        "cherry",
        Builtin::Killer,
        &[("hspeed", real(1.5)), ("vspeed", real(0.5))],
    );
    c.put(
        0,
        96.0,
        544.0,
        "save",
        Builtin::Save,
        &[("on_touch", word("always"))],
    );
    c.put(
        0,
        704.0,
        544.0,
        "warp",
        Builtin::Warp,
        &[
            ("room_to", ParamValue::Room(c.rooms[1])),
            ("warp_x", real(80.0)),
            ("warp_y", real(540.0)),
        ],
    );
    c.put(
        1,
        704.0,
        544.0,
        "warp_back",
        Builtin::Warp,
        &[
            ("room_to", ParamValue::Room(c.rooms[0])),
            ("warp_x", real(80.0)),
            ("warp_y", real(540.0)),
        ],
    );
    c.put(0, 384.0, 448.0, "platform", Builtin::Platform, &[]);
    c.put(
        0,
        0.0,
        0.0,
        "camera",
        Builtin::Camera,
        &[("mode", word("screen_snap"))],
    );
    c.put(0, 640.0, 64.0, "decor", Builtin::Decor, &[]);
}

/// A2: water, vines on both sides, a moving platform, fake and hidden blocks, a bullet
/// blocker.
fn a2_objects(c: &mut Course) {
    c.put(0, 320.0, 544.0, "water", Builtin::Water, &[]);
    c.put(
        0,
        352.0,
        544.0,
        "water2",
        Builtin::Water,
        &[("variant", real(2.0))],
    );
    c.put(0, 32.0, 416.0, "vine_l", Builtin::Vine, &[("side", word("left"))]);
    c.put(
        0,
        736.0,
        416.0,
        "vine_r",
        Builtin::Vine,
        &[("side", word("right"))],
    );
    c.put(
        0,
        480.0,
        512.0,
        "moving_platform",
        Builtin::MovingPlatform,
        &[("hspeed", real(1.0)), ("bounce", ParamValue::Bool(true))],
    );
    c.put(0, 160.0, 544.0, "fake_block", Builtin::FakeBlock, &[]);
    c.put(
        0,
        576.0,
        416.0,
        "hidden_block",
        Builtin::HiddenBlock,
        &[("reveal_distance", real(48.0))],
    );
    c.put(0, 256.0, 512.0, "bullet_blocker", Builtin::BulletBlocker, &[]);
}

/// A3: a trigger launches a trap that a trap stopper stops; a drop platform, a breakable
/// block and a jump refresher.
fn a3_objects(c: &mut Course) {
    let trigger = c.put(
        0,
        256.0,
        544.0,
        "trigger",
        Builtin::Trigger,
        &[("trg", real(1.0))],
    );
    c.put(
        0,
        384.0,
        320.0,
        "trap",
        Builtin::Trap,
        &[
            ("trigger_kind", ParamValue::Kind(trigger)),
            ("trigger_id", real(1.0)),
            ("vspeed", real(3.0)),
            ("stop_on_stopper", word("zero_speed")),
        ],
    );
    c.put(0, 384.0, 448.0, "trap_stopper", Builtin::TrapStopper, &[]);
    c.put(
        0,
        512.0,
        512.0,
        "drop_platform",
        Builtin::DropPlatform,
        &[("mode", word("fall"))],
    );
    c.put(
        0,
        128.0,
        512.0,
        "breakable_block",
        Builtin::BreakableBlock,
        &[("break_on", word("shot"))],
    );
    c.put(0, 320.0, 480.0, "jump_refresher", Builtin::JumpRefresher, &[]);
}

/// A4: slopes both ways, a conveyor, a spring, a kid parameter zone and a ladder.
fn a4_objects(c: &mut Course) {
    let slope = c.slope;
    let up = c.put(
        0,
        288.0,
        544.0,
        "slope",
        Builtin::Slope,
        &[("shape", word("rise_right"))],
    );
    let down = c.put(
        0,
        352.0,
        544.0,
        "slope_left",
        Builtin::Slope,
        &[("shape", word("rise_left"))],
    );
    for k in [up, down] {
        c.def_mut().kinds[k.0 as usize].sprite = Some(slope);
    }
    c.put(
        0,
        128.0,
        544.0,
        "conveyor",
        Builtin::Conveyor,
        &[("h", real(1.0))],
    );
    c.put(0, 544.0, 544.0, "spring", Builtin::Spring, &[]);
    c.put(
        0,
        416.0,
        512.0,
        "param_zone",
        Builtin::KidParamZone,
        &[("run_speed", real(4.0))],
    );
    c.put(0, 640.0, 448.0, "ladder", Builtin::Ladder, &[]);
    c.put(0, 640.0, 480.0, "ladder", Builtin::Ladder, &[]);
}

/// A5: gravity flippers both ways, an edge warp, an orbiting killer, a path platform and a
/// time limit.
fn a5_objects(c: &mut Course) {
    c.put(
        0,
        256.0,
        512.0,
        "flip_up",
        Builtin::GravityFlip,
        &[("direction", word("up"))],
    );
    c.put(
        0,
        256.0,
        64.0,
        "flip_down",
        Builtin::GravityFlip,
        &[("direction", word("down"))],
    );
    c.put(
        0,
        0.0,
        0.0,
        "edge_warp",
        Builtin::EdgeWarp,
        &[
            ("room_to", ParamValue::Room(c.rooms[1])),
            ("warp_x", real(80.0)),
            ("warp_y", real(540.0)),
        ],
    );
    c.put(
        1,
        704.0,
        544.0,
        "warp_back",
        Builtin::Warp,
        &[
            ("room_to", ParamValue::Room(c.rooms[0])),
            ("warp_x", real(80.0)),
            ("warp_y", real(540.0)),
        ],
    );
    c.put(
        0,
        560.0,
        300.0,
        "orbit",
        Builtin::Orbit,
        &[
            ("center_x", real(600.0)),
            ("center_y", real(300.0)),
            ("speed", real(3.0)),
        ],
    );
    c.def_mut().paths.push(PathDef {
        name: "loop".into(),
        points: vec![
            (r(400.0), r(400.0), r(100.0)),
            (r(560.0), r(400.0), r(100.0)),
            (r(560.0), r(320.0), r(100.0)),
        ],
        closed: true,
        smooth: false,
        precision: 4,
    });
    c.put(
        0,
        400.0,
        400.0,
        "path_platform",
        Builtin::PathPlatform,
        &[
            ("path", real(0.0)),
            ("speed", real(2.0)),
            ("path_end", word("restart")),
        ],
    );
    c.put(
        0,
        0.0,
        0.0,
        "time_limit",
        Builtin::TimeLimit,
        &[("seconds", real(25.0))],
    );
}

/// B: music, a boss gate and item, a secret item, an item viewer, a sign, a difficulty warp to
/// the annex, and a game clear the course never reaches (it would end the run). Family B's
/// death runs the `death_sound` engine kind.
fn b_objects(c: &mut Course) {
    let track = sound(c.def_mut(), "music", SoundKind::Multimedia);
    c.put(0, 0.0, 0.0, "music", Builtin::Music, &[("track", track)]);
    c.put(
        0,
        320.0,
        448.0,
        "boss_gate",
        Builtin::BossGate,
        &[("boss_index", real(1.0))],
    );
    c.put(
        0,
        352.0,
        544.0,
        "boss_item",
        Builtin::BossItem,
        &[("boss_index", real(1.0))],
    );
    c.put(
        0,
        160.0,
        544.0,
        "secret_item",
        Builtin::SecretItem,
        &[("item_index", real(1.0))],
    );
    c.put(
        0,
        64.0,
        64.0,
        "item_viewer",
        Builtin::ItemViewer,
        &[("kind", word("item")), ("index", real(1.0))],
    );
    c.put(
        0,
        512.0,
        544.0,
        "sign",
        Builtin::Sign,
        &[("text", word("golden"))],
    );
    c.put(
        0,
        704.0,
        544.0,
        "difficulty_warp",
        Builtin::DifficultyWarp,
        &[("dif", real(1.0)), ("room_to", ParamValue::Room(c.rooms[1]))],
    );
    c.put(
        1,
        704.0,
        544.0,
        "warp_back",
        Builtin::Warp,
        &[
            ("room_to", ParamValue::Room(c.rooms[0])),
            ("warp_x", real(80.0)),
            ("warp_y", real(540.0)),
        ],
    );
    c.put(0, 400.0, -200.0, "game_clear", Builtin::GameClear, &[]);
    // Spikes, so the kid dies and family B's death creates the `death_sound` engine kind.
    c.put(0, 448.0, 544.0, "spike", Builtin::Spike, &[]);
}

/// C: a particle system that streams every tick, spikes to die on, a save and a playerface.
fn c_objects(c: &mut Course) {
    let def = c.def_mut();
    def.particle_systems.push(PartSystemDef {
        name: "rain".into(),
        types: vec![PartTypeDef {
            name: "drop".into(),
            draws: PartType {
                size: PartRange::new(1.0, 2.0),
                speed: PartRange::new(1.0, 3.0),
                direction: PartRange::new(250.0, 290.0),
                life: PartRange::new(20.0, 40.0),
                ..PartType::default()
            },
            look: Default::default(),
        }],
        emitters: vec![PartEmitterDef {
            rect: [r(0.0), r(800.0), r(0.0), r(16.0)],
            region: Default::default(),
            stream: Some((0, 2)),
            burst: Some((0, 10)),
        }],
        creates: Vec::new(),
        depth: r(0.0),
    });
    c.put(
        0,
        0.0,
        0.0,
        "particles",
        Builtin::ParticleSystem,
        &[
            ("system", ParamValue::Particles(PartSystemDefId(0))),
            ("on_room_end", word("destroy")),
        ],
    );
    c.put(0, 416.0, 544.0, "spike", Builtin::Spike, &[]);
    c.put(
        0,
        96.0,
        544.0,
        "save",
        Builtin::Save,
        &[("on_touch", word("always"))],
    );
    // The death screen face is placed by packages, not spawned by the engine.
    c.put(0, 400.0, 240.0, "face", Builtin::Playerface, &[]);
}

/// The script API case: a `scripted` controller that calls every function of the script API
/// (`script_api.rhai`), and a `scripted` mover it creates and steers.
fn script_objects(c: &mut Course) {
    sound(c.def_mut(), "blip", SoundKind::Normal);
    sound(c.def_mut(), "theme", SoundKind::Multimedia);
    c.def_mut().var_names.insert("hp".into(), VarId(1));
    c.def_mut().var_names.insert("phase".into(), VarId(2));
    c.scripts.push(ScriptDef {
        name: "scripts/api.rhai".into(),
        source: include_str!("script_api.rhai").into(),
    });
    c.scripts.push(ScriptDef {
        name: "scripts/mover.rhai".into(),
        source: include_str!("script_mover.rhai").into(),
    });
    let api = c.kind("api", Builtin::Scripted, &[]);
    let mover = c.kind("mover", Builtin::Scripted, &[]);
    let small = c.small;
    let def = c.def_mut();
    def.kinds[api.0 as usize].script = Some(ScriptId(0));
    def.kinds[mover.0 as usize].script = Some(ScriptId(1));
    def.kinds[mover.0 as usize].sprite = Some(small);
    c.at(0, api, 400.0, 200.0);
    c.at(1, api, 401.0, 200.0);
    c.put(0, 480.0, 544.0, "spike", Builtin::Spike, &[]);
}
