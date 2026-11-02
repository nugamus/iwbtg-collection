//! WP7a loader tests (docs/plans/sim-core.md 8.2 steps 1-6, 8, 9; section 12 `format` row):
//! sprites and masks from PNG strips, kinds, `run` expansion order, rank selectors, spawnable
//! kind ranks, sorted variable ids, `[engine]`, `[timing]`, CR3 infra mapping, Trace-mode inert
//! kinds, `gameplay_hash`. Packages are built in memory, so there are no binary fixtures.

use iwbtg_format::load::{LoadMode, LoadReport, load_report};
use iwbtg_format::{PackageFiles, Severity};
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, MaskSet, MaskShape, ParamValue, RestartKind, RoomAction, RoomId,
    RoomRole, VarId,
};
use iwbtg_sim::num::Real;
use iwbtg_sim::timing::TimingTrigger;
use iwbtg_sim::world::Value;

const STRICT: LoadMode = LoadMode::Strict;
const TRACE: LoadMode = LoadMode::Trace {
    unmapped_kinds_inert: true,
};

const ORDER: &str = r#"object_order = [
  "world", "kid", "block", { kind = "spike", dir = "up" }, "kid_start", "warp", "@boss",
  { kind = "spike", dir = "down" }, "spike", "kid_bullet", "blood", "blood_emitter", "game_over",
  "bow", "playerface",
]"#;

