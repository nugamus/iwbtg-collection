//! WP7b loader tests (docs/plans/sim-core.md 8.2 step 7): physics (current and legacy forms,
//! sprite names, `required` tricks), params against `builtins::param_specs`, timelines, paths,
//! sounds. Packages are built in memory.

use iwbtg_format::load::{LoadMode, LoadReport, load_report};
use iwbtg_format::{PackageFiles, Severity};
use iwbtg_sim::def::{Builtin, GameDef, KidFamily, ParamValue, PartSystemDefId, SoundKind};
use iwbtg_sim::kid::presets;
use iwbtg_sim::num::Real;
use iwbtg_sim::particles::{PartRange, Particles, Region, RegionDistr, RegionShape};
use iwbtg_sim::rng::Rng;

const STRICT: LoadMode = LoadMode::Strict;
const TRACE: LoadMode = LoadMode::Trace {
    unmapped_kinds_inert: true,
};

fn manifest(extra: &str) -> String {
    format!(
        r#"#:schema ../game.schema.json
format = 0
id = "wp7b"
title = "WP7b"
version = "0.1.0"
engine_rev = 0
runner = "gm80"
physics = "physics.toml"
start_room = "stage1"
max_ticks = 180_000
room_order = ["stage1"]
object_order = ["world", "kid", "block", "kid_start", "warp", "kid_bullet", "blood", "blood_emitter", "game_over", "bow", "playerface"]

[engine]
new_game_room = "stage1"
float_model = "f64"
restart = {{ kind = "room_goto_saved", trigger = "step_pressed", excluded_roles = ["title"], boot_ticks = 2, flipped_load_dy = 4 }}
reset = {{ kind = "game_reset" }}

[defaults.kid]
sprite = "kid"
mask = "kid"

[defaults.block]
sprite = "block"

[[creators]]
name = "IWBTG Collection"
role = "author"

[source]
url = "https://example.com/wp7b"
permission = "granted"

[timing]
start = {{ trigger = "run_start" }}
end = {{ trigger = "run_end" }}
stages = [{{ id = "s1", name = "Stage 1", trigger = "enter_room", room = "stage1" }}]
{extra}"#
    )
}

const STAGE1: &str = r#"#:schema ../room.schema.json
size = [800, 608]
file_speed = 30
instances = [
  { kind = "kid_start", x = 48, y = 552 },
  { kind = "block", x = 0, y = 576 },
]
"#;

fn png(rows: &[&str]) -> Vec<u8> {
    let h = u32::try_from(rows.len()).unwrap();
    let w = u32::try_from(rows[0].len()).unwrap();
    let mut px = Vec::new();
    for r in rows {
        for c in r.chars() {
            px.extend_from_slice(&[255, 255, 255, if c == '#' { 255 } else { 0 }]);
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }
    out
}

fn sidecar(w: u32, h: u32) -> Vec<u8> {
    format!(
        "#:schema ../sprite.schema.json\nframes = 1\nframe_size = [{w}, {h}]\norigin = [0, 0]\ncollision = \"rect\"\nbbox = [0, 0, {}, {}]\n",
        w - 1,
        h - 1
    )
    .into_bytes()
}

/// The basic package; `edit` may change or add any file.
fn package(extra_manifest: &str, edit: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> PackageFiles {
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("game.toml".into(), manifest(extra_manifest).into_bytes()),
        ("physics.toml".into(), b"preset = \"A1\"\n".to_vec()),
        ("rooms/stage1.toml".into(), STAGE1.as_bytes().to_vec()),
        ("sprites/kid.toml".into(), sidecar(4, 4)),
        ("sprites/kid.png".into(), png(&["####"; 4])),
        ("sprites/block.toml".into(), sidecar(2, 2)),
        ("sprites/block.png".into(), png(&["##", "##"])),
    ];
    edit(&mut files);
    let mut p = PackageFiles::new();
    for (path, bytes) in files {
        p.insert(path, bytes).unwrap();
    }
    p
}

fn put(files: &mut Vec<(String, Vec<u8>)>, path: &str, bytes: &str) {
    match files.iter_mut().find(|(p, _)| p == path) {
        Some(f) => f.1 = bytes.as_bytes().to_vec(),
        None => files.push((path.to_owned(), bytes.as_bytes().to_vec())),
    }
}

fn render(r: &LoadReport) -> String {
    r.diagnostics.iter().map(|d| format!("{d}\n")).collect()
}

fn errors(r: &LoadReport) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(ToString::to_string)
        .collect()
}

fn warnings(r: &LoadReport) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .map(ToString::to_string)
        .collect()
}

fn load_ok(files: &PackageFiles, mode: LoadMode) -> (GameDef, LoadReport) {
    let r = load_report(files, mode);
    assert!(r.ok(), "load failed:\n{}", render(&r));
    (r.def.clone().unwrap(), r)
}

