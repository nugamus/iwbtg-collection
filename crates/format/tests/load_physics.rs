//! Loader tests for physics profiles (docs/format/physics.md): the game's profile, kid sprites and
//! sounds, legacy profiles, room profiles and control labels. Split from `load_defs.rs` (#792),
//! whose package helpers they use.

use crate::load_defs::*;
use iwbtg_format::load::load_report;
use iwbtg_sim::def::{GameDef, KidFamily};
use iwbtg_sim::kid::presets;
use iwbtg_sim::num::Real;

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

pub(crate) const LEGACY: &str =
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
