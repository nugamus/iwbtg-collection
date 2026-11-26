//! Loader tests for objects and built-in params (docs/format/objects.md, builtins.md): tier 1-3
//! built-in params, derived kinds, scripted objects. Split from `load.rs` (#792), whose
//! package helpers they use.

use crate::load::*;
use crate::load_engine_packs::with_pack;
use iwbtg_format::load::load_report;
use iwbtg_format::{PackageFiles, Severity};
use iwbtg_sim::def::{ClassMask, EngineKind, GameDef, ParamValue, RoomId};
use iwbtg_sim::num::Real;

/// The WP7a order with a `save` object appended (`@saver`, `base = "save"`).
const ORDER_SAVE: &str = r#"object_order = [
  "world", "kid", "block", { kind = "spike", dir = "up" }, "kid_start", "warp", "@boss",
  { kind = "spike", dir = "down" }, "spike", "kid_bullet", "blood", "blood_emitter", "game_over",
  "bow", "playerface", "@saver",
]"#;

/// The WP7a package with `once = true` (a little-trap move param) on a `save` object, under
/// package id `id`.
fn save_once_package(id: &str) -> PackageFiles {
    package(|f| {
        let text = manifest(ORDER_SAVE, "").replace("id = \"wp7a_basic\"", &format!("id = \"{id}\""));
        set(f, "game.toml", text);
        f.push((
            "objects/saver.toml".into(),
            b"base = \"save\"\nonce = true\n".to_vec(),
        ));
    })
}

#[test]
fn a_legacy_param_outside_its_game_is_a_warning() {
    let report = load_report(&save_once_package("wp7a_basic"), STRICT);
    assert!(report.ok(), "{}", render(&report));
    let warning = report
        .diagnostics
        .iter()
        .find(|d| d.message.contains("format-0 legacy alias"))
        .unwrap_or_else(|| panic!("no legacy warning:\n{}", render(&report)));
    assert_eq!(warning.severity, Severity::Warning);
    assert!(warning.message.contains("little-trap"), "{}", warning.message);
    assert!(warning.message.contains("#1269"), "{}", warning.message);
    assert!(warning.message.contains("wp7a_basic"), "{}", warning.message);
}

#[test]
fn a_legacy_param_for_its_game_does_not_warn() {
    let report = load_report(&save_once_package("little_trap"), STRICT);
    assert!(report.ok(), "{}", render(&report));
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| !d.message.contains("format-0 legacy alias")),
        "{}",
        render(&report)
    );
}

/// The same package rewritten as format 1: `ruleset` for `engine_rev` and a `[compat]` profile.
fn save_once_package_format_one() -> PackageFiles {
    package(|f| {
        let text = manifest(ORDER_SAVE, "\n[compat]\nprofile = \"gm8/a1@1\"\n")
            .replace("format = 0\n", "format = 1\n")
            .replace("engine_rev = 0\n", "ruleset = \"gm8@0\"\n");
        set(f, "game.toml", text);
        f.push((
            "objects/saver.toml".into(),
            b"base = \"save\"\nonce = true\n".to_vec(),
        ));
    })
}

#[test]
fn a_legacy_param_outside_its_game_warns_in_format_one_too() {
    let report = load_report(&save_once_package_format_one(), STRICT);
    assert!(report.ok(), "{}", render(&report));
    let warning = report
        .diagnostics
        .iter()
        .find(|d| d.message.contains("format-0 legacy alias"))
        .unwrap_or_else(|| panic!("no legacy warning in format 1:\n{}", render(&report)));
    assert_eq!(warning.severity, Severity::Warning);
    assert!(warning.message.contains("#1269"), "{}", warning.message);
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

const MY_WORLD: &str = "base = \"world\"\nlatch_cleared_by_world = true\npause = \"deactivate\"\npause_delay = 5\nmute = true\ntimer_per_tick = 2\ntimer_play_rooms_only = true\ntimer_needs_fps = true\ntimer_stops_on_clear = true\nclear_triggers_to = 10\nclear_switches_on_room_end = true\n";

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
        ("clear_triggers_to", 10.0),
    ] {
        assert_eq!(w.params.0.get(k), Some(&real(v)), "{k}");
    }
    for k in [
        "latch_cleared_by_world",
        "mute",
        "clear_switches_on_room_end",
        "timer_play_rooms_only",
        "timer_needs_fps",
        "timer_stops_on_clear",
    ] {
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
pub(crate) fn tier2_package(
    extra_instances: &str,
    edit: impl FnOnce(&mut Vec<(String, Vec<u8>)>),
) -> PackageFiles {
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

pub(crate) fn placed<'d>(
    def: &'d GameDef,
    room: &str,
    kind: &str,
) -> Vec<&'d iwbtg_sim::def::PlacedInstance> {
    let (id, _) = kind_named(def, kind);
    let room = def.rooms.iter().find(|r| r.name == room).expect("room");
    room.instances.iter().filter(|i| i.kind == id).collect()
}