// ---- physics ----------------------------------------------------------------------------------

#[test]
fn a_current_physics_file_is_read_and_the_preset_applied() {
    let files = package("", |f| {
        put(f, "physics.toml", "preset = \"A1\"\n[numbers]\nrun_speed = 5\n");
    });
    let (def, _) = load_ok(&files, STRICT);
    let mut base = presets::defaults(KidFamily::A1);
    assert_ne!(def.physics, base, "run_speed 5 differs from the preset");
    base.numbers.run_speed = Real::new(5.0);
    assert_eq!(def.physics, base);
}

#[test]
fn physics_mistakes_have_positions() {
    let files = package("", |f| put(f, "physics.toml", "preset = \"A1\"\nbogus = 1\n"));
    let r = load_report(&files, STRICT);
    let e = errors(&r);
    assert_eq!(e.len(), 1, "{}", render(&r));
    assert!(e[0].starts_with("physics.toml:2:1: error:"), "{}", e[0]);
}

#[test]
fn kid_sprite_names_resolve_to_sprite_ids() {
    let files = package("", |f| {
        put(
            f,
            "physics.toml",
            "preset = \"A1\"\n[sprites]\nidle = \"kid\"\nrunning = \"block\"\n",
        );
    });
    let (def, _) = load_ok(&files, STRICT);
    let s = &def.physics.sprites;
    assert!(s.idle.is_some() && s.running.is_some() && s.fall.is_none());
    assert_ne!(s.idle, s.running);
    let files = package("", |f| {
        put(f, "physics.toml", "preset = \"A1\"\n[sprites]\nidle = \"nope\"\n");
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.contains("physics.toml") && e.contains("`nope` does not exist")),
        "{}",
        render(&r)
    );
}

#[test]
fn kid_sound_names_resolve_to_sound_ids() {
    let files = package("", |f| {
        put(f, "sounds/jump.ogg", "ogg");
        put(f, "sounds/death.ogg", "ogg");
        put(
            f,
            "physics.toml",
            "preset = \"A1\"
[sounds]
jump = \"jump\"
death = \"death\"
",
        );
    });
    let (def, _) = load_ok(&files, STRICT);
    let s = &def.physics.sounds;
    assert!(s.jump.is_some() && s.death.is_some() && s.shoot.is_none());
    assert_ne!(s.jump, s.death);
    let files = package("", |f| {
        put(
            f,
            "physics.toml",
            "preset = \"A1\"
[sounds]
shoot = \"nope\"
",
        );
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.contains("physics.toml") && e.contains("sound `nope` does not exist")),
        "{}",
        render(&r)
    );
}

const LEGACY: &str =
    "rules = \"classic\"\njumps = [8.5, 7.0]\ngravity = 0.4\nstep_sets = { gravity = 0.0 }\n";

#[test]
fn a_legacy_profile_maps_to_the_a1_preset_with_a_warning() {
    for mode in [STRICT, TRACE] {
        let files = package("", |f| put(f, "physics.toml", LEGACY));
        let (def, r) = load_ok(&files, mode);
        assert_eq!(def.physics, presets::defaults(KidFamily::A1));
        let w = warnings(&r);
        assert!(
            w.iter().any(|m| m
                .starts_with("physics.toml:1:1: warning: legacy physics profile mapped to the A1")
                && m.contains("jumps")
                && m.contains("step_sets")),
            "{}",
            render(&r)
        );
    }
}

#[test]
fn a_legacy_rules_word_with_no_family_is_an_error_except_in_trace_mode() {
    let files = package("", |f| {
        put(f, "physics.toml", "rules = \"max_jumps\"\njumps = [8.5]\n")
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|e| e.contains("names no family")),
        "{}",
        render(&r)
    );
    let (def, r) = load_ok(&files, TRACE);
    assert_eq!(def.physics, presets::defaults(KidFamily::A1));
    assert!(warnings(&r).iter().any(|e| e.contains("names no family")));
}

#[test]
fn profile_constants_parse_to_the_nearest_double() {
    // N14: Rust parses decimals correctly rounded; gravity 0.4 is the traced value
    // (kaenbyou/stand, 0x3FD999999999999A).
    let files = package("", |f| {
        put(f, "physics.toml", "preset = \"A1\"\n[numbers]\ngravity = 0.4\n")
    });
    let (def, _) = load_ok(&files, STRICT);
    assert_eq!(def.physics.numbers.gravity.get().to_bits(), 0x3FD9_9999_9999_999A);
}

// ---- tricks -----------------------------------------------------------------------------------

