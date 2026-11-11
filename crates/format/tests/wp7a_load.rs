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
    let bbox = bbox.map_or(String::new(), |b| {
        format!("bbox = [{}, {}, {}, {}]\n", b[0], b[1], b[2], b[3])
    });
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
        ("sprites/kid_mask.png".into(), { let row = ".".repeat(32); png(&[row.as_str(); 32]) }),
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
    assert_eq!(
        def.rooms.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["title", "stage1"]
    );
    assert_eq!(
        (def.rooms[0].role, def.rooms[0].file_speed),
        (RoomRole::Title, 60)
    );
    assert_eq!((def.rooms[1].role, def.rooms[1].file_speed), (RoomRole::Play, 30));
    assert_eq!(def.start_room, RoomId(0));
    assert_eq!(def.new_game_room, RoomId(1));

    // `run` expands row by row, in place, never sorted.
    let stage = &def.rooms[1];
    assert_eq!(stage.instances.len(), 12);
    let pos: Vec<(f64, f64)> = stage.instances[1..7]
        .iter()
        .map(|i| (i.x.get(), i.y.get()))
        .collect();
    assert_eq!(
        pos,
        [
            (0.0, 576.0),
            (32.0, 576.0),
            (64.0, 576.0),
            (0.0, 544.0),
            (32.0, 544.0),
            (64.0, 544.0)
        ]
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
    assert_eq!(
        left.params.0.get("depth"),
        Some(&ParamValue::Real(Real::new(5.0)))
    );
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
        (
            kid_mask.bbox.left,
            kid_mask.bbox.top,
            kid_mask.bbox.right,
            kid_mask.bbox.bottom
        ),
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
    assert_eq!(
        names,
        [
            ("depth", VarId(0)),
            ("difficulty", VarId(1)),
            ("dir", VarId(2)),
            ("room_to", VarId(3))
        ]
    );
    assert_eq!(
        def.rooms[0].on_create,
        [RoomAction::SetGlobal {
            var: VarId(1),
            value: Value::Real(Real::ONE),
            difficulty: Some(1)
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
    assert!(
        matches!(
            warp.params.0.get("room_to"),
            Some(ParamValue::Str(s)) if s == "title"
        ) || warp.params.0.get("room_to") == Some(&ParamValue::Room(RoomId(0)))
    );
}

#[test]
fn placed_kind_without_rank_is_an_error() {
    let order = ORDER.replace("\"kid_start\", ", "");
    let files = package(|f| set(f, "game.toml", manifest(&order, "")));
    let r = load_report(&files, STRICT);
    let e = errors(&r);
    assert!(
        e.iter()
            .any(|m| m.contains("`kid_start` is placed but object_order does not list it")),
        "{e:#?}"
    );
}

#[test]
fn spawnable_engine_kinds_need_ranks() {
    for engine in [
        "bow",
        "blood",
        "blood_emitter",
        "game_over",
        "playerface",
        "kid_bullet",
        "world",
    ] {
        let order = ORDER.replace(&format!("\"{engine}\","), "");
        let files = package(|f| set(f, "game.toml", manifest(&order, "")));
        let r = load_report(&files, STRICT);
        let e = errors(&r);
        assert!(
            e.iter().any(|m| m.contains(&format!(
                "`{engine}` can be spawned but object_order does not list it"
            ))),
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
        let text =
            manifest(&order, "").replace("[engine]\n", "[engine]\nroles = { playerface = \"none\" }\n");
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
        errors(&r)
            .iter()
            .any(|m| m.contains("`scripts/boss.rhai` does not exist")),
        "{}",
        render(&r)
    );
    // The failed kind's later references stay quiet.
    assert_eq!(errors(&r).len(), 1, "{}", render(&r));

    let (def, r) = load_ok(&files, TRACE);
    let (_, boss) = kind_named(&def, "@boss");
    assert_eq!(boss.base, Builtin::Inert);
    assert!(boss.sprite.is_some(), "an inert kind keeps its mask");
    assert!(
        render(&r).contains("converter request CR1, CR2"),
        "{}",
        render(&r)
    );

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
    assert!(
        e.iter()
            .any(|m| m.contains("timing trigger `boss1_dead` is not in the vocabulary")),
        "{e:#?}"
    );
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

    let files = package(|f| {
        set(
            f,
            "objects/boss.toml",
            b"base = \"killer\"\nsprite = \"nope\"\n".to_vec(),
        )
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|m| m.starts_with("objects/boss.toml:2:10: error: sprite `nope`")),
        "{}",
        render(&r)
    );

    // A strip whose width does not match its sidecar.
    let files = package(|f| set(f, "sprites/boss.toml", sidecar(3, 2, 1, "precise", None, "")));
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|m| m.contains("sprites/boss.png")),
        "{}",
        render(&r)
    );
}

#[test]
fn gameplay_hash_ignores_comments_and_follows_masks() {
    let hash = |files: &PackageFiles| load_report(files, STRICT).gameplay_hash.expect("loads");
    let base = hash(&package(|_| {}));
    let commented = package(|f| {
        set(f, "rooms/stage1.toml", format!("# a comment\n{STAGE1}"));
        set(
            f,
            "game.toml",
            manifest(ORDER, "").replace("title = \"WP7a basic\"", "title = \"Renamed\""),
        );
    });
    assert_eq!(hash(&commented), base);
    // Key art is cosmetic (#491): adding `[key_art]` and its image keeps the hash.
    let key_art = package(|f| {
        set(
            f,
            "game.toml",
            format!(
                "{}
[key_art]
image = \"art/key.png\"
",
                manifest(ORDER, "")
            ),
        );
        f.push(("art/key.png".into(), png(&["#"])));
    });
    assert_eq!(hash(&key_art), base);
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

// ---- #234: tier 1 built-in params ---------------------------------------------------------

#[test]
fn tier1_params_load_and_the_block_collider_resolves_to_a_kind() {
    let files = package(|f| {
        f.push((
            "objects/rely_block.toml".into(),
            b"base = \"block\"\nsprite = \"block\"\nvisual_spawns_collider = true\ncollider = \"block\"\nkid_collides = false\n".to_vec(),
        ));
        f.push((
            "objects/start_b.toml".into(),
            b"base = \"kid_start\"\nshift_at = \"room_start\"\ngrav_gated = false\ncreates = \"player_or_player2\"\n".to_vec(),
        ));
        f.push((
            "objects/bullet_b.toml".into(),
            b"base = \"kid_bullet\"\nblock_check = \"collision_destroy\"\n".to_vec(),
        ));
        f.push((
            "objects/spike_b.toml".into(),
            b"base = \"spike\"\nsprite = \"spike_up\"\nauto_rotate = true\nsize = 16\n".to_vec(),
        ));
    });
    let (def, _) = load_ok(&files, TRACE);
    let (block_id, plain) = kind_named(&def, "block");
    assert_eq!(plain.classes, ClassMask::BLOCK);
    let (_, rely) = kind_named(&def, "@rely_block");
    assert_eq!(
        rely.params.0.get("collider"),
        Some(&ParamValue::Kind(block_id)),
        "a kind param names a kind"
    );
    assert_eq!(
        rely.params.0.get("visual_spawns_collider"),
        Some(&ParamValue::Bool(true))
    );
    assert_eq!(rely.classes, ClassMask::NONE, "kid_collides = false drops BLOCK");
    let (_, start) = kind_named(&def, "@start_b");
    assert_eq!(start.params.0.get("grav_gated"), Some(&ParamValue::Bool(false)));
    assert_eq!(
        start.params.0.get("shift_at"),
        Some(&ParamValue::Str("room_start".into()))
    );
    let (_, bullet) = kind_named(&def, "@bullet_b");
    assert_eq!(
        bullet.params.0.get("block_check"),
        Some(&ParamValue::Str("collision_destroy".into()))
    );
    let (_, spike) = kind_named(&def, "@spike_b");
    assert_eq!(spike.params.0.get("auto_rotate"), Some(&ParamValue::Bool(true)));
}

#[test]
fn a_block_collider_that_does_not_exist_is_an_error() {
    let files = package(|f| {
        f.push((
            "objects/rely_block.toml".into(),
            b"base = \"block\"\nvisual_spawns_collider = true\ncollider = \"@nope\"\n".to_vec(),
        ));
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|e| e.contains("collider")),
        "{}",
        render(&r)
    );
}

// ---- #293: tier 2 built-in params (trigger, trap, fake_block, hidden_block, world) ------------

/// The basic package with the WP6b.2 kinds and every object in `objects` (`(file stem, text)`)
/// listed in `object_order`. `my_world` is added with its default text unless given, and is the
/// game's `world` role when `world_role` is set.
fn wp6b2_package(objects: &[(&str, &str)], world_role: bool) -> PackageFiles {
    let mut stems: Vec<&str> = objects.iter().map(|(s, _)| *s).collect();
    if !stems.contains(&"my_world") {
        stems.push("my_world");
    }
    let listed: String = stems.iter().map(|s| format!("\"@{s}\", ")).collect();
    let order = ORDER.replace(
        "\"playerface\",",
        &format!("\"playerface\", \"trigger\", \"trap\", \"fake_block\", \"hidden_block\", {listed}"),
    );
    let mut manifest = manifest(&order, "");
    if world_role {
        manifest = manifest.replace(
            "[engine]
",
            "[engine]
roles = { world = \"@my_world\" }
",
        );
    }
    package(|f| {
        set(f, "game.toml", manifest.into_bytes());
        if !objects.iter().any(|(s, _)| *s == "my_world") {
            f.push(("objects/my_world.toml".into(), MY_WORLD.as_bytes().to_vec()));
        }
        for (stem, text) in objects {
            f.push((format!("objects/{stem}.toml"), text.as_bytes().to_vec()));
        }
    })
}

const MY_WORLD: &str = "base = \"world\"\nlatch_cleared_by_world = true\npause = \"deactivate\"\npause_delay = 5\nmute = true\ntimer_per_tick = 2\ntimer_on_restart = 2\nclear_triggers_to = 10\nclear_switches_on_room_end = true\n";

#[test]
fn wp6b2_params_load() {
    let files = wp6b2_package(
        &[
            ("my_world", MY_WORLD),
            (
                "free_trigger",
                "base = \"trigger\"\nmodel = \"per_object\"\ntrg = 3\ngroup = \"@flag_trigger\"\none_shot_destroy = true\nsets_frame = true\nsize = 16\n",
            ),
            (
                "flag_trigger",
                "base = \"trigger\"\nmodel = \"flag_array\"\ntrg = 7\nneed_key = 2\narray = \"stop\"\nreset = true\n",
            ),
            (
                "my_trap",
                concat!(
                    "base = \"trap\"\ntrigger_id = 3\ntrigger_model = \"per_object\"\n",
                    "trigger_kind = \"@free_trigger\"\ntrigger_kind_b = \"@flag_trigger\"\n",
                    "launch = \"path\"\npath = 0\npath_end = \"reverse\"\npath_speed = 2.5\n",
                    "stop_on_stopper = \"flush\"\nflag_guard = true\nkind = \"killer\"\n",
                ),
            ),
            (
                "my_fake",
                "base = \"fake_block\"\nvanish_on = \"touch\"\napproach_distance = 4\nblood_solid_when_no_kid = true\nstops_bullets = true\nreverses_platforms = true\nsolid = false\nvisible = true\n",
            ),
            (
                "my_hidden",
                "base = \"hidden_block\"\nreveal_distance = 1\nstops_bullets = true\ndestroy_when_no_kid = false\ndestroyed_by_bullet = true\ndestroyed_by_blood = true\n",
            ),
        ],
        true,
    );
    let (def, _) = load_ok(&files, STRICT);
    let word = |s: &str| ParamValue::Str(s.into());
    let real = |v: f64| ParamValue::Real(Real::new(v));
    let yes = ParamValue::Bool(true);

    let (_, w) = kind_named(&def, "@my_world");
    assert_eq!(w.params.0.get("pause"), Some(&word("deactivate")));
    for (k, v) in [
        ("pause_delay", 5.0),
        ("timer_per_tick", 2.0),
        ("timer_on_restart", 2.0),
        ("clear_triggers_to", 10.0),
    ] {
        assert_eq!(w.params.0.get(k), Some(&real(v)), "{k}");
    }
    for k in ["latch_cleared_by_world", "mute", "clear_switches_on_room_end"] {
        assert_eq!(w.params.0.get(k), Some(&yes), "{k}");
    }
    assert_eq!(
        def.engine_kind(EngineKind::World),
        Some(kind_named(&def, "@my_world").0),
        "the object is the game's world"
    );

    let (flag_id, flag) = kind_named(&def, "@flag_trigger");
    assert_eq!(flag.params.0.get("model"), Some(&word("flag_array")));
    assert_eq!(flag.params.0.get("array"), Some(&word("stop")));
    assert_eq!(flag.params.0.get("need_key"), Some(&real(2.0)));
    assert_eq!(flag.params.0.get("reset"), Some(&yes));
    let (free_id, free) = kind_named(&def, "@free_trigger");
    assert_eq!(
        free.params.0.get("group"),
        Some(&ParamValue::Kind(flag_id)),
        "a kind param names a kind"
    );
    assert_eq!(free.params.0.get("trg"), Some(&real(3.0)));

    let (_, trap) = kind_named(&def, "@my_trap");
    assert_eq!(
        trap.params.0.get("trigger_kind"),
        Some(&ParamValue::Kind(free_id))
    );
    assert_eq!(
        trap.params.0.get("trigger_kind_b"),
        Some(&ParamValue::Kind(flag_id))
    );
    assert_eq!(trap.params.0.get("launch"), Some(&word("path")));
    assert_eq!(trap.params.0.get("path"), Some(&real(0.0)), "a path is its index");
    assert_eq!(trap.params.0.get("path_end"), Some(&word("reverse")));
    assert_eq!(trap.params.0.get("stop_on_stopper"), Some(&word("flush")));
    assert_eq!(trap.params.0.get("flag_guard"), Some(&yes));

    let (_, fake) = kind_named(&def, "@my_fake");
    assert_eq!(fake.params.0.get("vanish_on"), Some(&word("touch")));
    assert_eq!(fake.params.0.get("approach_distance"), Some(&real(4.0)));
    assert_eq!(fake.params.0.get("reverses_platforms"), Some(&yes));
    // `solid` and `visible` of an object file are the kind's own flags, not params.
    assert!(!fake.solid && fake.visible);
    assert_eq!(fake.params.0.get("solid"), None);
    let (_, hidden) = kind_named(&def, "@my_hidden");
    assert_eq!(hidden.params.0.get("reveal_distance"), Some(&real(1.0)));
    assert_eq!(
        hidden.params.0.get("destroy_when_no_kid"),
        Some(&ParamValue::Bool(false))
    );
    assert_eq!(hidden.params.0.get("destroyed_by_bullet"), Some(&yes));
    assert_eq!(hidden.params.0.get("destroyed_by_blood"), Some(&yes));
}

#[test]
fn wp6b2_params_are_checked() {
    let cases = [
        ("base = \"world\"\npause = \"freeze\"\n", "pause"),
        ("base = \"world\"\nmute = 1\n", "mute"),
        (
            "base = \"world\"\nclear_triggers_to = \"ten\"\n",
            "clear_triggers_to",
        ),
        (
            "base = \"world\"\nclear_switches_on_room_end = \"yes\"\n",
            "clear_switches_on_room_end",
        ),
        ("base = \"world\"\nbogus = 1\n", "bogus"),
        ("base = \"trigger\"\nmodel = \"fancy\"\n", "model"),
        ("base = \"trigger\"\narray = \"both\"\n", "array"),
        ("base = \"trigger\"\ngroup = \"@nope\"\n", "group"),
        ("base = \"trap\"\nlaunch = \"teleport\"\n", "launch"),
        ("base = \"trap\"\ntrigger_kind = \"@nope\"\n", "trigger_kind"),
        (
            "base = \"trap\"\nstop_on_stopper = \"explode\"\n",
            "stop_on_stopper",
        ),
        ("base = \"fake_block\"\nvanish_on = \"later\"\n", "vanish_on"),
        (
            "base = \"fake_block\"\nstops_bullets = \"yes\"\n",
            "stops_bullets",
        ),
        (
            "base = \"hidden_block\"\nreveal_distance = true\n",
            "reveal_distance",
        ),
    ];
    for (text, needle) in cases {
        let files = wp6b2_package(&[("bad", text)], false);
        let r = load_report(&files, STRICT);
        assert!(!r.ok(), "expected an error for {text:?}:\n{}", render(&r));
        assert!(
            errors(&r).iter().any(|e| e.contains(needle)),
            "{text:?}: no error mentions `{needle}`: {:?}",
            errors(&r)
        );
    }
}

// ---- #319: tier 2 built-in params (drop_platform, gravity_flip, edge_warp, bullet_blocker, slope)

/// The basic package with the WP6b.3 kinds in `object_order` and `extra_instances` appended to
/// `rooms/stage1.toml` (objects and extra files through `edit`).
fn tier2_package(extra_instances: &str, edit: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> PackageFiles {
    let order = ORDER.replace(
        "\"playerface\",",
        "\"playerface\", \"drop_platform\", \"gravity_flip\", \"edge_warp\", \"bullet_blocker\", \"slope\",",
    );
    package(|f| {
        set(f, "game.toml", manifest(&order, "").into_bytes());
        let stage = STAGE1.replace(
            "  { kind = \"warp\", x = 768, y = 512, room_to = \"title\" },\n",
            &format!("  {{ kind = \"warp\", x = 768, y = 512, room_to = \"title\" }},\n{extra_instances}"),
        );
        set(f, "rooms/stage1.toml", stage.into_bytes());
        edit(f);
    })
}

fn placed<'d>(def: &'d GameDef, room: &str, kind: &str) -> Vec<&'d iwbtg_sim::def::PlacedInstance> {
    let (id, _) = kind_named(def, kind);
    let room = def.rooms.iter().find(|r| r.name == room).expect("room");
    room.instances.iter().filter(|i| i.kind == id).collect()
}

#[test]
fn wp6b3_params_load_on_kinds_and_per_instance() {
    let files = tier2_package(
        concat!(
            "  { kind = \"drop_platform\", x = 100, y = 100, mode = \"rise\", speed = 3, solid = true },\n",
            "  { kind = \"gravity_flip\", x = 140, y = 100, direction = \"up\" },\n",
            "  { kind = \"gravity_flip\", x = 180, y = 100, direction = \"down\", transition = \"grav_sign\" },\n",
            "  { kind = \"bullet_blocker\", x = 220, y = 100, except_when_overlapping_kid = true },\n",
            "  { kind = \"slope\", x = 260, y = 100, shape = \"rise_left\", scale_x = -1 },\n",
            "  { kind = \"edge_warp\", x = 0, y = 0, left_room = \"title\", right_room = \"stage1\" },\n",
            // The SPK table, resolved per placed instance (CR17): one switch with an entry, one without.
            "  { kind = \"edge_warp\", x = 300, y = 100, mode = \"room_assign\", room_to = \"title\", warp_x = 32, warp_y = 64 },\n",
            "  { kind = \"edge_warp\", x = 340, y = 100, mode = \"room_assign\" },\n",
        ),
        |f| {
            f.push((
                "objects/slow_drop.toml".into(),
                b"base = \"drop_platform\"\nmode = \"rise_kae\"\nspeed = 1.5\n".to_vec(),
            ));
            f.push((
                "objects/grav_down.toml".into(),
                b"base = \"gravity_flip\"\ndirection = \"down\"\n".to_vec(),
            ));
        },
    );
    let (def, _) = load_ok(&files, STRICT);
    let word = |s: &str| ParamValue::Str(s.into());

    let drop = placed(&def, "stage1", "drop_platform")[0];
    assert_eq!(drop.params.0.get("mode"), Some(&word("rise")));
    assert_eq!(
        drop.params.0.get("speed"),
        Some(&ParamValue::Real(Real::new(3.0)))
    );
    assert_eq!(drop.params.0.get("solid"), Some(&ParamValue::Bool(true)));
    let (_, slow) = kind_named(&def, "@slow_drop");
    assert_eq!(slow.params.0.get("mode"), Some(&word("rise_kae")));
    assert_eq!(
        slow.params.0.get("speed"),
        Some(&ParamValue::Real(Real::new(1.5)))
    );

    let flips = placed(&def, "stage1", "gravity_flip");
    assert_eq!(flips[0].params.0.get("direction"), Some(&word("up")));
    assert_eq!(flips[1].params.0.get("direction"), Some(&word("down")));
    assert_eq!(flips[1].params.0.get("transition"), Some(&word("grav_sign")));
    let (_, down) = kind_named(&def, "@grav_down");
    assert_eq!(down.params.0.get("direction"), Some(&word("down")));

    assert_eq!(
        placed(&def, "stage1", "bullet_blocker")[0]
            .params
            .0
            .get("except_when_overlapping_kid"),
        Some(&ParamValue::Bool(true))
    );
    let slope = placed(&def, "stage1", "slope")[0];
    assert_eq!(slope.params.0.get("shape"), Some(&word("rise_left")));
    assert_eq!(
        slope.params.0.get("scale_x"),
        Some(&ParamValue::Real(Real::new(-1.0)))
    );

    let title = RoomId(def.rooms.iter().position(|r| r.name == "title").unwrap() as u32);
    let warps = placed(&def, "stage1", "edge_warp");
    assert_eq!(warps[0].params.0.get("left_room"), Some(&ParamValue::Room(title)));
    assert_eq!(warps[1].params.0.get("mode"), Some(&word("room_assign")));
    assert_eq!(
        warps[1].params.0.get("room_to"),
        Some(&ParamValue::Room(title)),
        "a room name resolves to the room"
    );
    assert_eq!(
        warps[1].params.0.get("warp_x"),
        Some(&ParamValue::Real(Real::new(32.0)))
    );
    assert_eq!(
        warps[1].params.0.get("warp_y"),
        Some(&ParamValue::Real(Real::new(64.0)))
    );
    assert_eq!(
        warps[2].params.0.get("room_to"),
        None,
        "a switch without a table entry has no target"
    );
}

fn tier2_errors(instance: &str) -> Vec<String> {
    let files = tier2_package(&format!("  {instance},\n"), |_| {});
    let r = load_report(&files, STRICT);
    assert!(!r.ok(), "expected an error for {instance}:\n{}", render(&r));
    errors(&r)
}

#[test]
fn wp6b3_params_are_checked() {
    let cases = [
        (
            r#"{ kind = "drop_platform", x = 0, y = 0, mode = "sideways" }"#,
            "mode",
        ),
        (
            r#"{ kind = "drop_platform", x = 0, y = 0, speed = "fast" }"#,
            "speed",
        ),
        (r#"{ kind = "drop_platform", x = 0, y = 0, solid = 1 }"#, "solid"),
        (r#"{ kind = "gravity_flip", x = 0, y = 0 }"#, "direction"),
        (
            r#"{ kind = "gravity_flip", x = 0, y = 0, direction = "left" }"#,
            "direction",
        ),
        (
            r#"{ kind = "gravity_flip", x = 0, y = 0, direction = "up", transition = "teleport" }"#,
            "transition",
        ),
        (
            r#"{ kind = "gravity_flip", x = 0, y = 0, direction = "up", bogus = 1 }"#,
            "bogus",
        ),
        (
            r#"{ kind = "bullet_blocker", x = 0, y = 0, except_when_overlapping_kid = "yes" }"#,
            "except_when_overlapping_kid",
        ),
        (r#"{ kind = "slope", x = 0, y = 0, shape = "wavy" }"#, "shape"),
        (r#"{ kind = "slope", x = 0, y = 0, scale_x = true }"#, "scale_x"),
        (
            r#"{ kind = "edge_warp", x = 0, y = 0, mode = "teleport" }"#,
            "mode",
        ),
        (
            r#"{ kind = "edge_warp", x = 0, y = 0, mode = "room_assign", room_to = "nowhere" }"#,
            "nowhere",
        ),
        (
            r#"{ kind = "edge_warp", x = 0, y = 0, room_to = "title", left_room = "title" }"#,
            "room_to",
        ),
    ];
    for (instance, needle) in cases {
        let errs = tier2_errors(instance);
        assert!(
            errs.iter().any(|e| e.contains(needle)),
            "{instance}: no error mentions `{needle}`: {errs:?}"
        );
    }
}

#[test]
fn a_gravity_flip_with_view_rotation_loads() {
    let files = tier2_package(
        r#"  { kind = "gravity_flip", x = 0, y = 0, direction = "up", transition = "yflag_with_view_rotation" },
"#,
        |_| {},
    );
    let r = load_report(&files, STRICT);
    assert!(r.ok(), "{}", render(&r));
}

#[test]
fn script_problems_are_diagnostics_with_file_line_column() {
    let files = package(|f| {
        set(
            f,
            "scripts/boss.rhai",
            "fn step() {
    if this.x > 1.0 && random(1.0) < 0.5 { destroy(this); }
}
",
        );
    });
    let r = load_report(&files, STRICT);
    let text = render(&r);
    assert!(
        text.contains("scripts/boss.rhai:2:") && text.contains("`random` draws from the RNG"),
        "{text}"
    );
    assert!(!r.ok(), "{text}");

    let files = package(|f| {
        set(
            f,
            "scripts/boss.rhai",
            "fn step() { if this.x == 1.5 { destroy(this); } }
",
        )
    });
    let r = load_report(&files, STRICT);
    assert!(
        r.ok(),
        "a lint warning does not refuse the package: {}",
        render(&r)
    );
    assert!(render(&r).contains("warning: raw `==`"), "{}", render(&r));

    let files = package(|f| {
        set(
            f,
            "scripts/boss.rhai",
            "fn create() { instance_create(\"@nope\", 1, 2); }
",
        )
    });
    assert!(render(&load_report(&files, STRICT)).contains("unknown kind \"@nope\""));
}

// ---- #138: tier 3 misc built-ins (time_limit, sign, ladder, decor)

fn misc_package(objects: &[(&str, &str)]) -> PackageFiles {
    let listed: String = objects.iter().map(|(s, _)| format!("\"@{s}\", ")).collect();
    let order = ORDER.replace("\"playerface\",", &format!("\"playerface\", {listed}"));
    package(|f| {
        set(f, "game.toml", manifest(&order, "").into_bytes());
        for (stem, text) in objects {
            f.push((format!("objects/{stem}.toml"), text.as_bytes().to_vec()));
        }
    })
}

#[test]
fn wp6c3_params_load() {
    let files = misc_package(&[
        (
            "my_sign",
            "base = \"sign\"\ntext = \"Beware of the apple.\"\nmode = \"while_overlapping\"\n",
        ),
        (
            "my_timer",
            "base = \"time_limit\"\nrole = \"timer\"\nminutes = 2\nseconds = 56\non_expire = \"goto_room\"\nroom = \"title\"\nwarp_x = 32\nwarp_y = 64\n",
        ),
        ("my_zone", "base = \"time_limit\"\nrole = \"start_zone\"\n"),
        ("my_ladder", "base = \"ladder\"\n"),
        ("my_decor", "base = \"decor\"\n"),
    ]);
    let (def, _) = load_ok(&files, STRICT);
    let (_, sign) = kind_named(&def, "@my_sign");
    assert_eq!(
        sign.params.0.get("text"),
        Some(&ParamValue::Str("Beware of the apple.".into())),
        "text stays text"
    );
    assert_eq!(
        sign.params.0.get("mode"),
        Some(&ParamValue::Str("while_overlapping".into()))
    );
    let (_, timer) = kind_named(&def, "@my_timer");
    assert_eq!(
        timer.params.0.get("minutes"),
        Some(&ParamValue::Real(Real::new(2.0)))
    );
    assert!(
        matches!(timer.params.0.get("room"), Some(ParamValue::Room(_))),
        "a room param names a room"
    );
    let (_, ladder) = kind_named(&def, "@my_ladder");
    assert!(ladder.classes.intersects(iwbtg_sim::def::ClassMask::LADDER));
}

#[test]
fn wp6c3_params_are_checked() {
    let cases = [
        ("base = \"sign\"\ntext = 3\n", "text"),
        ("base = \"sign\"\ntext = \"x\"\nmode = \"always\"\n", "mode"),
        ("base = \"time_limit\"\non_expire = \"explode\"\n", "on_expire"),
        ("base = \"time_limit\"\nroom = \"@nope\"\n", "room"),
        ("base = \"time_limit\"\nminutes = true\n", "minutes"),
        ("base = \"ladder\"\nrungs = 3\n", "rungs"),
        ("base = \"decor\"\nbogus = 1\n", "bogus"),
    ];
    for (text, needle) in cases {
        let files = misc_package(&[("bad", text)]);
        let r = load_report(&files, STRICT);
        assert!(!r.ok(), "expected an error for {text:?}:\n{}", render(&r));
        assert!(
            errors(&r).iter().any(|e| e.contains(needle)),
            "{text:?}: no error mentions `{needle}`: {:?}",
            errors(&r)
        );
    }
}

// ---- #291: tier 2 flag and music built-ins (boss_gate, boss_item, secret_item, item_viewer, game_clear, music)

fn flags_package(objects: &[(&str, &str)]) -> PackageFiles {
    let listed: String = objects.iter().map(|(s, _)| format!("\"@{s}\", ")).collect();
    let order = ORDER.replace("\"playerface\",", &format!("\"playerface\", {listed}"));
    package(|f| {
        set(f, "game.toml", manifest(&order, "").into_bytes());
        f.push(("sounds/theme.ogg".into(), b"ogg".to_vec()));
        for (stem, text) in objects {
            f.push((format!("objects/{stem}.toml"), text.as_bytes().to_vec()));
        }
    })
}

#[test]
fn wp6b4_params_load() {
    let files = flags_package(&[
        ("gate", "base = \"boss_gate\"\nboss_index = 3\n"),
        (
            "reward",
            "base = \"boss_item\"\nboss_index = 3\nwatched_object = \"@boss\"\n",
        ),
        ("secret", "base = \"secret_item\"\nitem_index = 15\n"),
        ("viewer", "base = \"item_viewer\"\nkind = \"item\"\nindex = 2\n"),
        ("finish", "base = \"game_clear\"\nsave_with_position = false\n"),
        (
            "bgm",
            "base = \"music\"\ntrack = \"theme\"\nloop = false\nmode = \"every_step_if_not_playing\"\nrespects_music_off = true\n",
        ),
    ]);
    let (def, _) = load_ok(&files, STRICT);
    let real = |v: f64| ParamValue::Real(Real::new(v));
    let (_, gate) = kind_named(&def, "@gate");
    assert_eq!(gate.params.0.get("boss_index"), Some(&real(3.0)));
    let (boss_id, _) = kind_named(&def, "@boss");
    let (_, reward) = kind_named(&def, "@reward");
    assert_eq!(
        reward.params.0.get("watched_object"),
        Some(&ParamValue::Kind(boss_id))
    );
    let (_, secret) = kind_named(&def, "@secret");
    assert_eq!(secret.params.0.get("item_index"), Some(&real(15.0)));
    let (_, viewer) = kind_named(&def, "@viewer");
    assert_eq!(viewer.params.0.get("kind"), Some(&ParamValue::Str("item".into())));
    assert_eq!(viewer.params.0.get("index"), Some(&real(2.0)));
    let (_, finish) = kind_named(&def, "@finish");
    assert_eq!(
        finish.params.0.get("save_with_position"),
        Some(&ParamValue::Bool(false))
    );
    let (_, bgm) = kind_named(&def, "@bgm");
    assert!(
        matches!(bgm.params.0.get("track"), Some(ParamValue::Sound(_))),
        "a track names a sound"
    );
    assert_eq!(bgm.params.0.get("loop"), Some(&ParamValue::Bool(false)));
    assert_eq!(
        bgm.params.0.get("mode"),
        Some(&ParamValue::Str("every_step_if_not_playing".into()))
    );
    assert_eq!(
        bgm.params.0.get("respects_music_off"),
        Some(&ParamValue::Bool(true))
    );
}

/// The basic package with the WP6b.4 built-ins listed and `instance` placed in `rooms/stage1.toml`.
fn flags_instance_errors(instance: &str) -> Vec<String> {
    let order = ORDER.replace(
        "\"playerface\",",
        "\"playerface\", \"boss_gate\", \"boss_item\", \"secret_item\", \"item_viewer\", \"game_clear\", \"music\",",
    );
    let files = package(|f| {
        set(f, "game.toml", manifest(&order, "").into_bytes());
        f.push(("sounds/theme.ogg".into(), b"ogg".to_vec()));
        let stage = STAGE1.replace(
            "  { kind = \"warp\", x = 768, y = 512, room_to = \"title\" },
",
            &format!(
                "  {{ kind = \"warp\", x = 768, y = 512, room_to = \"title\" }},
  {instance},
"
            ),
        );
        set(f, "rooms/stage1.toml", stage.into_bytes());
    });
    let r = load_report(&files, STRICT);
    assert!(
        !r.ok(),
        "expected an error for {instance}:
{}",
        render(&r)
    );
    errors(&r)
}

#[test]
fn wp6b4_params_are_checked() {
    let cases = [
        (r#"{ kind = "boss_gate", x = 0, y = 0 }"#, "boss_index"),
        (
            r#"{ kind = "boss_gate", x = 0, y = 0, boss_index = 16 }"#,
            "boss_index",
        ),
        (
            r#"{ kind = "boss_gate", x = 0, y = 0, boss_index = "one" }"#,
            "boss_index",
        ),
        (
            r#"{ kind = "boss_item", x = 0, y = 0, boss_index = 0, watched_object = "nope" }"#,
            "watched_object",
        ),
        (
            r#"{ kind = "boss_item", x = 0, y = 0, boss_index = 0, watched_object = 3 }"#,
            "watched_object",
        ),
        (
            r#"{ kind = "secret_item", x = 0, y = 0, item_index = 1.5 }"#,
            "item_index",
        ),
        (r#"{ kind = "item_viewer", x = 0, y = 0, index = 1 }"#, "kind"),
        (
            r#"{ kind = "item_viewer", x = 0, y = 0, index = 1, bogus = 1 }"#,
            "bogus",
        ),
        (
            r#"{ kind = "game_clear", x = 0, y = 0, save_with_position = "no" }"#,
            "save_with_position",
        ),
        (r#"{ kind = "music", x = 0, y = 0 }"#, "track"),
        (r#"{ kind = "music", x = 0, y = 0, track = "nope" }"#, "nope"),
        (
            r#"{ kind = "music", x = 0, y = 0, track = "theme", mode = "sometimes" }"#,
            "mode",
        ),
        (
            r#"{ kind = "music", x = 0, y = 0, track = "theme", loop = 1 }"#,
            "loop",
        ),
        (
            r#"{ kind = "music", x = 0, y = 0, track = "theme", respects_music_off = "yes" }"#,
            "respects_music_off",
        ),
    ];
    for (instance, needle) in cases {
        let errs = flags_instance_errors(instance);
        assert!(
            errs.iter().any(|e| e.contains(needle)),
            "{instance}: no error mentions `{needle}`: {errs:?}"
        );
    }
}

#[test]
fn an_item_viewer_kind_is_a_preset_param_because_the_placed_key_kind_is_taken() {
    // `kind` on a placed instance names the object; the viewer's own `kind` param (boss or item)
    // can only come from an object file.
    let files = flags_package(&[(
        "viewer",
        "base = \"item_viewer\"
kind = \"trophy\"
index = 1
",
    )]);
    let r = load_report(&files, STRICT);
    assert!(!r.ok());
    assert!(errors(&r).iter().any(|e| e.contains("kind")), "{:?}", errors(&r));
}

// -- Engine packs (docs/format/engine.md) ------------------------------------------------------

const PACK_TOML: &str = "#:schema ../engine.schema.json\nid = \"a1\"\ntitle = \"Pack A1\"\nfamily = \"A1\"\n\n[[creators]]\nname = \"Someone\"\nrole = \"art\"\n";

/// The basic package with its kid sprite moved into a bundled pack `a1`.
fn with_pack(edit: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> PackageFiles {
    package(|f| {
        f.retain(|(p, _)| p != "sprites/kid.toml" && p != "sprites/kid.png");
        let m = String::from_utf8(f[0].1.clone()).unwrap();
        let m = m
            .replace(
                "[defaults.kid]\nsprite = \"kid\"",
                "[defaults.kid]\nsprite = \"engine:a1/kid\"",
            )
            .replace(
                "physics = \"physics.toml\"",
                "physics = \"physics.toml\"\nengine_pack = \"a1\"",
            );
        f[0].1 = m.into_bytes();
        f.push(("engine/a1/engine.toml".into(), PACK_TOML.as_bytes().to_vec()));
        f.push((
            "engine/a1/sprites/kid.toml".into(),
            sidecar(1, 4, 4, "precise", None, ""),
        ));
        f.push((
            "engine/a1/sprites/kid.png".into(),
            png(&["....", ".##.", ".##.", "...."]),
        ));
        edit(f);
    })
}

#[test]
fn an_engine_pack_sprite_resolves_like_a_package_sprite() {
    let (def, report) = load_ok(&with_pack(|_| {}), STRICT);
    let (_, kid) = kind_named(&def, "kid");
    let sprite = kid.sprite.expect("the kid has its pack sprite");
    let (plain, _) = load_ok(&package(|_| {}), STRICT);
    let plain_sprite = &plain.sprites[kind_named(&plain, "kid").1.sprite.unwrap().0 as usize];
    let packed = &def.sprites[sprite.0 as usize];
    assert_eq!(
        (packed.frames, packed.size),
        (plain_sprite.frames, plain_sprite.size)
    );
    assert!(report.gameplay_hash.is_some());
}

#[test]
fn pack_credits_are_not_gameplay_but_pack_sprites_are() {
    let base = load_ok(&with_pack(|_| {}), STRICT).1.gameplay_hash;
    let credits = load_ok(
        &with_pack(|f| {
            set(
                f,
                "engine/a1/engine.toml",
                PACK_TOML.replace("Someone", "Someone Else"),
            )
        }),
        STRICT,
    )
    .1
    .gameplay_hash;
    assert_eq!(base, credits);
    let changed = load_ok(
        &with_pack(|f| {
            set(
                f,
                "engine/a1/sprites/kid.toml",
                sidecar(1, 4, 4, "rect", Some([0, 0, 1, 1]), ""),
            )
        }),
        STRICT,
    )
    .1
    .gameplay_hash;
    assert_ne!(base, changed, "a pack sprite is gameplay data");
}

#[test]
fn an_engine_pack_that_is_not_in_the_package_is_an_error() {
    let files = with_pack(|f| f.retain(|(p, _)| !p.starts_with("engine/")));
    let r = load_report(&files, STRICT);
    let errs = errors(&r).join("\n");
    assert!(errs.contains("engine pack `a1` is not in the package"), "{errs}");
}

#[test]
fn an_engine_pack_without_the_sprite_is_an_error() {
    let files = with_pack(|f| {
        f.retain(|(p, _)| p != "engine/a1/sprites/kid.toml" && p != "engine/a1/sprites/kid.png");
    });
    let errs = errors(&load_report(&files, STRICT)).join("\n");
    assert!(
        errs.contains("`engine:a1/kid`: the engine pack `a1` has no sprite `kid`"),
        "{errs}"
    );
}

#[test]
fn engine_toml_is_checked() {
    let files = with_pack(|f| set(f, "engine/a1/engine.toml", PACK_TOML.replace("\"A1\"", "\"Z9\"")));
    let errs = errors(&load_report(&files, STRICT)).join("\n");
    assert!(errs.contains("family `Z9` must be one of"), "{errs}");
    let files = with_pack(|f| {
        set(
            f,
            "engine/a1/engine.toml",
            PACK_TOML.replace("id = \"a1\"", "id = \"b\""),
        )
    });
    let errs = errors(&load_report(&files, STRICT)).join("\n");
    assert!(errs.contains("must be the folder name `a1`"), "{errs}");
}

#[test]
fn an_engine_pack_sound_resolves_in_the_physics_sounds() {
    let files = with_pack(|f| {
        set(
            f,
            "physics.toml",
            "preset = \"A1\"\n[sounds]\njump = \"engine:a1/jump\"\n",
        );
        f.push(("engine/a1/sounds/jump.ogg".into(), b"OggS".to_vec()));
    });
    let (def, _) = load_ok(&files, STRICT);
    let id = def.physics.sounds.jump.expect("the jump sound resolves");
    assert_eq!(def.sounds[id.0 as usize].name, "engine:a1/jump");
    // A pack sound is gameplay data; the pack's credits are not.
    let other = with_pack(|f| {
        set(
            f,
            "physics.toml",
            "preset = \"A1\"\n[sounds]\njump = \"engine:a1/jump\"\n",
        );
        f.push(("engine/a1/sounds/jump.ogg".into(), b"OggS".to_vec()));
        f.push((
            "engine/a1/sounds/jump.toml".into(),
            b"length_ticks = 10\n".to_vec(),
        ));
    });
    let (def2, _) = load_ok(&other, STRICT);
    assert_eq!(def2.sounds[0].length_ticks, 10, "the pack sidecar is read");
}

#[test]
fn a_missing_engine_pack_sound_is_an_error() {
    let files = with_pack(|f| {
        set(
            f,
            "physics.toml",
            "preset = \"A1\"\n[sounds]\njump = \"engine:a1/nope\"\n",
        );
    });
    let errs = errors(&load_report(&files, STRICT)).join("\n");
    assert!(
        errs.contains("the engine pack `a1` has no sound `nope`"),
        "{errs}"
    );
}

// ---- #139: scripted objects get their events from the script and `collides_with`

fn scripted_package() -> PackageFiles {
    let order = ORDER.replace("\"playerface\",", "\"playerface\", \"@mover\",");
    package(|f| {
        set(f, "game.toml", manifest(&order, "").into_bytes());
        let stage = STAGE1.replace(
            "  { kind = \"warp\", x = 768, y = 512, room_to = \"title\" },\n",
            "  { kind = \"warp\", x = 768, y = 512, room_to = \"title\" },\n  { kind = \"@mover\", x = 100, y = 100 },\n",
        );
        set(f, "rooms/stage1.toml", stage.into_bytes());
        f.push((
            "objects/mover.toml".into(),
            b"script = \"mover\"\nsprite = \"block\"\ncollides_with = [\"kid_bullet\"]\n".to_vec(),
        ));
        f.push((
            "scripts/mover.rhai".into(),
            b"fn create() { this.set_alarm(0, 2); }\nfn step() { this.x += 1.0; }\nfn alarm(n) { this.y = 7.0; }\nfn collide(other) { this.y = 0.0; }\n"
                .to_vec(),
        ));
    })
}

#[test]
fn a_scripted_kind_handles_what_its_script_defines_and_its_collides_with() {
    let (def, _) = load_ok(&scripted_package(), STRICT);
    let (_, mover) = kind_named(&def, "@mover");
    assert_eq!(mover.base, iwbtg_sim::def::Builtin::Scripted);
    assert!(mover.events.step, "step() is defined");
    assert_ne!(mover.events.alarms, 0, "alarm(n) is defined");
    assert!(!mover.events.end_step && !mover.events.begin_step);
    assert_eq!(
        mover.events.collisions.as_slice(),
        &[iwbtg_sim::def::ClassMask::KID_BULLET],
        "collides_with names the classes collide(other) listens to"
    );
    let (_, boss) = kind_named(&def, "@boss");
    assert!(!boss.events.step, "a script with only create() adds nothing");
}

#[test]
fn a_scripted_object_runs_in_a_room() {
    use iwbtg_sim::input::InputFrame;
    let (def, _) = load_ok(&scripted_package(), STRICT);
    let stage = iwbtg_sim::def::RoomId(def.rooms.iter().position(|r| r.name == "stage1").unwrap() as u32);
    let (mover_kind, _) = kind_named(&def, "@mover");
    let mut sim = iwbtg_sim::Sim::new(
        std::sync::Arc::new(def),
        iwbtg_sim::SimOptions {
            start: iwbtg_sim::StartPoint::Room {
                room: stage,
                difficulty: 0,
            },
            seed: 1,
            dll_mode: iwbtg_sim::sound::DllMode::Stub,
            mode: iwbtg_sim::stripped::PlayMode::Original,
            map: None,
        },
    )
    .expect("the sim starts");
    let at = |sim: &iwbtg_sim::Sim| {
        let i = sim
            .world()
            .instances()
            .iter()
            .find(|i| i.kind == mover_kind)
            .expect("mover");
        (i.m.x().get(), i.m.y().get())
    };
    assert_eq!(at(&sim), (100.0, 100.0));
    for _ in 0..3 {
        sim.step(InputFrame::EMPTY);
    }
    assert_eq!(
        at(&sim),
        (103.0, 7.0),
        "step() moved it 3 times and the alarm set y"
    );
    let hash = sim.state_hash();
    let mut again =
        iwbtg_sim::Sim::new(std::sync::Arc::new(sim.def().clone()), sim.options().clone()).unwrap();
    for _ in 0..3 {
        again.step(InputFrame::EMPTY);
    }
    assert_eq!(again.state_hash(), hash, "two runs hash the same");
}

#[test]
fn sprite_pngs_follow_the_order_of_the_loaded_sprites() {
    let files = with_pack(|_| {});
    let (def, _) = load_ok(&files, STRICT);
    let pngs = iwbtg_format::sprites::sprite_pngs(&files);
    assert_eq!(pngs.len(), def.sprites.len());
    assert!(pngs.iter().any(|(k, _)| k == "engine:a1/kid"));
    for ((key, path), sprite) in pngs.iter().zip(&def.sprites) {
        let bytes = files.get(path).unwrap_or_else(|| panic!("{key}: no {path}"));
        let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
        assert_eq!(width, sprite.size.0 * u32::from(sprite.frames), "{key}");
    }
}

#[test]
fn the_golden_walker_script_loads_clean_through_the_loader() {
    // The golden corpus (crates/sim/tests/golden) runs this script; here the loader and the
    // validator accept the same text. The corpus names its kinds without `@`.
    let source =
        include_str!("../../sim/tests/golden/scripted_walker.rhai").replace("\"spark\"", "\"@boss\"");
    let files = package(|f| set(f, "scripts/boss.rhai", source));
    let r = load_report(&files, STRICT);
    assert!(r.ok(), "{}", render(&r));
    assert!(!render(&r).contains("scripts/boss.rhai"), "{}", render(&r));
}

// ---- #31: maps ------------------------------------------------------------------------------

const TWO_MAPS: &str = r#"
[[maps]]
id = "any"
name = "Any%"
category = "Any%"

[[maps]]
id = "hard_stage1"
name = "Hard, Stage 1"
difficulty = "Hard"
category = "Stage 1"
end = { trigger = "enter_room", room = "title" }
stages = []
"#;

#[test]
fn maps_get_their_own_timing_and_inherit_the_rest() {
    let files = package(|f| set(f, "game.toml", manifest(ORDER, TWO_MAPS)));
    let report = load_report(&files, STRICT);
    assert!(report.ok(), "{}", render(&report));
    let ids: Vec<&str> = report.maps.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["any", "hard_stage1"]);
    let def = report.def.as_ref().unwrap();
    // `any` inherits `[timing]`.
    assert_eq!(report.maps[0].timing, def.timing);
    let hard = &report.maps[1].timing;
    assert_eq!(hard.start, TimingTrigger::RunStart);
    assert!(hard.stages.is_empty());
    assert!(matches!(hard.end, TimingTrigger::EnterRoom(_)));
    // The definition carries the same chains for the sim (#505).
    let chains: Vec<_> = def.maps.iter().map(|m| (m.id.as_str(), &m.timing)).collect();
    let want: Vec<_> = report.maps.iter().map(|m| (m.id.as_str(), &m.timing)).collect();
    assert_eq!(chains, want);
}

#[test]
fn a_package_without_maps_has_one_main_map() {
    let report = load_report(&package(|_| {}), STRICT);
    assert_eq!(report.maps.len(), 1);
    assert_eq!(report.maps[0].id, "main");
    assert_eq!(report.maps[0].timing, report.def.as_ref().unwrap().timing);
}

#[test]
fn a_map_trigger_must_reach_a_room_that_exists() {
    let bad = TWO_MAPS.replace("room = \"title\"", "room = \"nowhere\"");
    let files = package(|f| set(f, "game.toml", manifest(ORDER, &bad)));
    let report = load_report(&files, STRICT);
    assert!(!report.ok());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.file == "game.toml" && d.message.contains("nowhere")),
        "{}",
        render(&report)
    );
    // The broken map is left out; the other still resolves.
    let ids: Vec<&str> = report.maps.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["any"]);
}

#[test]
fn renaming_a_map_keeps_the_gameplay_hash() {
    let hash = |text: &str| {
        let files = package(|f| set(f, "game.toml", manifest(ORDER, text)));
        load_report(&files, STRICT).gameplay_hash.expect("loads")
    };
    let base = hash(TWO_MAPS);
    assert_eq!(
        hash(&TWO_MAPS.replace("name = \"Any%\"", "name = \"Any% (glitched)\"")),
        base
    );
    assert_ne!(
        hash(&TWO_MAPS.replace("room = \"title\"", "room = \"stage1\"")),
        base
    );
}

// ---- #32: stripped mode markers ---------------------------------------------------------------

const STRIPPED: &str = r#"
[[maps]]
id = "medium"
name = "Medium"

[[maps]]
id = "hard"
name = "Hard"
stripped_start = { globals = { difficulty = 2 } }

[stripped]
start = { room = "stage1", globals = { difficulty = 1, clear = false, file = "save1", exact = { bits = "3FB999999999999A" } } }
cutscenes = [
  { id = "boss_intro", kind = "@boss" },
  { id = "title_scroll", room = "title" },
]
dialogues = [
  { id = "npc", kind = "warp", advance = "jump", timeout_ticks = 150 },
]

[stripped.cut]
rooms = [{ room = "title", to = "stage1" }]
kinds = ["kid_start"]
instances = [{ room = "stage1", name = "boss_a" }]
"#;

fn stripped_errors(text: &str) -> Vec<String> {
    let files = package(|f| set(f, "game.toml", manifest(ORDER, text)));
    errors(&load_report(&files, STRICT))
}

#[test]
fn every_stripped_field_loads() {
    let files = package(|f| set(f, "game.toml", manifest(ORDER, STRIPPED)));
    let report = load_report(&files, STRICT);
    assert!(report.ok(), "{}", render(&report));
    assert!(report.diagnostics.is_empty(), "{}", render(&report));
}

#[test]
fn stripped_resolves_into_the_game_def() {
    use iwbtg_sim::stripped::RegionCondition;
    let files = package(|f| set(f, "game.toml", manifest(ORDER, STRIPPED)));
    let (def, _) = load_ok(&files, STRICT);
    let room =
        |name: &str| iwbtg_sim::def::RoomId(def.rooms.iter().position(|r| r.name == name).unwrap() as u32);
    let s = &def.stripped;
    assert_eq!(s.start.room, Some(room("stage1")));
    assert_eq!(s.start.difficulty, Some(1));
    let names: Vec<&str> = s
        .start
        .globals
        .iter()
        .map(|(v, _)| def.var_names.iter().find(|(_, id)| *id == v).unwrap().0.as_str())
        .collect();
    assert_eq!(
        names,
        ["clear", "difficulty", "exact", "file"],
        "interned, in name order"
    );
    let ids: Vec<&str> = def.maps.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["medium", "hard"], "[[maps]] order");
    assert_eq!(def.maps[0].start, s.start, "no stripped_start: the package's");
    let hard = &def.maps[1].start;
    assert_eq!(hard.difficulty, Some(2), "the map's global wins");
    assert_eq!(
        hard.room,
        Some(room("stage1")),
        "keys it leaves out come from [stripped].start"
    );
    assert_eq!(hard.globals.len(), 4);
    assert_eq!(s.cut_rooms.get(&room("title")), Some(&room("stage1")));
    let (kid_start, _) = kind_named(&def, "kid_start");
    assert!(s.cut_kinds.contains(&kid_start));
    assert_eq!(s.cut_instances.len(), 1);
    let (stage, name) = *s.cut_instances.iter().next().unwrap();
    assert_eq!(stage, room("stage1"));
    assert_eq!(def.names[name.0 as usize], "boss_a");
    assert_eq!(s.cutscenes.len(), 2);
    assert_eq!(s.cutscenes[1].holds, RegionCondition::Room(room("title")));
    assert_eq!(s.dialogues[0].advance, iwbtg_sim::input::Action::Jump);
    assert_eq!(s.dialogues[0].timeout_ticks, 150);

    // Without [stripped]: nothing cut, one `main` start that defaults to new_game_room.
    let (plain, _) = load_ok(&package(|_| {}), STRICT);
    assert_eq!(plain.stripped.start, iwbtg_sim::stripped::RankedStart::default());
    assert_eq!(plain.maps.len(), 1);
    assert_eq!(plain.maps[0].id, "main");
    assert_eq!(plain.maps[0].start, plain.stripped.start);
    assert_eq!(plain.maps[0].timing, plain.timing);
    assert!(plain.stripped.cut_rooms.is_empty() && plain.stripped.cut_kinds.is_empty());
}

#[test]
fn a_ranked_start_difficulty_must_be_a_whole_number_from_0_to_255() {
    for bad in [
        "difficulty = 1.5",
        "difficulty = 256",
        "difficulty = -1",
        "difficulty = \"hard\"",
    ] {
        let errs = stripped_errors(&STRIPPED.replace("difficulty = 2", bad));
        assert!(
            errs.iter().any(|e| e.contains("a whole number from 0 to 255")),
            "{bad}: {errs:#?}"
        );
    }
}

#[test]
fn stripped_mode_rejects_bad_markers() {
    let cases: [(&str, &str, &str); 18] = [
        (
            "globals = { difficulty = 2 } }",
            "room = \"gone\", globals = { difficulty = 2 } }",
            "room `gone` does not exist",
        ),
        (
            "globals = { difficulty = 2 } }",
            "room = \"title\", globals = { difficulty = 2 } }",
            "ranked start room `title` is cut",
        ),
        (
            "rooms = [{ room = \"title\", to = \"stage1\" }]",
            "rooms = [{ room = \"title\", to = \"stage1\" }, { room = \"title\", to = \"stage1\" }]",
            "room `title` is cut twice",
        ),
        (
            "instances = [{ room = \"stage1\", name = \"boss_a\" }]",
            "instances = [{ room = \"stage1\", name = \"boss_a\" }, { room = \"stage1\", name = \"boss_a\" }]",
            "instance `boss_a` in room `stage1` is cut twice",
        ),
        (
            "room = \"stage1\", globals",
            "room = \"nowhere\", globals",
            "room `nowhere` does not exist",
        ),
        ("difficulty = 1,", "1difficulty = 1,", "is not a variable name"),
        ("clear = false", "clear = [1]", "global `clear`"),
        ("to = \"stage1\"", "to = \"lost\"", "room `lost` does not exist"),
        (
            "rooms = [{ room = \"title\", to = \"stage1\" }]",
            "rooms = [{ room = \"title\", to = \"stage1\" }, { room = \"stage1\", to = \"title\" }]",
            "which is cut too",
        ),
        (
            "kinds = [\"kid_start\"]",
            "kinds = [\"@ghost\"]",
            "cut kind `@ghost` is not a kind",
        ),
        (
            "kinds = [\"kid_start\"]",
            "kinds = [\"kid_start\", \"kid_start\"]",
            "is cut twice",
        ),
        (
            "name = \"boss_a\"",
            "name = \"boss_b\"",
            "has no instance named `boss_b`",
        ),
        (
            "kind = \"@boss\" }",
            "kind = \"@boss\", room = \"title\" }",
            "has both `kind` and `room`",
        ),
        (
            "{ id = \"boss_intro\", kind = \"@boss\" }",
            "{ id = \"boss_intro\" }",
            "needs `kind` or `room`",
        ),
        (
            "id = \"npc\"",
            "id = \"boss_intro\"",
            "region id `boss_intro` is used twice",
        ),
        (
            "advance = \"jump\"",
            "advance = \"space\"",
            "`space` is not an action",
        ),
        (
            "timeout_ticks = 150",
            "timeout_ticks = 0",
            "timeout_ticks must be at least 1",
        ),
        (
            "kind = \"warp\", advance",
            "kind = \"@npc\", advance",
            "region kind `@npc` is not a kind",
        ),
    ];
    for (from, to, want) in cases {
        assert!(STRIPPED.contains(from), "case text `{from}` not in STRIPPED");
        let errs = stripped_errors(&STRIPPED.replacen(from, to, 1));
        assert!(
            errs.iter().any(|e| e.contains(want)),
            "`{to}` should give `{want}`, got {errs:#?}"
        );
    }
}

#[test]
fn the_ranked_start_room_cannot_be_cut() {
    // Named start room.
    let errs = stripped_errors(&STRIPPED.replace(
        "{ room = \"title\", to = \"stage1\" }",
        "{ room = \"stage1\", to = \"title\" }",
    ));
    assert!(
        errs.iter()
            .any(|e| e.contains("ranked start room `stage1` is cut")),
        "{errs:#?}"
    );
    // Default start room: `[engine].new_game_room` is `stage1`.
    let no_start = "\n[stripped.cut]\nrooms = [{ room = \"stage1\", to = \"title\" }]\n";
    let errs = stripped_errors(no_start);
    assert!(
        errs.iter().any(|e| e.contains("ranked runs start in `stage1`")),
        "{errs:#?}"
    );
    // Every map names its own start room: the default start may be cut.
    let all_maps = "
[[maps]]
id = \"a\"
name = \"A\"
stripped_start = { room = \"title\" }
                    
[stripped.cut]
rooms = [{ room = \"stage1\", to = \"title\" }]
";
    let errs = stripped_errors(all_maps);
    assert!(errs.is_empty(), "{errs:#?}");
}

#[test]
fn stripped_markers_count_for_the_gameplay_hash() {
    let hash = |text: &str| {
        let files = package(|f| set(f, "game.toml", manifest(ORDER, text)));
        load_report(&files, STRICT).gameplay_hash.expect("loads")
    };
    let base = hash(STRIPPED);
    assert_ne!(
        hash(&STRIPPED.replace("timeout_ticks = 150", "timeout_ticks = 100")),
        base
    );
    assert_ne!(hash(&STRIPPED.replace("difficulty = 2", "difficulty = 3")), base);
}

#[test]
fn a_set_global_difficulty_must_be_a_whole_number_from_0_to_255() {
    for bad in ["1.5", "256", "-1", "\"hard\""] {
        let files = package(|f| {
            set(
                f,
                "rooms/title.toml",
                TITLE.replace("value = 1 }", &format!("value = {bad} }}")),
            )
        });
        let errs = errors(&load_report(&files, STRICT));
        assert!(
            errs.iter().any(|e| e.contains("a whole number from 0 to 255")),
            "{bad}: {errs:#?}"
        );
    }
}
