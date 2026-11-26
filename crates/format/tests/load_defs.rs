//! Loader tests (docs/architecture/sim/builtins.md; docs/format/physics.md, timelines.md,
//! paths.md, sounds.md): physics (current and legacy forms, sprite names, `required` tricks),
//! params against `builtins::param_specs`, timelines, paths, sounds. Packages are built in memory.

use crate::load_physics::LEGACY;
use iwbtg_format::load::{LoadMode, LoadReport, load_report};
use iwbtg_format::{PackageFiles, Severity};
use iwbtg_sim::def::{GameDef, ParamValue, SoundId, SoundKind};
use iwbtg_sim::num::Real;

pub(crate) const STRICT: LoadMode = LoadMode::Strict;
pub(crate) const TRACE: LoadMode = LoadMode::Trace {
    unmapped_kinds_inert: true,
};

pub(crate) fn manifest(extra: &str) -> String {
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

pub(crate) const STAGE1: &str = r#"#:schema ../room.schema.json
size = [800, 608]
file_speed = 30
instances = [
  { kind = "kid_start", x = 48, y = 552 },
  { kind = "block", x = 0, y = 576 },
]
"#;

pub(crate) fn png(rows: &[&str]) -> Vec<u8> {
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

pub(crate) fn sidecar(w: u32, h: u32) -> Vec<u8> {
    format!(
        "#:schema ../sprite.schema.json\nframes = 1\nframe_size = [{w}, {h}]\norigin = [0, 0]\ncollision = \"rect\"\nbbox = [0, 0, {}, {}]\n",
        w - 1,
        h - 1
    )
    .into_bytes()
}

/// The basic package; `edit` may change or add any file.
pub(crate) fn package(extra_manifest: &str, edit: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> PackageFiles {
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

pub(crate) fn put(files: &mut Vec<(String, Vec<u8>)>, path: &str, bytes: &str) {
    match files.iter_mut().find(|(p, _)| p == path) {
        Some(f) => f.1 = bytes.as_bytes().to_vec(),
        None => files.push((path.to_owned(), bytes.as_bytes().to_vec())),
    }
}

pub(crate) fn render(r: &LoadReport) -> String {
    r.diagnostics.iter().map(|d| format!("{d}\n")).collect()
}

pub(crate) fn errors(r: &LoadReport) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(ToString::to_string)
        .collect()
}

pub(crate) fn warnings(r: &LoadReport) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .map(ToString::to_string)
        .collect()
}

pub(crate) fn load_ok(files: &PackageFiles, mode: LoadMode) -> (GameDef, LoadReport) {
    let r = load_report(files, mode);
    assert!(r.ok(), "load failed:\n{}", render(&r));
    (r.def.clone().unwrap(), r)
}

// ---- mask aliases (#1015) ---------------------------------------------------------------------

/// A precise 2 x 2 sprite sidecar with an optional `mask` alias.
fn precise_sidecar(mask: Option<&str>) -> String {
    format!(
        "#:schema ../sprite.schema.json\nframes = 1\nframe_size = [2, 2]\norigin = [0, 0]\ncollision = \"precise\"\n{}",
        mask.map_or_else(String::new, |m| format!("mask = \"{m}\"\n"))
    )
}

/// The index of `key` in `def.sprites`; the order is the `sprite_pngs` order (`load_objects.rs`).
fn alias_sprite_index(files: &PackageFiles, key: &str) -> usize {
    iwbtg_format::sprites::sprite_pngs(files)
        .iter()
        .position(|(k, _)| k == key)
        .unwrap_or_else(|| panic!("no sprite `{key}`"))
}

/// A package with `referrer -> middle -> terminal`; the three arts differ so a premature copy
/// would be visible.
fn chain_package(terminal: &str, middle: &str, referrer: &str) -> PackageFiles {
    package("", |f| {
        put(f, &format!("sprites/{terminal}.toml"), &precise_sidecar(None));
        f.push((format!("sprites/{terminal}.png"), png(&["#.", ".#"])));
        put(
            f,
            &format!("sprites/{middle}.toml"),
            &precise_sidecar(Some(terminal)),
        );
        f.push((format!("sprites/{middle}.png"), png(&["##", "##"])));
        put(
            f,
            &format!("sprites/{referrer}.toml"),
            &precise_sidecar(Some(middle)),
        );
        f.push((format!("sprites/{referrer}.png"), png(&["..", ".."])));
    })
}

/// Both alias nodes must end at the terminal's own masks, whatever the key order.
fn assert_chain(terminal: &str, middle: &str, referrer: &str) {
    let files = chain_package(terminal, middle, referrer);
    let (def, _) = load_ok(&files, STRICT);
    let t = alias_sprite_index(&files, terminal);
    let m = alias_sprite_index(&files, middle);
    let r = alias_sprite_index(&files, referrer);
    assert_eq!(
        def.sprites[m].masks, def.sprites[t].masks,
        "{middle} must take {terminal}'s masks"
    );
    assert_eq!(
        def.sprites[r].masks, def.sprites[t].masks,
        "{referrer} must take {terminal}'s masks"
    );
}

#[test]
fn a_mask_chain_resolves_to_its_terminal_in_either_key_order() {
    // Referrer first in key order: the old single pass copied the middle's own masks.
    assert_chain("a", "b", "c");
    // Referrer last: renaming keys without changing the graph does not change the topology.
    assert_chain("z", "y", "x");
}

#[test]
fn a_mask_alias_to_a_missing_sprite_is_an_error() {
    let files = package("", |f| {
        put(f, "sprites/a.toml", &precise_sidecar(Some("nope")));
        f.push(("sprites/a.png".into(), png(&["##", "##"])));
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.starts_with("sprites/a.toml:") && e.contains("mask sprite `nope` does not exist")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_self_mask_alias_is_an_error() {
    let files = package("", |f| {
        put(f, "sprites/a.toml", &precise_sidecar(Some("a")));
        f.push(("sprites/a.png".into(), png(&["##", "##"])));
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.starts_with("sprites/a.toml:") && e.contains("uses itself as its mask")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_mask_alias_cycle_is_an_error_at_each_sprite() {
    let files = package("", |f| {
        put(f, "sprites/a.toml", &precise_sidecar(Some("b")));
        f.push(("sprites/a.png".into(), png(&["##", "##"])));
        put(f, "sprites/b.toml", &precise_sidecar(Some("a")));
        f.push(("sprites/b.png".into(), png(&["##", "##"])));
    });
    let r = load_report(&files, STRICT);
    let e = errors(&r);
    assert!(!r.ok());
    assert!(
        e.iter()
            .any(|m| m.starts_with("sprites/a.toml:") && m.contains("a -> b -> a is a cycle")),
        "{}",
        render(&r)
    );
    assert!(
        e.iter()
            .any(|m| m.starts_with("sprites/b.toml:") && m.contains("b -> a -> b is a cycle")),
        "{}",
        render(&r)
    );
}

/// A precise 2 x 2 sidecar with `frames` frames, optional separate masks and an optional alias.
fn frames_sidecar(frames: u32, separate: bool, mask: Option<&str>) -> String {
    format!(
        "#:schema ../sprite.schema.json\nframes = {frames}\nframe_size = [2, 2]\norigin = [0, 0]\ncollision = \"precise\"\nseparate_masks = {separate}\n{}",
        mask.map_or_else(String::new, |m| format!("mask = \"{m}\"\n"))
    )
}

/// A horizontal strip of `frames` 2 x 2 frames.
fn strip(frames: u32) -> Vec<u8> {
    let row = "#.".repeat(usize::try_from(frames).unwrap());
    png(&[row.as_str(), row.as_str()])
}

#[test]
fn a_mask_alias_with_too_few_per_frame_masks_is_an_error() {
    // `t` has 2 separate masks; `a` has 3 frames and aliases it.
    let files = package("", |f| {
        put(f, "sprites/t.toml", &frames_sidecar(2, true, None));
        f.push(("sprites/t.png".into(), strip(2)));
        put(f, "sprites/a.toml", &frames_sidecar(3, true, Some("t")));
        f.push(("sprites/a.png".into(), strip(3)));
    });
    let r = load_report(&files, STRICT);
    let e = errors(&r);
    assert!(!r.ok());
    assert!(
        e.iter().any(|m| m.starts_with("sprites/a.toml:")
            && m.contains("`a`")
            && m.contains("`t`")
            && m.contains("3 frame")
            && m.contains("2 separate mask")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_mask_chain_checks_the_terminal_coverage_for_each_sprite() {
    // `r -> m -> t`: `t` has 2 separate masks, `m` has 2 frames (valid), `r` has 3 (invalid).
    let files = package("", |f| {
        put(f, "sprites/t.toml", &frames_sidecar(2, true, None));
        f.push(("sprites/t.png".into(), strip(2)));
        put(f, "sprites/m.toml", &frames_sidecar(2, true, Some("t")));
        f.push(("sprites/m.png".into(), strip(2)));
        put(f, "sprites/r.toml", &frames_sidecar(3, true, Some("m")));
        f.push(("sprites/r.png".into(), strip(3)));
    });
    let r = load_report(&files, STRICT);
    let e = errors(&r);
    assert!(
        e.iter()
            .any(|m| m.starts_with("sprites/r.toml:") && m.contains("`r`") && m.contains("`t`")),
        "{}",
        render(&r)
    );
    assert!(
        !e.iter().any(|m| m.starts_with("sprites/m.toml:")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_mask_alias_with_enough_frames_loads() {
    // Equal and larger coverage, and a shared target, all cover the sprite.
    for (target_frames, alias_frames, separate) in [(2, 2, true), (3, 2, true), (1, 3, false)] {
        let files = package("", |f| {
            put(
                f,
                "sprites/t.toml",
                &frames_sidecar(target_frames, separate, None),
            );
            f.push(("sprites/t.png".into(), strip(target_frames)));
            put(
                f,
                "sprites/a.toml",
                &frames_sidecar(alias_frames, separate, Some("t")),
            );
            f.push(("sprites/a.png".into(), strip(alias_frames)));
        });
        let (def, _) = load_ok(&files, STRICT);
        let t = alias_sprite_index(&files, "t");
        let a = alias_sprite_index(&files, "a");
        assert_eq!(
            def.sprites[a].masks, def.sprites[t].masks,
            "target {target_frames}, alias {alias_frames}, separate {separate}"
        );
    }
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

pub(crate) fn with_objects(files: &[(&str, &str)], room: Option<&str>) -> PackageFiles {
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

/// #1173: a sound with no sidecar loads with length 0, and the sim plays it until stopped.
#[test]
fn a_sound_without_a_sidecar_plays_until_stopped() {
    use iwbtg_sim::output::OutputEvent;
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::{self, DllMode, SoundOutput, SoundTarget};
    use iwbtg_sim::world::{Ctx, World};

    let files = package("", |f| put(f, "sounds/death.ogg", "ogg"));
    let (def, _) = load_ok(&files, STRICT);
    let id = SoundId(def.sounds.iter().position(|s| s.name == "death").unwrap() as u32);
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::default();
    let mut w = World::new(def.start_room, 1);
    let mut ctx = Ctx {
        def: &def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Model,
    };
    sound::play(&mut ctx, id, false).unwrap();
    for _ in 0..1000 {
        sound::tick(&mut ctx);
        ctx.w.tick += 1;
    }
    assert!(sound::is_playing(ctx.w, &def, id, &rules.sound));
    sound::stop(&mut ctx, id);
    assert!(!sound::is_playing(ctx.w, &def, id, &rules.sound));
    let play: Vec<_> = w
        .output
        .iter()
        .filter(|e| matches!(e, OutputEvent::Sound(SoundOutput::Play { .. })))
        .collect();
    assert_eq!(
        play,
        [&OutputEvent::Sound(SoundOutput::Play {
            target: SoundTarget::Builtin(id),
            looping: false,
            length_ticks: 0,
            tick: 0,
        })]
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