#[test]
fn a_required_trick_needs_its_switch_on() {
    let ok = package(
        "\n[tricks]\ndual_shift = \"required\"\nrestart_glitch = \"required\"\nh_align = \"required\"\n",
        |_| {},
    );
    let (_, r) = load_ok(&ok, STRICT);
    assert!(errors(&r).is_empty());
    let bad = package("\n[tricks]\nvdiet = \"required\"\nad_align = \"on\"\n", |_| {});
    let r = load_report(&bad, STRICT);
    let e = errors(&r);
    assert_eq!(e.len(), 1, "{}", render(&r));
    assert!(e[0].starts_with("game.toml:"), "{}", e[0]);
    assert!(e[0].contains("`vdiet`") && e[0].contains("required"), "{}", e[0]);
    let sampled = package("\n[tricks]\nrestart_glitch = \"required\"\n", |f| {
        put(
            f,
            "physics.toml",
            "preset = \"A1\"\n[input]\nwalk = \"sampled\"\n",
        );
    });
    let r = load_report(&sampled, STRICT);
    assert!(
        errors(&r).iter().any(|e| e.contains("direct_latch")),
        "{}",
        render(&r)
    );
}

// ---- params -----------------------------------------------------------------------------------

fn with_objects(files: &[(&str, &str)], room: Option<&str>) -> PackageFiles {
    let files: Vec<(String, String)> = files
        .iter()
        .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
        .collect();
    let room = room.map(str::to_owned);
    package("", move |f| {
        for (p, t) in &files {
            put(f, p, t);
        }
        if let Some(r) = &room {
            put(f, "rooms/stage1.toml", r);
        }
    })
}

#[test]
fn params_are_checked_against_the_built_in_specs() {
    let files = with_objects(
        &[(
            "objects/b1.toml",
            "base = \"spike\"
sprite = \"block\"
bogus = 1
auto_rotate = 3
dir = \"sideways\"
",
        )],
        None,
    );
    let r = load_report(&files, STRICT);
    let e = errors(&r);
    assert!(
        e.iter().all(|m| m.starts_with("objects/b1.toml:")),
        "{}",
        render(&r)
    );
    assert!(
        e.iter().any(|m| m.contains("unknown param `bogus`")),
        "{}",
        render(&r)
    );
    assert!(
        e.iter()
            .any(|m| m.contains("param `auto_rotate` must be true or false")),
        "{}",
        render(&r)
    );
    assert!(
        e.iter()
            .any(|m| m.contains("param `dir` is `sideways`; it must be one of")),
        "{}",
        render(&r)
    );
    // The same problems are warnings for converter output.
    let r = load_report(&files, TRACE);
    assert!(errors(&r).is_empty(), "{}", render(&r));
    assert!(warnings(&r).iter().any(|m| m.contains("unknown param `bogus`")));
}

#[test]
fn a_placed_instance_may_give_what_its_kind_lacks() {
    let room = r#"size = [800, 608]
file_speed = 30
instances = [
  { kind = "kid_start", x = 48, y = 552 },
  { kind = "warp", x = 100, y = 100, room_to = "stage1" },
]
"#;
    let files = with_objects(&[], Some(room));
    let (_, r) = load_ok(&files, STRICT);
    assert!(errors(&r).is_empty());
    let room = r#"size = [800, 608]
file_speed = 30
instances = [
  { kind = "kid_start", x = 48, y = 552 },
  { kind = "warp", x = 100, y = 100 },
]
"#;
    let files = with_objects(&[], Some(room));
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.starts_with("rooms/stage1.toml:") && e.contains("room_to")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_room_param_resolves_to_a_room() {
    let room = r#"size = [800, 608]
file_speed = 30
instances = [
  { kind = "kid_start", x = 48, y = 552 },
  { kind = "warp", x = 100, y = 100, room_to = "stage1", warp_x = 32, warp_y = 64 },
]
"#;
    let files = with_objects(&[], Some(room));
    let (def, _) = load_ok(&files, STRICT);
    let w = def.rooms[0].instances.last().unwrap();
    assert!(matches!(w.params.0.get("room_to"), Some(ParamValue::Room(_))));
}

// ---- timelines, paths, sounds -----------------------------------------------------------------

#[test]
fn timelines_load_in_file_order_with_ticks() {
    let files = package("", |f| {
        put(
            f,
            "timelines/boss.toml",
            "moments = [{ t = 0, call = \"intro\" }, { t = 150, call = \"wave1\" }]\n",
        );
        put(f, "timelines/avoid.toml", "moments = [{ t = 5, call = \"a\" }]\n");
    });
    let (def, _) = load_ok(&files, STRICT);
    let names: Vec<&str> = def.timelines.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["avoid", "boss"], "path order");
    assert_eq!(def.timelines[1].moments, [0, 150]);
}

