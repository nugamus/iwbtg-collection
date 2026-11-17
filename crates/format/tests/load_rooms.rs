//! Loader tests for room files (docs/format/rooms.md): common instance keys, creation code rows,
//! instance `params`, GM ids. Split from `wp7a_load.rs` (#792), whose package helpers they use.

use crate::load_objects::{placed, tier2_errors, tier2_package};
use crate::wp7a_load::*;
use iwbtg_format::PackageFiles;
use iwbtg_format::load::load_report;
use iwbtg_sim::def::{ParamValue, RoomAction};
use iwbtg_sim::num::Real;
use iwbtg_sim::world::Value;

// ---- #632: instance `depth` and `visible` are common keys, not params

#[test]
fn instance_depth_and_visible_are_common_keys_on_built_ins() {
    let files = tier2_package(
        concat!(
            "  { kind = \"slope\", x = 260, y = 100, shape = \"rise_left\", depth = 3 },\n",
            "  { kind = \"gravity_flip\", x = 140, y = 100, direction = \"up\", visible = false },\n",
            "  { kind = \"warp\", x = 20, y = 20, room_to = \"title\", depth = -2, visible = true },\n",
        ),
        |_| {},
    );
    let (def, report) = load_ok(&files, STRICT);
    assert!(report.ok(), "{}", render(&report));
    // `rise_left` picks `slope_mirror`, so the instance has the derived kind (#659).
    let slope = placed(&def, "stage1", "slope[shape=rise_left]")[0];
    assert_eq!(slope.depth, Some(Real::new(3.0)));
    assert_eq!(slope.visible, None);
    assert!(!slope.params.0.contains_key("depth"));
    let flip = placed(&def, "stage1", "gravity_flip")[0];
    assert_eq!(flip.visible, Some(false));
    assert_eq!(flip.depth, None);
    assert!(!flip.params.0.contains_key("visible"));
    let warps = placed(&def, "stage1", "warp");
    let w = warps.iter().find(|w| w.depth.is_some()).expect("warp with depth");
    assert_eq!((w.depth, w.visible), (Some(Real::new(-2.0)), Some(true)));
}