fn manifest(order: &str, extra: &str) -> String {
    format!(
        r#"#:schema ../game.schema.json
format = 0
id = "wp7a_basic"
title = "WP7a basic"
version = "0.1.0"
engine_rev = 0
runner = "gm80"
physics = "physics.toml"
start_room = "title"
max_ticks = 180_000
room_order = ["title", "stage1"]
{order}

[engine]
new_game_room = "stage1"
float_model = "f64"
restart = {{ kind = "room_goto_saved", trigger = "step_pressed", excluded_roles = ["title"], boot_ticks = 2, flipped_load_dy = 4 }}
reset = {{ kind = "game_reset" }}

[defaults.kid]
sprite = "kid"
mask = "kid_mask"

[defaults.block]
sprite = "block"

[defaults.spike]
sprite = "spike_up"
variants = [{{ dir = "down", sprite = "spike_down" }}]

[[creators]]
name = "IWBTG Collection"
role = "author"

[source]
url = "https://example.com/wp7a"
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
views_enabled = true
instances = [
  { kind = "kid_start", x = 48, y = 552 },
  { kind = "block", x = 0, y = 576, run = [3, 2], step = [32, -32] },
  { kind = "spike", x = 320, y = 544, dir = "up", group = "trap1" },
  { kind = "spike", x = 352, y = 544, dir = "down" },
  { kind = "spike", x = 384, y = 544, dir = "left", depth = 5 },
  { kind = "@boss", x = 400, y = 200, name = "boss_a" },
  { kind = "warp", x = 768, y = 512, room_to = "title" },
]

[[views]]
visible = true
view = [0, 0, 800, 608]
follow = "kid"
"#;

const TITLE: &str = r#"size = [800, 608]
file_speed = 60
role = "title"
on_create = [{ do = "set_global", var = "difficulty", value = 1 }]
"#;

/// An RGBA PNG of `w` x `h` from rows of `#` (opaque) and `.` (transparent), side by side frames.
fn png(rows: &[&str]) -> Vec<u8> {
    let h = rows.len() as u32;
    let w = rows[0].len() as u32;
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

fn sidecar(frames: u16, w: u32, h: u32, collision: &str, bbox: Option<[i64; 4]>, extra: &str) -> Vec<u8> {
    let bbox = bbox.map_or(String::new(), |b| format!("bbox = [{}, {}, {}, {}]\n", b[0], b[1], b[2], b[3]));
    format!(
        "#:schema ../sprite.schema.json\nframes = {frames}\nframe_size = [{w}, {h}]\norigin = [0, 0]\ncollision = \"{collision}\"\n{bbox}{extra}"
    )
    .into_bytes()
}

/// The basic package; `edit` may change any file before it is loaded.
fn package(edit: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> PackageFiles {
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("game.toml".into(), manifest(ORDER, "").into_bytes()),
        ("physics.toml".into(), b"preset = \"A1\"\n".to_vec()),
        ("rooms/title.toml".into(), TITLE.as_bytes().to_vec()),
        ("rooms/stage1.toml".into(), STAGE1.as_bytes().to_vec()),
        ("objects/boss.toml".into(), b"base = \"killer\"\nscript = \"boss\"\nsprite = \"boss\"\ndepth = -5\ncounts_as = [\"boss_part\", \"solidblock\"]\n".to_vec()),
        ("scripts/boss.rhai".into(), b"fn create() {}\n".to_vec()),
        ("sprites/kid.toml".into(), sidecar(1, 4, 4, "precise", None, "")),
        ("sprites/kid.png".into(), png(&["....", ".##.", ".##.", "...."])),
        ("sprites/kid_mask.toml".into(), sidecar(1, 32, 32, "rect", Some([12, 11, 22, 31]), "")),
        ("sprites/kid_mask.png".into(), png(&[&".".repeat(32); 32])),
        ("sprites/block.toml".into(), sidecar(1, 2, 2, "rect", Some([0, 0, 1, 1]), "")),
        ("sprites/block.png".into(), png(&["##", "##"])),
        ("sprites/spike_up.toml".into(), sidecar(1, 4, 2, "precise", Some([0, 0, 3, 1]), "")),
        ("sprites/spike_up.png".into(), png(&[".##.", "####"])),
        ("sprites/spike_down.toml".into(), sidecar(1, 4, 2, "precise", Some([0, 0, 3, 1]), "")),
        ("sprites/spike_down.png".into(), png(&["####", ".##."])),
        ("sprites/boss.toml".into(), sidecar(2, 2, 1, "precise", None, "separate_masks = true\n")),
        ("sprites/boss.png".into(), png(&["#..#"])),
    ];
    edit(&mut files);
    let mut p = PackageFiles::new();
    for (path, bytes) in files {
        p.insert(path, bytes).unwrap();
    }
    p
}

fn set(files: &mut [(String, Vec<u8>)], path: &str, bytes: impl Into<Vec<u8>>) {
    let bytes = bytes.into();
    match files.iter_mut().find(|(p, _)| p == path) {
        Some(f) => f.1 = bytes,
        None => panic!("no {path}"),
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

fn load_ok(files: &PackageFiles, mode: LoadMode) -> (GameDef, LoadReport) {
    let r = load_report(files, mode);
    assert!(r.ok(), "load failed:\n{}", render(&r));
    (r.def.clone().unwrap(), r)
}

fn kind_named<'d>(def: &'d GameDef, name: &str) -> (iwbtg_sim::def::KindId, &'d iwbtg_sim::def::KindDef) {
    let i = def
        .kinds
        .iter()
        .position(|k| k.name == name)
        .unwrap_or_else(|| panic!("no kind {name}"));
    (iwbtg_sim::def::KindId(i as u32), &def.kinds[i])
}

#[test]
fn basic_package_loads_strict() {
    let (def, report) = load_ok(&package(|_| {}), STRICT);
    assert!(report.gameplay_hash.is_some());

    // Rooms follow room_order; file speed and role come from the room files.
    assert_eq!(def.rooms.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["title", "stage1"]);
    assert_eq!((def.rooms[0].role, def.rooms[0].file_speed), (RoomRole::Title, 60));
    assert_eq!((def.rooms[1].role, def.rooms[1].file_speed), (RoomRole::Play, 30));
    assert_eq!(def.start_room, RoomId(0));
    assert_eq!(def.new_game_room, RoomId(1));

    // `run` expands row by row, in place, never sorted.
    let stage = &def.rooms[1];
    assert_eq!(stage.instances.len(), 12);
    let pos: Vec<(f64, f64)> = stage.instances[1..7].iter().map(|i| (i.x.get(), i.y.get())).collect();
    assert_eq!(
        pos,
        [(0.0, 576.0), (32.0, 576.0), (64.0, 576.0), (0.0, 544.0), (32.0, 544.0), (64.0, 544.0)]
    );

    // Rank selectors: up, down (a derived variant kind with its own sprite) and the plain entry.
    let (spike, spike_def) = kind_named(&def, "spike");
    let (spike_down, down_def) = kind_named(&def, "spike[dir=down]");
    let up = &stage.instances[7];
    let down = &stage.instances[8];
    let left = &stage.instances[9];
    assert_eq!((up.kind, up.rank), (spike, 3));
    assert_eq!((down.kind, down.rank), (spike_down, 7));
    assert_eq!((left.kind, left.rank), (spike, 8));
    assert_ne!(spike_def.sprite, down_def.sprite);
    assert_eq!(def.rank_for(spike_down, None), Some(7));
    assert_eq!(left.params.0.get("depth"), Some(&ParamValue::Real(Real::new(5.0))));
    assert_eq!(spike_def.classes, ClassMask::KILLER);

    // Names and groups.
    assert_eq!(def.names, ["boss_a"]);
    assert_eq!(def.groups, ["trap1"]);
    assert_eq!(stage.instances[10].name.map(|n| n.0), Some(0));
    assert_eq!(up.group.map(|g| g.0), Some(0));

    // The scripted preset.
    let (boss, boss_def) = kind_named(&def, "@boss");
    assert_eq!(boss_def.base, Builtin::Killer);
    assert!(boss_def.script.is_some());
    assert_eq!(def.scripts[0].name, "scripts/boss.rhai");
    assert_eq!(boss_def.depth, Real::new(-5.0));
    assert_eq!(
        boss_def.classes,
        ClassMask::KILLER | ClassMask::SOLIDBLOCK | ClassMask::package(0).unwrap()
    );
    assert_eq!(stage.instances[10].kind, boss);
    let boss_sprite = def.sprite(boss_def.sprite.unwrap()).unwrap();
    assert!(matches!(&boss_sprite.masks, MaskSet::PerFrame(v) if v.len() == 2));

    // Masks: the kid's object mask is the 11 x 21 rectangle; spikes are precise.
    let (_, kid_def) = kind_named(&def, "kid");
    let kid_mask = def.mask(kid_def.mask.unwrap()).unwrap();
    assert_eq!(kid_mask.shape, MaskShape::Rect);
    assert_eq!(
        (kid_mask.bbox.left, kid_mask.bbox.top, kid_mask.bbox.right, kid_mask.bbox.bottom),
        (12, 11, 22, 31)
    );
    let MaskSet::Shared(m) = def.sprite(spike_def.sprite.unwrap()).unwrap().masks else {
        panic!("spike mask is shared");
    };
    match &def.mask(m).unwrap().shape {
        MaskShape::Bits(b) => assert_eq!(b.count(), 6),
        other => panic!("{other:?}"),
    }

    // Variable ids come from the sorted name list.
    let names: Vec<(&str, VarId)> = def.var_names.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    assert_eq!(names, [("depth", VarId(0)), ("difficulty", VarId(1)), ("dir", VarId(2)), ("room_to", VarId(3))]);
    assert_eq!(
        def.rooms[0].on_create,
        [RoomAction::SetGlobal {
            var: VarId(1),
            value: Value::Real(Real::ONE)
        }]
    );

    // Views.
    let (kid, _) = kind_named(&def, "kid");
    assert!(stage.views_enabled);
    assert_eq!(stage.views[0].follow, Some(kid));
    assert_eq!(stage.views[0].hborder, Real::new(32.0));
    assert_eq!(stage.views[0].hspeed, Real::new(-1.0));

    // [engine]: engine kinds, boot, models.
    assert_eq!(def.engine_kind(EngineKind::Kid), Some(kid));
    let (world, _) = kind_named(&def, "world");
    assert_eq!(def.engine_kind(EngineKind::World), Some(world));
    assert_eq!(def.boot, [world]);
    assert_eq!(def.engine_kind(EngineKind::KidFlipped), None);
    assert_eq!(def.restart.kind, RestartKind::RoomGotoSaved);
    assert_eq!(def.restart.boot_ticks, 2);
    assert_eq!(def.restart.flipped_load_dy, Real::new(4.0));
    assert!(def.restart.excluded_roles.contains(RoomRole::Title));
    assert_eq!(def.reset.kind, iwbtg_sim::def::ResetKind::GameReset);

    // [timing].
    assert_eq!(def.timing.start, TimingTrigger::RunStart);
    assert_eq!(def.timing.stages, [TimingTrigger::EnterRoom(RoomId(1))]);

    // The warp keeps its target (a room once the warp's ParamSpec names the type).
    let warp = &stage.instances[11];
    assert!(matches!(
        warp.params.0.get("room_to"),
        Some(ParamValue::Str(s)) if s == "title"
    ) || warp.params.0.get("room_to") == Some(&ParamValue::Room(RoomId(0))));
}

#[test]
fn placed_kind_without_rank_is_an_error() {
    let order = ORDER.replace("\"kid_start\", ", "");
    let files = package(|f| set(f, "game.toml", manifest(&order, "")));
    let r = load_report(&files, STRICT);
    let e = errors(&r);
    assert!(
        e.iter().any(|m| m.contains("`kid_start` is placed but object_order does not list it")),
        "{e:#?}"
    );
}

#[test]
fn spawnable_engine_kinds_need_ranks() {
    for engine in ["bow", "blood", "blood_emitter", "game_over", "playerface", "kid_bullet", "world"] {
        let order = ORDER.replace(&format!("\"{engine}\","), "");
        let files = package(|f| set(f, "game.toml", manifest(&order, "")));
        let r = load_report(&files, STRICT);
        let e = errors(&r);
        assert!(
            e.iter().any(|m| m.contains(&format!("`{engine}` can be spawned but object_order does not list it"))),
            "{engine}: {e:#?}"
        );
        // Trace mode ranks it after the listed entries and names CR4.
        let r = load_report(&files, TRACE);
        assert!(r.ok(), "{}", render(&r));
        assert!(render(&r).contains("CR4"));
    }
    // A game that never spawns one says so.
    let order = ORDER.replace("\"playerface\",", "");
    let files = package(|f| {
        let text = manifest(&order, "").replace("[engine]\n", "[engine]\nroles = { playerface = \"none\" }\n");
        set(f, "game.toml", text);
    });
    let (def, _) = load_ok(&files, STRICT);
    assert_eq!(def.engine_kind(EngineKind::Playerface), None);
}

#[test]
fn missing_script_is_inert_in_trace_mode_only() {
    let files = package(|f| f.retain(|(p, _)| p != "scripts/boss.rhai"));
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|m| m.contains("`scripts/boss.rhai` does not exist")),
        "{}",
        render(&r)
    );
    // The failed kind's later references stay quiet.
    assert_eq!(errors(&r).len(), 1, "{}", render(&r));

    let (def, r) = load_ok(&files, TRACE);
    let (_, boss) = kind_named(&def, "@boss");
    assert_eq!(boss.base, Builtin::Inert);
    assert!(boss.sprite.is_some(), "an inert kind keeps its mask");
    assert!(render(&r).contains("converter request CR1, CR2"), "{}", render(&r));

    let strict_off = LoadMode::Trace {
        unmapped_kinds_inert: false,
    };
    assert!(!load_report(&files, strict_off).ok());
}

#[test]
fn infra_objects_map_to_built_ins_or_are_dropped() {
    let files = package(|f| {
        f.push(("objects/world.toml".into(), b"script = \"world\"\n".to_vec()));
        f.push(("objects/init.toml".into(), b"script = \"init\"\n".to_vec()));
        let order = ORDER.replace("\"world\",", "\"@world\", \"@init\",");
        set(f, "game.toml", manifest(&order, ""));
        set(
            f,
            "rooms/title.toml",
            TITLE.replace("role = \"title\"", "role = \"title\"\ninstances = [{ kind = \"@init\", x = 0, y = 0 }, { kind = \"@world\", x = 0, y = 0 }]"),
        );
    });
    let (def, r) = load_ok(&files, STRICT);
    let (world, w) = kind_named(&def, "@world");
    assert_eq!(w.base, Builtin::World);
    assert!(w.script.is_none());
    assert!(def.kinds.iter().all(|k| k.name != "@init"));
    assert_eq!(def.rooms[0].instances.len(), 1);
    assert_eq!(def.rooms[0].instances[0].kind, world);
    assert_eq!(def.engine_kind(EngineKind::World), Some(world));
    assert!(render(&r).contains("CR3"));
}

#[test]
fn trace_mode_reads_the_trace_map() {
    let files = package(|f| {
        // No file_speed, no role, no defaults: the trace map supplies speed and sprites.
        set(f, "rooms/stage1.toml", STAGE1.replace("file_speed = 30\n", ""));
        let text = manifest(ORDER, "").replace("[defaults.block]\nsprite = \"block\"\n", "");
        set(f, "game.toml", text);
        f.push((
            "tests/trace-map.toml".into(),
            br#"[rooms]
"room1" = { id = "stage1", order = 4, speed = 30 }
[objects]
"block" = { index = 1, kind = "block", status = "builtin", sprite = "block" }
"#
            .to_vec(),
        ));
    });
    let (def, r) = load_ok(&files, TRACE);
    assert_eq!(def.rooms[1].file_speed, 30);
    let (_, block) = kind_named(&def, "block");
    assert!(block.sprite.is_some());
    let text = render(&r);
    assert!(text.contains("CR12") && text.contains("CR13"), "{text}");
    // Strict mode ignores the trace map: the tick rate applies.
    let (def, _) = load_ok(&files, STRICT);
    assert_eq!(def.rooms[1].file_speed, 50);
}

#[test]
fn timing_vocabulary_is_checked() {
    let files = package(|f| {
        let text = manifest(ORDER, "").replace(
            "trigger = \"enter_room\", room = \"stage1\"",
            "trigger = \"boss1_dead\"",
        );
        set(f, "game.toml", text);
    });
    let e = errors(&load_report(&files, STRICT));
    assert!(e.iter().any(|m| m.contains("timing trigger `boss1_dead` is not in the vocabulary")), "{e:#?}");
}

#[test]
fn bad_references_have_positions() {
    let files = package(|f| set(f, "rooms/stage1.toml", STAGE1.replace("\"@boss\"", "\"@nobody\"")));
    let r = load_report(&files, STRICT);
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.message.contains("`@nobody` is not a kind"))
        .unwrap_or_else(|| panic!("{}", render(&r)));
    assert_eq!((d.file.as_str(), d.line), ("rooms/stage1.toml", 11));

    let files = package(|f| set(f, "objects/boss.toml", b"base = \"killer\"\nsprite = \"nope\"\n".to_vec()));
    let r = load_report(&files, STRICT);
    assert!(errors(&r).iter().any(|m| m.starts_with("objects/boss.toml:2:10: error: sprite `nope`")), "{}", render(&r));

    // A strip whose width does not match its sidecar.
    let files = package(|f| set(f, "sprites/boss.toml", sidecar(3, 2, 1, "precise", None, "")));
    let r = load_report(&files, STRICT);
    assert!(errors(&r).iter().any(|m| m.contains("sprites/boss.png")), "{}", render(&r));
}

#[test]
fn gameplay_hash_ignores_comments_and_follows_masks() {
    let hash = |files: &PackageFiles| load_report(files, STRICT).gameplay_hash.expect("loads");
    let base = hash(&package(|_| {}));
    let commented = package(|f| {
        set(f, "rooms/stage1.toml", format!("# a comment\n{STAGE1}"));
        set(f, "game.toml", manifest(ORDER, "").replace("title = \"WP7a basic\"", "title = \"Renamed\""));
    });
    assert_eq!(hash(&commented), base);
    let new_pixel = package(|f| set(f, "sprites/spike_up.png", png(&["###.", "####"])));
    assert_ne!(hash(&new_pixel), base);
    let moved = package(|f| set(f, "rooms/stage1.toml", STAGE1.replace("x = 48", "x = 49")));
    assert_ne!(hash(&moved), base);
}

#[test]
fn load_game_returns_the_def_or_the_errors() {
    assert!(iwbtg_format::load_game(&package(|_| {}), STRICT).is_ok());
    let broken = package(|f| set(f, "rooms/stage1.toml", "size = [0, 0]\n"));
    let errs = iwbtg_format::load_game(&broken, STRICT).unwrap_err();
    assert!(errs.iter().any(|d| d.file == "rooms/stage1.toml"));
}