#[test]
fn timeline_mistakes_are_errors_and_loop_and_calls_reach_the_def() {
    let files = package("", |f| {
        put(
            f,
            "timelines/bad.toml",
            "moments = [{ t = 10, call = \"a\" }, { t = 10, call = \"b\" }]\n",
        );
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|e| e.contains("moments must ascend")),
        "{}",
        render(&r)
    );
    let files = package("", |f| {
        put(f, "timelines/bad.toml", "moments = [{ t = 1, call = \"1a\" }]\n");
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|e| e.contains("not a function name")),
        "{}",
        render(&r)
    );
    let files = package("", |f| {
        put(
            f,
            "timelines/l.toml",
            "loop = true\nmoments = [{ t = 1, call = \"a\" }]\n",
        );
    });
    let (def, r) = load_ok(&files, STRICT);
    assert_eq!(def.timelines.len(), 1);
    assert!(def.timelines[0].looping);
    assert_eq!(def.timelines[0].calls, ["a"]);
    assert!(
        !warnings(&r).iter().any(|e| e.contains("loop")),
        "no loop warning: {}",
        render(&r)
    );
}

#[test]
fn paths_load_with_default_speed_and_precision() {
    let files = package("", |f| {
        put(
            f,
            "paths/p1.toml",
            "points = [[0, 0], [32, 0, 50], [32, 32]]\nclosed = true\n",
        );
    });
    let (def, _) = load_ok(&files, STRICT);
    let p = &def.paths[0];
    assert_eq!(p.name, "p1");
    assert_eq!(p.points.len(), 3);
    assert_eq!(p.points[0].2, Real::new(100.0));
    assert_eq!(p.points[1].2, Real::new(50.0));
    assert!(p.closed && !p.smooth);
    assert_eq!(p.precision, 4);
}

#[test]
fn path_mistakes_are_errors() {
    for text in [
        "points = [[0, 0]]\n",
        "points = [[0, 0], [1]]\n",
        "points = [[0, 0], [1, 1]]\nspeed = 3\n",
    ] {
        let files = package("", |f| put(f, "paths/p1.toml", text));
        let r = load_report(&files, STRICT);
        assert!(!errors(&r).is_empty(), "{text}: {}", render(&r));
        assert!(
            errors(&r).iter().all(|e| e.starts_with("paths/p1.toml:")),
            "{}",
            render(&r)
        );
    }
}