/// #324: a slope kind's `shape` and `scale_x` pick its class; rising left is `slope_mirror`
/// (eat-the-new-year yaneM is `scale_x = -1`, CR10; kid/A4.md, kid/A1.md RBF).
#[test]
fn slope_shape_and_scale_x_pick_the_kind_class() {
    let objects: [(&str, &[u8]); 6] = [
        (
            "plain",
            b"base = \"slope\"
",
        ),
        (
            "right",
            b"base = \"slope\"
shape = \"rise_right\"
",
        ),
        (
            "left",
            b"base = \"slope\"
shape = \"rise_left\"
",
        ),
        (
            "sprite",
            b"base = \"slope\"
shape = \"precise_sprite\"
",
        ),
        (
            "yane_m",
            b"base = \"slope\"
scale_x = -1
",
        ),
        (
            "left_flipped",
            b"base = \"slope\"
shape = \"rise_left\"
scale_x = -1
",
        ),
    ];
    let files = tier2_package("", |f| {
        for (id, text) in objects {
            f.push((format!("objects/{id}.toml"), text.to_vec()));
        }
    });
    let (def, _) = load_ok(&files, STRICT);
    let class = |id: &str| kind_named(&def, &format!("@{id}")).1.classes;
    assert_eq!(class("plain"), ClassMask::SLOPE);
    assert_eq!(class("right"), ClassMask::SLOPE);
    assert_eq!(class("sprite"), ClassMask::SLOPE);
    assert_eq!(class("left"), ClassMask::SLOPE_MIRROR);
    assert_eq!(class("yane_m"), ClassMask::SLOPE_MIRROR);
    assert_eq!(class("left_flipped"), ClassMask::SLOPE);
}

