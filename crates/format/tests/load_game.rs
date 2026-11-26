//! Loader tests for the `game.toml` parts (docs/format/game.md, validation.md): maps, reachable
//! timing rooms, collision handler ties, stripped mode. Split from `load.rs` (#792), whose
//! package helpers they use.

use crate::load::*;
use crate::load_objects::scripted_package;
use iwbtg_format::load::load_report;
use iwbtg_format::{PackageFiles, Severity};
use iwbtg_sim::timing::{TimingDef, TimingTrigger};

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
    assert_eq!(report.maps[0].timing, *def.map_timing(0));
    // The loader keeps the chains in `maps` only (#1472): no duplicate in `def.timing`.
    assert_eq!(def.timing, TimingDef::default());
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
    assert_eq!(report.maps[0].timing, *report.def.as_ref().unwrap().map_timing(0));
    assert_eq!(
        report.def.as_ref().unwrap().timing,
        TimingDef::default(),
        "a loaded def does not duplicate its map chain (#1472)"
    );
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

// ---- #504: timing rooms are reachable ----------------------------------------------------------

const LOST_END: &str = r#"
[[maps]]
id = "any"
name = "Any%"

[[maps]]
id = "lost"
name = "Lost"
end = { trigger = "enter_room", room = "lost" }
"#;

fn reach_warnings(edit: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> Vec<String> {
    let files = package(|f| {
        set(f, "game.toml", manifest(ORDER, LOST_END));
        f.push(("rooms/lost.toml".into(), b"size = [800, 608]\n".to_vec()));
        edit(f);
    });
    let report = load_report(&files, STRICT);
    assert!(report.ok(), "a warning, not an error:\n{}", render(&report));
    report
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning && d.message.contains("cannot be reached"))
        .map(ToString::to_string)
        .collect()
}

#[test]
fn a_map_end_room_with_no_way_in_warns() {
    let w = reach_warnings(|_| {});
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].starts_with("game.toml:"), "{w:?}");
    assert!(w[0].contains("`lost`"), "{w:?}");
}

#[test]
fn a_warp_into_the_room_makes_it_reachable() {
    let stage1 = STAGE1.replace("room_to = \"title\"", "room_to = \"lost\"");
    assert!(reach_warnings(|f| set(f, "rooms/stage1.toml", stage1)).is_empty());
}

#[test]
fn a_scripted_kind_naming_the_room_in_text_makes_it_reachable() {
    // A converter's `@warp_1` with the original's `roomTo`: script params are untyped.
    let stage1 = STAGE1.replace(
        "  { kind = \"warp\"",
        "  { kind = \"@door\", x = 0, y = 0, roomTo = \"lost\" },\n  { kind = \"warp\"",
    );
    let order = ORDER.replace("\"@boss\",", "\"@boss\", \"@door\",");
    assert!(
        reach_warnings(|f| {
            set(f, "game.toml", manifest(&order, LOST_END));
            set(f, "rooms/stage1.toml", stage1);
            f.push(("objects/door.toml".into(), b"script = \"door\"\n".to_vec()));
            f.push(("scripts/door.rhai".into(), b"fn create() {}\n".to_vec()));
        })
        .is_empty()
    );
}

/// `reach_warnings` with a `@door` placed in stage1 whose script is `script` (#731).
fn door_warnings(script: &str, place: bool) -> Vec<String> {
    let order = ORDER.replace("\"@boss\",", "\"@boss\", \"@door\",");
    let stage1 = STAGE1.replace(
        "  { kind = \"warp\"",
        "  { kind = \"@door\", x = 0, y = 0 },\n  { kind = \"warp\"",
    );
    reach_warnings(|f| {
        set(f, "game.toml", manifest(&order, LOST_END));
        if place {
            set(f, "rooms/stage1.toml", stage1);
        }
        f.push(("objects/door.toml".into(), b"script = \"door\"\n".to_vec()));
        f.push(("scripts/door.rhai".into(), script.as_bytes().to_vec()));
    })
}

#[test]
fn a_script_room_call_makes_the_room_reachable() {
    // Only the script leads into `lost`: a room literal, or the next room in room order
    // (title, stage1, then lost in path order).
    assert!(door_warnings("fn step() { room_goto(\"lost\"); }\n", true).is_empty());
    assert!(door_warnings("fn step() { room_goto_next(); }\n", true).is_empty());
    // A kind no room places: its literal is a root, its next call leads nowhere known.
    assert!(door_warnings("fn step() { room_goto(\"lost\"); }\n", false).is_empty());
    assert_eq!(door_warnings("fn step() { room_goto_next(); }\n", false).len(), 1);
    // Previous from stage1 is title, and a restart goes nowhere new.
    assert_eq!(
        door_warnings("fn step() { room_goto_previous(); room_restart(); }\n", true).len(),
        1
    );
}