#[test]
fn sounds_and_music_get_kinds_lengths_and_ids() {
    let files = package("", |f| {
        put(f, "sounds/jump.ogg", "ogg");
        put(f, "sounds/jump.toml", "length_ticks = 12\nvolume = 0.5\n");
        put(f, "sounds/death.ogg", "ogg");
        put(f, "music/bgm.ogg", "ogg");
        put(f, "music/bgm.toml", "length_ticks = 5000\npan = -0.25\n");
        put(f, "music/odd.ogg", "ogg");
        put(f, "music/odd.toml", "length_ticks = 1\nkind = \"normal\"\n");
    });
    let (def, r) = load_ok(&files, STRICT);
    let s = |n: &str| def.sounds.iter().find(|s| s.name == n).unwrap();
    assert_eq!(def.sounds.len(), 4);
    assert_eq!((s("jump").kind, s("jump").length_ticks), (SoundKind::Normal, 12));
    assert_eq!(s("jump").volume, Real::new(0.5));
    assert_eq!(
        (s("bgm").kind, s("bgm").length_ticks),
        (SoundKind::Multimedia, 5000)
    );
    assert_eq!(s("bgm").pan, Real::new(-0.25));
    assert_eq!(s("odd").kind, SoundKind::Normal, "a sidecar kind wins");
    assert_eq!(s("death").length_ticks, 0);
    let names: Vec<&str> = def.sounds.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        ["death", "jump", "bgm", "odd"],
        "sounds first, then music, each in path order"
    );
    assert!(
        warnings(&r)
            .iter()
            .any(|w| w.contains("CR9") && w.contains("death") && !w.contains("jump")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_sound_id_used_twice_is_an_error_except_in_trace_mode() {
    let files = package("", |f| {
        put(f, "sounds/death.ogg", "ogg");
        put(f, "music/death.ogg", "ogg");
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.contains("both a sound and a music track")),
        "{}",
        render(&r)
    );
    let (def, _) = load_ok(&files, TRACE);
    assert_eq!(def.sounds.len(), 1);
}

// ---- trace-mode integration (#11) ---------------------------------------------------------------

#[test]
fn built_in_blocks_are_solid_unless_an_object_says_otherwise() {
    let files = package("", |f| {
        put(f, "objects/floor.toml", "base = \"block\"\nsprite = \"block\"\n");
        put(
            f,
            "objects/visual.toml",
            "base = \"block\"\nsprite = \"block\"\nsolid = false\n",
        );
    });
    let (def, _) = load_ok(&files, STRICT);
    let solid = |n: &str| def.kinds.iter().find(|k| k.name == n).unwrap().solid;
    assert!(solid("block"), "the bare built-in");
    assert!(solid("@floor"));
    assert!(!solid("@visual"));
    assert!(!solid("fake_block"));
}

#[test]
fn a_legacy_profile_finds_the_converters_kid_sprites() {
    let files = package("", |f| {
        put(f, "physics.toml", LEGACY);
        put(
            f,
            "sprites/player_idle.toml",
            &String::from_utf8(sidecar(4, 4)).unwrap(),
        );
        f.push(("sprites/player_idle.png".into(), png(&["####"; 4])));
    });
    let (def, _) = load_ok(&files, STRICT);
    assert!(def.physics.sprites.idle.is_some(), "player_idle is found");
    assert!(
        def.physics.sprites.running.is_none(),
        "the others are not in the package"
    );
}

#[test]
fn a_placed_instance_its_built_in_refuses_becomes_inert_in_trace_mode_only() {
    let room = r#"size = [800, 608]
file_speed = 30
instances = [
  { kind = "kid_start", x = 48, y = 552 },
  { kind = "warp", x = 100, y = 100 },
]
"#;
    let files = with_objects(&[], Some(room));
    let r = load_report(&files, STRICT);
    assert!(!errors(&r).is_empty(), "strict mode refuses the warp");
    let (def, r) = load_ok(&files, TRACE);
    let w = def.rooms[0].instances.last().unwrap();
    let k = &def.kinds[w.kind.0 as usize];
    assert_eq!(k.name, "warp[inert]");
    assert_eq!(k.base, iwbtg_sim::def::Builtin::Inert);
    assert!(warnings(&r).iter().any(|m| m.contains("CR5")), "{}", render(&r));
}

// ---- particles (#765) -------------------------------------------------------------------------

const PARTICLES: &str = r##"#:schema ../particles.schema.json
depth = 10

[types.spark]
shape = "flare"
size = [0.5, 1, -0.01, 0]
direction = [0, 360]
life = [20, 20]
color = ["#ffffff", "#ff0000"]
alpha = [1, 0]
additive = true
death = { type = "dust", count = 2 }

[types.dust]
sprite = { id = "block", animate = true, random = true }
speed = [1, 2]
life = [10, 30]

[[emitters]]
region = [0, 800, 0, 608]
shape = "ellipse"
distr = "invgaussian"
stream = { type = "dust", number = -20 }

[[emitters]]
region = [100, 200, 100, 200]
burst = { type = "spark", number = 5 }

[[create]]
at = [400, 304]
type = "spark"
number = 3
"##;

/// The basic package with `files` and `@effect` in `object_order`, placed in stage1.
fn effect_package(files: &[(&str, &str)]) -> PackageFiles {
    let files: Vec<(String, String)> = files
        .iter()
        .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
        .collect();
    package("", move |f| {
        let game = String::from_utf8(f[0].1.clone())
            .unwrap()
            .replace("\"playerface\"]", "\"playerface\", \"@effect\"]");
        put(f, "game.toml", &game);
        put(
            f,
            "rooms/stage1.toml",
            &STAGE1.replace(
                "
]
",
                "
  { kind = \"@effect\", x = 0, y = 0 },
]
",
            ),
        );
        for (p, t) in &files {
            put(f, p, t);
        }
    })
}

#[test]
fn a_particle_file_loads_and_a_particle_system_names_it() {
    let files = with_objects(
        &[
            ("particles/fx.toml", PARTICLES),
            (
                "objects/effect.toml",
                "base = \"particle_system\"\nsystem = \"fx\"\non_room_end = \"destroy\"\n",
            ),
        ],
        None,
    );
    let (def, _) = load_ok(&files, STRICT);
    let [s] = def.particle_systems.as_slice() else {
        panic!("one system: {:?}", def.particle_systems);
    };
    assert_eq!(s.name, "fx");
    assert_eq!(s.depth, Real::new(10.0));
    // Types in name order; step and death types are indexes into that list.
    let names: Vec<&str> = s.types.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["dust", "spark"]);
    let (dust, spark) = (&s.types[0], &s.types[1]);
    assert_eq!(spark.draws.death.map(|c| (c.ty.0, c.count)), Some((0, 2)));
    assert_eq!(spark.draws.size, PartRange::new(0.5, 1.0));
    assert_eq!(spark.look.size_change, [Real::new(-0.01), Real::ZERO]);
    assert_eq!(spark.look.shape.as_deref(), Some("flare"));
    assert_eq!(spark.look.colors, [0xFF_FF_FF, 0xFF_00_00]);
    assert!(spark.look.additive);
    assert!(dust.draws.random_subimage && dust.look.shape.is_none());
    assert_eq!(
        dust.look.sprite.map(|(_, animate, stretch)| (animate, stretch)),
        Some((true, false))
    );
    assert_eq!(dust.draws.life, PartRange::new(10.0, 30.0));
    // Emitters in file order with their regions, streams and bursts.
    assert_eq!(s.emitters.len(), 2);
    assert_eq!(
        s.emitters[0].region,
        Region {
            shape: RegionShape::Ellipse,
            distr: RegionDistr::InvGaussian
        }
    );
    assert_eq!(s.emitters[0].stream, Some((0, -20)));
    assert_eq!(s.emitters[0].burst, None);
    assert_eq!(s.emitters[1].rect, [100.0, 200.0, 100.0, 200.0].map(Real::new));
    assert_eq!(s.emitters[1].burst, Some((1, 5)));
    assert_eq!(s.creates.len(), 1);
    assert_eq!((s.creates[0].ty, s.creates[0].number), (1, 3));
    // The object's `system` param resolves to the file.
    let effect = def
        .kinds
        .iter()
        .find(|k| k.base == Builtin::ParticleSystem && k.name.contains("effect"))
        .expect("effect kind");
    assert_eq!(
        effect.params.0.get("system"),
        Some(&ParamValue::Particles(PartSystemDefId(0)))
    );
    assert!(effect.events.room_end && !effect.events.room_start);
}