#[test]
fn instance_depth_and_visible_are_typed() {
    let errs = tier2_errors(r#"{ kind = "slope", x = 0, y = 0, shape = "rise_left", visible = 1 }"#);
    assert!(errs.iter().any(|e| e.contains("visible")), "{errs:?}");
    let errs = tier2_errors(r#"{ kind = "slope", x = 0, y = 0, shape = "rise_left", depth = "deep" }"#);
    assert!(errs.iter().any(|e| e.contains("depth")), "{errs:?}");
}

// ---- #629: room creation code rows and instance `params`

const ROWS_629: &str = r#"size = [800, 608]
role = "title"
on_create = [
  { do = "set_global", var = "stage", index = { global = "savenum" }, value = 29 },
  { do = "set_global", var = "stage", index = 2, value = 3 },
  { do = "set_global", var = "stage", index = { global = "slot_var" }, value = 4 },
  { do = "move_kid", x = 241, y = 503, when = { difficulty_below = 3 } },
  { do = "save_game", when = { kid_exists = true } },
  { do = "save_death_time" },
  { do = "destroy", kind = "@boss" },
  { do = "play_sound", sound = "theme" },
  { do = "loop_sound", sound = "theme" },
  { do = "stop_sound", sound = "theme" },
  { do = "spawn", kind = "@boss", x = 0, y = 0, when = { kind_absent = "@boss" } },
  { do = "load_game", when = { difficulty_below = 1, kid_exists = false } },
]
"#;

fn rows_package(rows: &str) -> PackageFiles {
    package(|f| {
        set(f, "rooms/title.toml", rows.as_bytes().to_vec());
        f.push(("sounds/theme.ogg".into(), b"ogg".to_vec()));
    })
}

#[test]
fn on_create_rows_629_load() {
    use iwbtg_sim::def::{GlobalIndex, RoomWhen, SoundId};
    let (def, _) = load_ok(&rows_package(ROWS_629), STRICT);
    let title = def.rooms.iter().find(|r| r.name == "title").unwrap();
    let (boss, _) = kind_named(&def, "@boss");
    let slot_var = def.var_names["slot_var"];
    assert!(
        !def.var_names.contains_key("stage"),
        "indexed globals are run-time names"
    );
    let theme = SoundId(def.sounds.iter().position(|s| s.name == "theme").unwrap() as u32);
    let when = |w: RoomWhen, a: RoomAction| RoomAction::When {
        when: w,
        action: Box::new(a),
    };
    assert_eq!(
        title.on_create,
        [
            RoomAction::SetGlobalIndexed {
                var: "stage".into(),
                index: GlobalIndex::Savenum,
                value: Value::Real(Real::new(29.0)),
            },
            RoomAction::SetGlobalIndexed {
                var: "stage".into(),
                index: GlobalIndex::Const(2),
                value: Value::Real(Real::new(3.0)),
            },
            RoomAction::SetGlobalIndexed {
                var: "stage".into(),
                index: GlobalIndex::Global(slot_var),
                value: Value::Real(Real::new(4.0)),
            },
            when(
                RoomWhen {
                    difficulty_below: Some(Real::new(3.0)),
                    ..RoomWhen::default()
                },
                RoomAction::MoveKid {
                    x: Real::new(241.0),
                    y: Real::new(503.0),
                },
            ),
            when(
                RoomWhen {
                    kid_exists: Some(true),
                    ..RoomWhen::default()
                },
                RoomAction::SaveGame,
            ),
            RoomAction::SaveDeathTime,
            RoomAction::Destroy { kind: boss },
            RoomAction::PlaySound {
                sound: theme,
                looping: false,
            },
            RoomAction::PlaySound {
                sound: theme,
                looping: true,
            },
            RoomAction::StopSound { sound: theme },
            when(
                RoomWhen {
                    kind_absent: Some(boss),
                    ..RoomWhen::default()
                },
                RoomAction::Spawn {
                    kind: boss,
                    x: Real::ZERO,
                    y: Real::ZERO,
                    params: Default::default(),
                },
            ),
            when(
                RoomWhen {
                    difficulty_below: Some(Real::ONE),
                    kid_exists: Some(false),
                    kind_absent: None,
                },
                RoomAction::LoadGame,
            ),
        ]
    );
}

#[test]
fn on_create_rows_629_are_checked() {
    let bad = |row: &str| {
        let text = format!("size = [800, 608]\nrole = \"title\"\non_create = [\n  {row},\n]\n");
        let r = load_report(&rows_package(&text), STRICT);
        assert!(!r.ok(), "{row} loaded");
        render(&r)
    };
    assert!(bad(r#"{ do = "destroy", kind = "@nobody" }"#).contains("destroy: `@nobody` is not a kind"));
    assert!(bad(r#"{ do = "play_sound", sound = "nope" }"#).contains("sound `nope` does not exist"));
    assert!(bad(r#"{ do = "save_game", when = {} }"#).contains("`when` needs"));
    assert!(bad(r#"{ do = "save_game", when = { kind_absent = "@x" } }"#).contains("is not a kind"));
    assert!(bad(r#"{ do = "set_global", var = "stage", index = 1.5, value = 1 }"#).contains("whole number"));
    assert!(bad(r#"{ do = "set_global", var = "stage", index = -1, value = 1 }"#).contains("whole number"));
    assert!(bad(r#"{ do = "move_kid", x = 1 }"#).contains("y"));
    assert!(bad(r#"{ do = "save_game", when = { hp_below = 3 } }"#).contains("hp_below"));
}

/// The base package plus a script-only object `@yuno` placed in stage1 as `entry`.
fn yuno_package(entry: &str) -> PackageFiles {
    let order = ORDER.replace("\"playerface\",", "\"playerface\", \"@yuno\",");
    let stage = STAGE1.replace(
        r#"{ kind = "@boss", x = 400, y = 200, name = "boss_a" },"#,
        &format!(
            "{{ kind = \"@boss\", x = 400, y = 200, name = \"boss_a\" }},
  {entry},"
        ),
    );
    package(|f| {
        set(f, "game.toml", manifest(&order, "").into_bytes());
        set(f, "rooms/stage1.toml", stage.into_bytes());
        f.push((
            "objects/yuno.toml".into(),
            b"script = \"boss\"
sprite = \"boss\"
"
            .to_vec(),
        ));
    })
}

#[test]
fn instance_params_table_629() {
    let files = yuno_package(r#"{ kind = "@yuno", x = 0, y = 0, params = { kind = 20 } }"#);
    let (def, _) = load_ok(&files, STRICT);
    let (yuno, _) = kind_named(&def, "@yuno");
    let stage1 = def.rooms.iter().find(|r| r.name == "stage1").unwrap();
    let placed = stage1.instances.iter().find(|i| i.kind == yuno).unwrap();
    assert_eq!(
        placed.params.0.get("kind"),
        Some(&ParamValue::Real(Real::new(20.0)))
    );
    assert!(!placed.params.0.contains_key("params"));

    // `params` holds only names that are instance keys.
    let r = load_report(
        &yuno_package(r#"{ kind = "@yuno", x = 0, y = 0, params = { hp = 3 } }"#),
        STRICT,
    );
    assert!(
        render(&r).contains("write `hp` on the instance"),
        "{}",
        render(&r)
    );
}

/// #680: `gm_id` per placed instance and `[engine].first_runtime_id` are optional; the loader keeps
/// them on the definition (CLONE_SPEC, Instances, "Placed instance ids").
#[test]
fn gm_ids_and_first_runtime_id() {
    // Without the fields the package loads and neither is set.
    let (def, _) = load_ok(&package(|_| {}), STRICT);
    assert_eq!(def.first_runtime_id, None);
    assert!(
        def.rooms
            .iter()
            .flat_map(|r| &r.instances)
            .all(|i| i.gm_id.is_none())
    );

    let with_ids = |first: u32| {
        package(|f| {
            let m = manifest(ORDER, "").replace(
                "float_model = \"f64\"\n",
                &format!("float_model = \"f64\"\nfirst_runtime_id = {first}\n"),
            );
            set(f, "game.toml", m);
            let room = STAGE1
                .replace(
                    "{ kind = \"kid_start\", x = 48, y = 552 }",
                    "{ kind = \"kid_start\", x = 48, y = 552, gm_id = 104180 }",
                )
                .replace(
                    "{ kind = \"@boss\", x = 400, y = 200, name = \"boss_a\" }",
                    "{ kind = \"@boss\", x = 400, y = 200, name = \"boss_a\", gm_id = 104132 }",
                );
            set(f, "rooms/stage1.toml", room);
        })
    };
    let (def, _) = load_ok(&with_ids(105_106), STRICT);
    assert_eq!(def.first_runtime_id, Some(105_106));
    assert_eq!(placed(&def, "stage1", "kid_start")[0].gm_id, Some(104_180));
    assert_eq!(placed(&def, "stage1", "@boss")[0].gm_id, Some(104_132));
    assert!(placed(&def, "stage1", "block").iter().all(|i| i.gm_id.is_none()));

    // Runtime ids come after every placed id.
    let e = errors(&load_report(&with_ids(104_180), STRICT));
    assert!(
        e.iter().any(|m| m.contains("game.toml")
            && m.contains("must be above every placed `gm_id` (room `stage1` places 104180)")),
        "{e:#?}"
    );
    let e = errors(&load_report(&with_ids(2_147_483_648), STRICT));
    assert!(e.iter().any(|m| m.contains("at most 2147483647")), "{e:#?}");
}