/// `reach_warnings` with a `@door` placed in stage1 whose script creates `@lever` by `create`,
/// and `@lever`'s script `lever` (#751).
fn lever_warnings(create: &str, lever: &str) -> Vec<String> {
    let order = ORDER.replace("\"@boss\",", "\"@boss\", \"@door\", \"@lever\",");
    let stage1 = STAGE1.replace(
        "  { kind = \"warp\"",
        "  { kind = \"@door\", x = 0, y = 0 },
  { kind = \"warp\"",
    );
    let door = format!(
        "fn create() {{ {create} }}
"
    );
    reach_warnings(|f| {
        set(f, "game.toml", manifest(&order, LOST_END));
        set(f, "rooms/stage1.toml", stage1);
        f.push((
            "objects/door.toml".into(),
            b"script = \"door\"
"
            .to_vec(),
        ));
        f.push(("scripts/door.rhai".into(), door.into_bytes()));
        f.push((
            "objects/lever.toml".into(),
            b"script = \"lever\"
"
            .to_vec(),
        ));
        f.push(("scripts/lever.rhai".into(), lever.as_bytes().to_vec()));
    })
}

#[test]
fn a_script_created_kind_runs_its_room_calls_in_the_creating_room() {
    // `@lever` is only created by `@door`'s script, placed in stage1: its next leads from stage1
    // to lost, the only way in.
    let next = "fn step() { room_goto_next(); }
";
    assert!(lever_warnings("instance_create(\"@lever\", 0, 0);", next).is_empty());
    // A computed name says nothing about the room, and previous from stage1 is title.
    assert_eq!(
        lever_warnings("let k = \"@lever\"; instance_create(k, 0, 0);", next).len(),
        1
    );
    assert_eq!(
        lever_warnings(
            "instance_create(\"@lever\", 0, 0);",
            "fn step() { room_goto_previous(); }
"
        )
        .len(),
        1
    );
}

#[test]
fn a_ranked_start_or_cut_room_makes_it_reachable() {
    let start = format!("{LOST_END}\n[stripped]\nstart = {{ room = \"lost\" }}\n");
    assert!(reach_warnings(|f| set(f, "game.toml", manifest(ORDER, &start))).is_empty());
    let cut = format!("{LOST_END}\n[stripped.cut]\nrooms = [{{ room = \"title\", to = \"lost\" }}]\n");
    assert!(reach_warnings(|f| set(f, "game.toml", manifest(ORDER, &cut))).is_empty());
}

#[test]
fn a_room_only_another_unreachable_room_leads_to_still_warns() {
    let lost = "size = [800, 608]\ninstances = [{ kind = \"warp\", x = 0, y = 0, room_to = \"lost\" }]\n";
    let w = reach_warnings(|f| f.push(("rooms/island.toml".into(), lost.as_bytes().to_vec())));
    assert_eq!(w.len(), 1, "{w:?}");
}

// ---- #737: kinds that tie for a collision handler ----------------------------------------------

fn tie_warnings(collides_with: &str) -> Vec<String> {
    let files = scripted_package();
    let mut f: Vec<(String, Vec<u8>)> = files.iter().map(|(p, b)| (p.to_owned(), b.to_vec())).collect();
    set(
        &mut f,
        "objects/mover.toml",
        format!("script = \"mover\"\nsprite = \"block\"\ncollides_with = {collides_with}\n"),
    );
    let mut p = PackageFiles::new();
    for (path, bytes) in f {
        p.insert(path, bytes).unwrap();
    }
    let report = load_report(&p, STRICT);
    assert!(report.ok(), "a warning, not an error:\n{}", render(&report));
    report
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning && d.message.contains("equally specific"))
        .map(ToString::to_string)
        .collect()
}

#[test]
fn the_basic_package_has_no_handler_ties() {
    assert!(tie_warnings("[\"kid_bullet\"]").is_empty());
}

#[test]
fn two_handler_classes_of_one_kind_matching_one_other_warn() {
    // `@boss` is `killer | boss_part | solidblock`: `killer` and `boss_part` both rank 0.
    let w = tie_warnings("[\"killer\", \"boss_part\"]");
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].starts_with("objects/boss.toml:"), "{w:?}");
    assert!(
        w[0].contains("kind `@boss`")
            && w[0].contains("of `@mover`, `killer` and `boss_part`: only `killer` runs"),
        "{w:?}"
    );
}

#[test]
fn a_block_parent_handler_is_less_specific_and_does_not_tie() {
    // `solidblock` ranks after `killer` (ClassMask::generality), so `killer` wins outright.
    assert!(tie_warnings("[\"solidblock\", \"killer\"]").is_empty());
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
    assert_eq!(plain.maps[0].timing, *plain.map_timing(0));
    assert_eq!(plain.timing, TimingDef::default());
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