/// The built system is what the sim's model builds from the def: the loaded package runs.
#[test]
fn a_loaded_particle_system_builds_in_the_model() {
    let files = with_objects(&[("particles/fx.toml", PARTICLES)], None);
    let (def, _) = load_ok(&files, STRICT);
    let mut p = Particles::default();
    let mut rng = Rng::new(7);
    p.build(&mut rng, &def.particle_systems[0]).unwrap();
    assert_ne!(rng, Rng::new(7), "the burst and the create draw");
    for _ in 0..30 {
        p.update(&mut rng).unwrap();
    }
}

/// A negative burst and a negative death count are chances (#769; CLONE_SPEC numbers "Particle
/// draw rules" 8, research#167), not load errors; they reach the sim as written.
#[test]
fn negative_burst_and_death_counts_load() {
    let text = "[types.a]\nlife = [5, 5]\n[types.b]\nlife = [5, 5]\ndeath = { type = \"a\", count = -5 }\n\
                [[emitters]]\nregion = [0, 1, 0, 1]\nburst = { type = \"b\", number = -10 }\n";
    let files = package("", |f| put(f, "particles/p1.toml", text));
    let (def, _) = load_ok(&files, STRICT);
    let s = &def.particle_systems[0];
    assert_eq!(s.emitters[0].burst, Some((1, -10)));
    assert_eq!(s.types[1].draws.death.map(|c| c.count), Some(-5));
}

/// `system` may list files (rely objEffect's two systems) and `on_room_start_except` lists
/// rooms (go-the-spook-house oRainSys); both resolve each value, and a single value still works
/// (client#826).
#[test]
fn particle_system_list_params_resolve() {
    let files = with_objects(
        &[
            ("particles/fx.toml", PARTICLES),
            ("particles/rain.toml", "depth = -100000\n"),
            (
                "objects/effect.toml",
                "base = \"particle_system\"\nsystem = [\"rain\", \"fx\"]\non_room_start = \"clear\"\n\
                 on_room_start_except = [\"stage1\"]\non_room_start_destroys_instance = true\n",
            ),
        ],
        None,
    );
    let (def, _) = load_ok(&files, STRICT);
    let effect = def
        .kinds
        .iter()
        .find(|k| k.base == Builtin::ParticleSystem && k.name.contains("effect"))
        .expect("effect kind");
    let id = |name: &str| {
        let i = def.particle_systems.iter().position(|s| s.name == name).unwrap();
        ParamValue::Particles(PartSystemDefId(i as u32))
    };
    assert_eq!(
        effect.params.0.get("system"),
        Some(&ParamValue::List(vec![id("rain"), id("fx")]))
    );
    let stage1 = def.rooms.iter().position(|r| r.name == "stage1").unwrap();
    assert_eq!(
        effect.params.0.get("on_room_start_except"),
        Some(&ParamValue::List(vec![ParamValue::Room(iwbtg_sim::def::RoomId(
            stage1 as u32
        ))]))
    );
    assert!(effect.events.room_start);
}

/// Unknown files and rooms in a list, an empty list, and a list on a param that takes one value
/// are load errors (client#826).
#[test]
fn particle_system_list_mistakes_are_errors() {
    for (obj, needle) in [
        ("system = [\"fx\", \"nope\"]\n", "`nope` does not exist"),
        ("system = []\n", "param `system` is an empty list"),
        (
            "system = \"fx\"\non_room_start_except = [\"stage1\", \"nowhere\"]\n",
            "`nowhere` does not exist",
        ),
        (
            "system = \"fx\"\non_room_start_except = []\n",
            "param `on_room_start_except` is an empty list",
        ),
        ("system = \"fx\"\non_room_start = [\"clear\"]\n", "param `on_room_start` must be one of"),
    ] {
        let obj = format!("base = \"particle_system\"\n{obj}");
        let files = effect_package(&[("particles/fx.toml", PARTICLES), ("objects/effect.toml", &obj)]);
        let r = load_report(&files, STRICT);
        assert!(
            errors(&r).iter().any(|m| m.contains(needle)),
            "{obj}\nwanted `{needle}`:\n{}",
            render(&r)
        );
    }
}

