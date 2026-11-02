//! WP7b loader tests (docs/plans/sim-core.md 8.2 step 7): physics (current and legacy forms,
//! sprite names, `required` tricks), params against `builtins::param_specs`, timelines, paths,
//! sounds. Packages are built in memory.

use iwbtg_format::load::{LoadMode, LoadReport, load_report};
use iwbtg_format::{PackageFiles, Severity};
use iwbtg_sim::def::{GameDef, KidFamily, ParamValue, SoundKind};
use iwbtg_sim::kid::presets;
use iwbtg_sim::num::Real;

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