/// #659: params that pick a class (`objects::class_params`) on a placed instance give it those
/// classes through a kind derived for them, ranked like its kind; params that pick the kind's
/// own classes keep the kind.
#[test]
fn placed_class_params_derive_a_kind_with_those_classes() {
    let order = ORDER.replace(
        "\"playerface\",",
        "\"playerface\", \"water\", \"vine\", \"killer\", \"slope\", \"@yane_m\",",
    );
    let instances = concat!(
        "  { kind = \"water\", x = 100, y = 100, variant = 2 },
",
        "  { kind = \"water\", x = 132, y = 100, variant = 2 },
",
        "  { kind = \"water\", x = 164, y = 100, variant = 1 },
",
        "  { kind = \"vine\", x = 100, y = 140, side = \"left\" },
",
        "  { kind = \"block\", x = 100, y = 180, kid_collides = false },
",
        "  { kind = \"killer\", x = 100, y = 220, kill_outside_view = true },
",
        "  { kind = \"slope\", x = 100, y = 260, shape = \"rise_left\" },
",
        "  { kind = \"slope\", x = 132, y = 260, shape = \"rise_left\", scale_x = -1 },
",
        "  { kind = \"@yane_m\", x = 100, y = 300, scale_x = 1 },
",
    );
    let files = package(|f| {
        set(f, "game.toml", manifest(&order, "").into_bytes());
        let stage = STAGE1.replace(
            "  { kind = \"warp\", x = 768, y = 512, room_to = \"title\" },
",
            &format!(
                "  {{ kind = \"warp\", x = 768, y = 512, room_to = \"title\" }},
{instances}"
            ),
        );
        set(f, "rooms/stage1.toml", stage.into_bytes());
        f.push((
            "objects/yane_m.toml".into(),
            b"base = \"slope\"
scale_x = -1
counts_as = [\"slope_wall\"]
"
            .to_vec(),
        ));
    });
    let (def, _) = load_ok(&files, STRICT);
    let class = |name: &str| kind_named(&def, name).1.classes;

    let water2 = placed(&def, "stage1", "water[variant=2]");
    assert_eq!(water2.len(), 2, "both instances share one derived kind");
    assert_eq!(class("water[variant=2]"), ClassMask::WATER2);
    assert_eq!(
        placed(&def, "stage1", "water").len(),
        1,
        "`variant = 1` picks the kind's own class and keeps the kind"
    );
    assert_eq!(class("vine[side=left]"), ClassMask::VINE_L);
    assert_eq!(placed(&def, "stage1", "vine[side=left]").len(), 1);
    assert_eq!(class("block[kid_collides=false]"), ClassMask::NONE);
    assert_eq!(placed(&def, "stage1", "block[kid_collides=false]").len(), 1);
    let (_, killer) = kind_named(&def, "killer[kill_outside_view=true]");
    assert_eq!(killer.classes, ClassMask::NONE);
    assert_eq!(
        killer.params.0.get("kill_outside_view"),
        Some(&ParamValue::Bool(true)),
        "the derived kind carries the param, so its event set has the kid collision"
    );
    assert_eq!(class("slope[shape=rise_left]"), ClassMask::SLOPE_MIRROR);
    assert_eq!(
        placed(&def, "stage1", "slope").len(),
        1,
        "rise_left mirrored by scale_x -1 is the kind's own class"
    );
    assert_eq!(
        class("@yane_m[scale_x=1]"),
        ClassMask::SLOPE | ClassMask::SLOPE_WALL,
        "a package kind's derived kind keeps its counts_as"
    );
    assert_eq!(class("@yane_m"), ClassMask::SLOPE_MIRROR | ClassMask::SLOPE_WALL);

    let rank = |name: &str| placed(&def, "stage1", name)[0].rank;
    assert_eq!(rank("water[variant=2]"), rank("water"));
    assert_eq!(rank("@yane_m[scale_x=1]"), rank("slope") + 1);
}

/// #853: params that pick the event set (`builtins::event_set_params`) on a placed instance give
/// it that event set through a derived kind, as the class-picking params do; a value that keeps
/// the kind's event set keeps the kind.
#[test]
fn placed_event_set_params_derive_a_kind_with_that_event_set() {
    let order = ORDER.replace("\"playerface\",", "\"playerface\", \"killer\", \"@outside\",");
    let instances = concat!(
        "  { kind = \"killer\", x = 100, y = 100, destroy_outside_room = true },
",
        "  { kind = \"killer\", x = 132, y = 100, destroy_outside_room = false },
",
        "  { kind = \"@outside\", x = 100, y = 140, destroy_outside_room = false },
",
        "  { kind = \"@outside\", x = 132, y = 140, hspeed = 2 },
",
        "  { kind = \"killer\", x = 100, y = 180, bounce = true },
",
        "  { kind = \"block\", x = 100, y = 220, proximity_frame = true },
",
    );
    let files = package(|f| {
        set(f, "game.toml", manifest(&order, "").into_bytes());
        let stage = STAGE1.replace(
            "  { kind = \"warp\", x = 768, y = 512, room_to = \"title\" },
",
            &format!(
                "  {{ kind = \"warp\", x = 768, y = 512, room_to = \"title\" }},
{instances}"
            ),
        );
        set(f, "rooms/stage1.toml", stage.into_bytes());
        f.push((
            "objects/outside.toml".into(),
            b"base = \"killer\"
destroy_outside_room = true
"
            .to_vec(),
        ));
    });
    let (def, _) = load_ok(&files, STRICT);
    let events = |name: &str| kind_named(&def, name).1.events.clone();

    assert!(events("killer[destroy_outside_room=true]").outside_room);
    assert!(!events("killer").outside_room);
    assert_eq!(
        placed(&def, "stage1", "killer[destroy_outside_room=true]").len(),
        1
    );
    assert_eq!(
        placed(&def, "stage1", "killer").len(),
        1,
        "`destroy_outside_room = false` is the kind's own event set and keeps the kind"
    );
    assert!(events("@outside").outside_room);
    assert!(!events("@outside[destroy_outside_room=false]").outside_room);
    assert_eq!(
        placed(&def, "stage1", "@outside[destroy_outside_room=false]").len(),
        1
    );
    assert_eq!(
        placed(&def, "stage1", "@outside").len(),
        1,
        "a param that picks no event set keeps the kind"
    );
    // #835 and #849: the Step these params add belongs to the kind, so they take effect on a
    // placed instance through its derived kind.
    assert!(events("killer[bounce=true]").step);
    assert!(!events("killer").step);
    assert_eq!(placed(&def, "stage1", "killer[bounce=true]").len(), 1);
    assert!(events("block[proximity_frame=true]").step);
    assert_eq!(placed(&def, "stage1", "block[proximity_frame=true]").len(), 1);
    let rank = |name: &str| placed(&def, "stage1", name)[0].rank;
    assert_eq!(rank("killer[destroy_outside_room=true]"), rank("killer"));
}

/// #837: a placed warp's gate lists load as lists (one number is a list of one) and give it a
/// Step through a derived kind; the move params load too.
#[test]
fn placed_warp_gate_lists_load_and_add_step() {
    let gated = concat!(
        "  { kind = \"warp\", x = 700, y = 512, room_to = \"title\", gate_bosses = [1, 2], gate_closed = \"destroy\" },
",
        "  { kind = \"warp\", x = 640, y = 512, room_to = \"title\", gate_items = 3 },
",
        "  { kind = \"warp\", x = 580, y = 512, room_to = \"title\", warp_x = 40, zero_axis = \"keep\", screen_shift = 1, clear_rising = true },
",
    );
    let files = package(|f| {
        let stage = STAGE1.replace(
            "  { kind = \"warp\", x = 768, y = 512, room_to = \"title\" },
",
            &format!(
                "  {{ kind = \"warp\", x = 768, y = 512, room_to = \"title\" }},
{gated}"
            ),
        );
        set(f, "rooms/stage1.toml", stage.into_bytes());
    });
    let (def, _) = load_ok(&files, STRICT);
    let warps: Vec<_> = def
        .kinds
        .iter()
        .filter(|k| k.base == iwbtg_sim::def::Builtin::Warp)
        .collect();
    let stepping: Vec<_> = warps.iter().filter(|k| k.events.step).collect();
    assert_eq!(stepping.len(), 2, "one derived kind per gate: {warps:#?}");
    assert!(!kind_named(&def, "warp").1.events.step);
    assert!(stepping.iter().any(|k| matches!(
        k.params.0.get("gate_bosses"),
        Some(iwbtg_sim::def::ParamValue::List(l)) if l.len() == 2
    )));
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
    // `mode = "room_assign"` picks another event set (a kid collision), so a derived kind (#853).
    let warps: Vec<_> = warps
        .into_iter()
        .chain(placed(&def, "stage1", "edge_warp[mode=room_assign]"))
        .collect();
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

pub(crate) fn tier2_errors(instance: &str) -> Vec<String> {
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

// ---- #139: scripted objects get their events from the script and `collides_with`

pub(crate) fn scripted_package() -> PackageFiles {
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

/// A scripted `@mover` with the object `timeline` key and two moment handlers.
pub(crate) fn timeline_package(timeline: &str) -> PackageFiles {
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
            format!("script = \"mover\"\nsprite = \"block\"\ntimeline = \"{timeline}\"\n").into_bytes(),
        ));
        f.push((
            "timelines/phases.toml".into(),
            b"moments = [{ t = 0, call = \"intro\" }, { t = 2, call = \"wave\" }]\n".to_vec(),
        ));
        f.push((
            "scripts/mover.rhai".into(),
            b"fn create() { this.x = 100.0; }\nfn moment_intro() { this.y = 42.0; }\nfn moment_wave() { this.y = 24.0; }\n"
                .to_vec(),
        ));
    })
}