/// A script kind's params are single values: a list is an error there (client#826).
#[test]
fn a_script_kind_param_list_is_an_error() {
    let files = with_objects(
        &[
            ("objects/s.toml", "script = \"s\"\nxs = [1, 2]\n"),
            ("scripts/s.rhai", "fn create() {}\n"),
        ],
        None,
    );
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|m| m.contains("param `xs` is a list")),
        "{}",
        render(&r)
    );
}

/// An alpha value may be `{ random = [min, max] }` (go-the-spook-house's rain, client#826): a
/// range in the def, drawing only, next to the fixed values.
#[test]
fn a_random_alpha_loads_as_a_range() {
    let text = "[types.rain]\nalpha = [{ random = [0.12, 0.32] }, 0]\n";
    let files = package("", |f| put(f, "particles/p1.toml", text));
    let (def, _) = load_ok(&files, STRICT);
    assert_eq!(
        def.particle_systems[0].types[0].look.alpha,
        [PartRange::new(0.12, 0.32), PartRange::fixed(0.0)]
    );
}

#[test]
fn particle_mistakes_are_errors() {
    let base = "[types.a]\nlife = [5, 5]\n";
    for (text, needle) in [
        (
            "[[emitters]]\nregion = [0, 1, 0, 1]\nstream = { type = \"b\", number = 1 }\n",
            "particle type `b` is not in this file",
        ),
        (
            "[[emitters]]\nregion = [0, 1, 0, 1]\nstream = { type = \"a\", number = 0 }\n",
            "a stream of 0",
        ),
        (
            "[[emitters]]\nregion = [0, 1, 0, 1]\nburst = { type = \"a\", number = 0 }\n",
            "a burst of 0",
        ),
        (
            "[types.b]\nlife = [5, 5]\ndeath = { type = \"a\", count = 0 }\n",
            "a `death` count of 0",
        ),
        (
            "[types.b]\nlife = [5, 5]\nstep = { type = \"a\", count = -1 }\n",
            "a `step` count of -1",
        ),
        ("[types.b]\nsize = [1, 2, 3]\n", "`size` is [min, max]"),
        (
            "[types.b]\nshape = \"disk\"\nsprite = { id = \"block\" }\n",
            "not both",
        ),
        (
            "[types.b]\nsprite = { id = \"nope\" }\n",
            "sprite `nope` does not exist",
        ),
        ("[types.b]\ncolor = [\"red\"]\n", "is not \"#rrggbb\""),
        (
            "[types.b]\ncolor = [\"#000000\"]\ncolor_mix = [\"#000000\", \"#ffffff\"]\n",
            "set only one of",
        ),
        (
            "[types.b]\nlife = [5, 10]\ndeath = { type = \"a\", count = 1 }\n",
            "need a fixed life",
        ),
        (
            "[types.a2]\nlife = [5, 5]\ndeath = { type = \"a\", count = 1 }\n[types.b]\nlife = [5, 5]\nstep = { type = \"a2\", count = 1 }\n",
            "has step or death particles of its own",
        ),
        (
            "[types.b]\nalpha = [{ random = [0.5, 0.2] }]\n",
            "`alpha` random range has max below min",
        ),
        ("[types.b]\nwiggle = 1\n", "unknown field"),
    ] {
        let text = format!("{base}{text}");
        let files = package("", |f| put(f, "particles/p1.toml", &text));
        let r = load_report(&files, STRICT);
        let e = errors(&r);
        assert!(
            e.iter()
                .any(|m| m.starts_with("particles/p1.toml:") && m.contains(needle)),
            "{text}\nwanted `{needle}`:\n{}",
            render(&r)
        );
    }
}

#[test]
fn a_particle_system_needs_a_known_system() {
    for obj in [
        "base = \"particle_system\"\n",
        "base = \"particle_system\"\nsystem = \"nope\"\n",
    ] {
        let files = effect_package(&[("objects/effect.toml", obj)]);
        let r = load_report(&files, STRICT);
        assert!(
            errors(&r).iter().any(|m| m.contains("system")),
            "{obj}:\n{}",
            render(&r)
        );
    }
}

// ---- room profiles and control labels (#33) ---------------------------------------------------

const STAGE1_LIGHT: &str = r#"#:schema ../room.schema.json
size = [800, 608]
file_speed = 30
physics = "light"
instances = [
  { kind = "kid_start", x = 48, y = 552 },
  { kind = "block", x = 0, y = 576 },
]
"#;