#[test]
fn an_object_timeline_key_loads_into_the_kind_and_its_moments_reach_the_script() {
    use iwbtg_sim::input::InputFrame;

    let (def, _) = load_ok(&timeline_package("phases"), STRICT);
    let (mover_kind, mover) = kind_named(&def, "@mover");
    let timeline = mover.timeline.expect("@mover starts a timeline");
    assert_eq!(def.timelines[timeline.index().unwrap()].name, "phases");
    assert!(!mover.params.0.contains_key("timeline"), "the key is not a param");
    assert_eq!(
        mover
            .events
            .moments
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["intro", "wave"],
        "the script's moment handlers are in the kind's event set"
    );

    let stage = RoomId(def.rooms.iter().position(|r| r.name == "stage1").unwrap() as u32);
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
    let y = |sim: &iwbtg_sim::Sim| {
        sim.world()
            .instances()
            .iter()
            .find(|i| i.kind == mover_kind)
            .expect("mover")
            .m
            .y()
            .get()
    };
    assert_eq!(y(&sim), 100.0, "created at y 100, no moment yet");
    sim.step(InputFrame::EMPTY);
    assert_eq!(y(&sim), 42.0, "moment_intro runs on the first tick");
    sim.step(InputFrame::EMPTY);
    sim.step(InputFrame::EMPTY);
    assert_eq!(y(&sim), 24.0, "moment_wave runs at t = 2");
}