fn stage1(def: &GameDef) -> iwbtg_sim::def::RoomId {
    let at = def.rooms.iter().position(|r| r.name == "stage1").unwrap();
    iwbtg_sim::def::RoomId(u32::try_from(at).unwrap())
}

#[test]
fn a_room_naming_a_profile_uses_it_and_others_use_the_game_profile() {
    let files = package("", |f| {
        put(f, "rooms/stage1.toml", STAGE1_LIGHT);
        put(
            f,
            "physics/light.toml",
            "preset = \"A1\"\n[numbers]\nrun_speed = 5\n",
        );
    });
    let (def, _) = load_ok(&files, STRICT);
    assert_eq!(def.physics, presets::defaults(KidFamily::A1), "the game's stays");
    assert_eq!(def.room_physics.len(), 1);
    assert_eq!(def.room(stage1(&def)).unwrap().physics, Some(0));
    assert_eq!(def.physics_in(stage1(&def)).numbers.run_speed, Real::new(5.0));
}

#[test]
fn a_room_naming_a_missing_profile_is_an_error_at_the_key() {
    let files = package("", |f| put(f, "rooms/stage1.toml", STAGE1_LIGHT));
    let r = load_report(&files, STRICT);
    let e = errors(&r);
    assert_eq!(e.len(), 1, "{}", render(&r));
    assert!(e[0].contains("rooms/stage1.toml:4:"), "{}", e[0]);
    assert!(
        e[0].contains("physics profile `light` has no `physics/light.toml`"),
        "{}",
        e[0]
    );
}

#[test]
fn a_room_profile_with_other_kid_events_is_an_error() {
    let files = package("", |f| {
        put(f, "rooms/stage1.toml", STAGE1_LIGHT);
        put(f, "physics/light.toml", "preset = \"B\"\n");
    });
    let r = load_report(&files, STRICT);
    let e = errors(&r);
    assert!(
        e.iter()
            .any(|m| m.contains("physics/light.toml") && m.contains("handles other kid events")),
        "{}",
        render(&r)
    );
    assert!(
        e.iter().all(|m| !m.contains("  ")),
        "no double space: {}",
        render(&r)
    );
}

#[test]
fn the_game_profile_in_physics_folder_is_shared_not_loaded_twice() {
    let files = package("", |f| {
        let m = manifest("").replace("physics = \"physics.toml\"", "physics = \"physics/main.toml\"");
        put(f, "game.toml", &m);
        put(f, "physics/main.toml", "preset = \"A1\"\n");
        put(
            f,
            "rooms/stage1.toml",
            &STAGE1_LIGHT.replace("\"light\"", "\"main\""),
        );
    });
    let (def, _) = load_ok(&files, STRICT);
    assert!(def.room_physics.is_empty());
    assert_eq!(def.room(stage1(&def)).unwrap().physics, None);
}

#[test]
fn room_profiles_are_in_the_gameplay_hash_and_control_labels_are_not() {
    let hash = |profile: &str, labels: &str| {
        let files = package(labels, |f| {
            put(f, "rooms/stage1.toml", STAGE1_LIGHT);
            put(f, "physics/light.toml", profile);
        });
        let r = load_report(&files, STRICT);
        assert!(r.ok(), "{}", render(&r));
        r.gameplay_hash.unwrap()
    };
    let base = hash("preset = \"A1\"\n", "");
    assert_ne!(base, hash("preset = \"A1\"\n[numbers]\nrun_speed = 5\n", ""));
    assert_eq!(
        base,
        hash("preset = \"A1\"\n", "\n[controls]\nextra1 = \"Teleport\"\n")
    );
}

#[test]
fn control_labels_load_and_bad_ones_are_errors() {
    let ok = package(
        "\n[controls]\nextra1 = \"Teleport\"\nextra3 = \"Nudge right\"\n",
        |_| {},
    );
    let r = load_report(&ok, STRICT);
    assert!(r.ok(), "{}", render(&r));
    let m = iwbtg_format::Manifest::parse(&manifest("\n[controls]\nextra1 = \"Teleport\"\n")).unwrap();
    assert_eq!(m.controls.unwrap().labels(), [Some("Teleport"), None, None, None]);
    for (labels, want) in [
        ("extra2 = \"\"", "controls label extra2 must not be empty"),
        (
            "extra1 = \"A label far too long for the HUD\"",
            "at most 24 characters",
        ),
        (r#"extra1 = "two\tcolumns""#, "one line"),
        ("extra5 = \"Fly\"", "unknown field `extra5`"),
    ] {
        let files = package(&format!("\n[controls]\n{labels}\n"), |_| {});
        let r = load_report(&files, STRICT);
        let e = errors(&r);
        assert!(e.iter().any(|m| m.contains(want)), "{labels}: {}", render(&r));
    }
}