#[test]
fn an_unknown_object_timeline_is_an_error() {
    let r = load_report(&timeline_package("missing"), STRICT);
    assert!(!r.ok());
    assert!(
        render(&r).contains("unknown timeline `missing`"),
        "{}",
        render(&r)
    );
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

// ---- #1075: 96 package classes -------------------------------------------------------------

/// Kinds that each declare a distinct package class through `counts_as`, listed in
/// `object_order`.
fn package_class_kinds(n: u32) -> Vec<(String, String)> {
    (0..n)
        .map(|i| {
            (
                format!("class_kind_{i}"),
                format!("base = \"killer\"\ncounts_as = [\"class_{i}\"]\n"),
            )
        })
        .collect()
}

fn load_package_classes(n: u32) -> iwbtg_format::load::LoadReport {
    let objects = package_class_kinds(n);
    let refs: Vec<(&str, &str)> = objects.iter().map(|(s, t)| (s.as_str(), t.as_str())).collect();
    load_report(&wp6b2_package(&refs, false), STRICT)
}

/// Decision 0004: the 17th package class (bit 48) is no longer a load error.
#[test]
fn seventeen_package_classes_load() {
    let r = load_package_classes(17);
    assert!(
        !errors(&r).iter().any(|e| e.contains("too many package classes")),
        "{}",
        render(&r)
    );
}

/// The 97th distinct package class is the new error, and it names the new cap.
#[test]
fn the_97th_package_class_is_an_error() {
    let r = load_package_classes(97);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.contains("too many package classes") && e.contains("at most 96")),
        "{}",
        render(&r)
    );
}

// ---- #1153 E3: classes and the draw key are declared data --------------------------------

/// #1153 E3: a class is a collision set from declared data and a draw layer is the declared
/// depth; neither is an instance's numeric id. Both live in package data, so a change to either
/// is visible in `gameplay_hash`.
#[test]
fn class_and_draw_data_changes_move_the_gameplay_hash() {
    let hash = |files: &PackageFiles| load_report(files, STRICT).gameplay_hash.expect("loads");
    let base = package(|_| {});

    // The object's `depth` is the draw-order key.
    let deeper = package(|f| {
        let boss = f.iter_mut().find(|(p, _)| p == "objects/boss.toml").unwrap();
        boss.1 = b"base = \"killer\"\nscript = \"boss\"\nsprite = \"boss\"\ndepth = -6\ncounts_as = [\"boss_part\", \"solidblock\"]\n".to_vec();
    });
    assert_ne!(hash(&base), hash(&deeper), "declared depth is in the hash");
    let (def, _) = load_ok(&deeper, STRICT);
    assert_eq!(kind_named(&def, "@boss").1.depth, Real::new(-6.0));

    // The object's `counts_as` is its declared collision class set.
    let other_class = package(|f| {
        let boss = f.iter_mut().find(|(p, _)| p == "objects/boss.toml").unwrap();
        boss.1 = b"base = \"killer\"\nscript = \"boss\"\nsprite = \"boss\"\ndepth = -5\ncounts_as = [\"boss_part\"]\n".to_vec();
    });
    assert_ne!(
        hash(&base),
        hash(&other_class),
        "declared classes are in the hash"
    );
    let (def, _) = load_ok(&other_class, STRICT);
    let boss = kind_named(&def, "@boss").1;
    assert!(boss.classes.contains(ClassMask::KILLER));
    assert!(!boss.classes.contains(ClassMask::SOLIDBLOCK));

    // A room-file instance `depth` override is declared data too.
    let moved = package(|f| {
        let room = f.iter_mut().find(|(p, _)| p == "rooms/stage1.toml").unwrap();
        let text = String::from_utf8(room.1.clone())
            .unwrap()
            .replace("depth = 5", "depth = 6");
        room.1 = text.into_bytes();
    });
    assert_ne!(
        hash(&base),
        hash(&moved),
        "an instance depth override is in the hash"
    );
}

#[test]
fn an_object_timeline_is_a_reserved_key_not_a_param() {
    let o = iwbtg_format::objects::parse_object(
        "m",
        "objects/m.toml",
        "timeline = \"phases\"
",
    )
    .unwrap();
    assert_eq!(o.timeline.unwrap().get_ref(), "phases");
    assert!(o.params.is_empty());
    let e = iwbtg_format::objects::parse_object(
        "m",
        "objects/m.toml",
        "timeline = \"a\"
[params]
timeline = \"b\"
",
    )
    .unwrap_err();
    assert!(format!("{e:?}").contains("param `timeline` is set twice or is a reserved key"));
    let e = iwbtg_format::objects::parse_object(
        "m",
        "objects/m.toml",
        "timeline = 3
",
    )
    .unwrap_err();
    assert!(format!("{e:?}").contains("`timeline` must be a string"));
}
