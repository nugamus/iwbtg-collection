//! WP5a.5: the `physics.toml` form of plan 5.2 loads into a draft, and the preset fills it in
//! (docs/format/physics.md). Rely's profile is `docs/format/examples/rely-physics.toml`.

use iwbtg_sim::def::KidFamily;
use iwbtg_sim::def::physics::{KidSpriteNames, PhysicsProfileDraft};
use iwbtg_sim::kid::presets;

const RELY: &str = include_str!("../../../docs/format/examples/rely-physics.toml");
const SCHEMA: &str = include_str!("../../../docs/format/schema/v0/physics.schema.json");
const DOC: &str = include_str!("../../../docs/format/physics.md");

fn draft(text: &str) -> Result<PhysicsProfileDraft, toml::de::Error> {
    toml::from_str(text)
}

#[test]
fn rely_written_out_in_full_equals_the_a1_preset() {
    let mut d = draft(RELY).expect("rely's physics.toml parses");
    let sprites = d.sprites.clone().expect("rely names its sprites");
    assert_eq!(
        sprites,
        KidSpriteNames {
            idle: Some("kid_idle".into()),
            running: Some("kid_run".into()),
            jump: Some("kid_jump".into()),
            fall: Some("kid_fall".into()),
            sliding: Some("kid_slide".into()),
        }
    );
    let p = presets::apply(&mut d);
    assert_eq!(p, presets::defaults(KidFamily::A1));
}

#[test]
fn the_preset_alone_is_rely() {
    let mut d = draft("preset = \"A1\"\n").expect("parses");
    assert_eq!(presets::apply(&mut d), presets::defaults(KidFamily::A1));
    let mut d = draft("").expect("an empty file parses");
    assert_eq!(presets::apply(&mut d), presets::defaults(KidFamily::A1));
}

#[test]
fn a_game_overrides_single_fields_in_toml() {
    let mut d = draft(
        r#"
        preset = "A1"
        [numbers]
        run_speed = 5
        [jump]
        ground_sets_djump = false
        ground_classes = ["block"]
        [events]
        step = ["walk", "jump"]
        [vine]
        kick = [10, -8]
        "#,
    )
    .expect("parses");
    let p = presets::apply(&mut d);
    let base = presets::defaults(KidFamily::A1);
    assert_eq!(p.numbers.run_speed.get(), 5.0);
    assert_eq!(p.numbers.jumps, base.numbers.jumps);
    assert!(!p.jump.ground_sets_djump);
    assert_eq!(p.jump.ground_classes, iwbtg_sim::def::ClassMask::BLOCK);
    assert_eq!(p.events.step.len(), 2);
    assert_eq!(p.events.create, base.events.create);
    assert_eq!((p.vine.kick[0].get(), p.vine.kick[1].get()), (10.0, -8.0));
}

#[test]
fn mistakes_are_load_errors_not_silent_defaults() {
    for (what, text) in [
        ("an unknown key", "[numbers]\nrun_spead = 3\n"),
        ("an unknown section", "[nubmers]\n"),
        ("an unknown action", "[events]\nstep = [\"wlak\"]\n"),
        ("an unknown class", "[jump]\nground_classes = [\"blokc\"]\n"),
        (
            "an unknown run",
            "[events]\ncollisions = [{ with = \"block\", run = \"nope\" }]\n",
        ),
        (
            "an unknown key name",
            "[events]\nkeys = [{ action = \"nokey\", on = \"press\", run = [] }]\n",
        ),
        ("an unknown enum value", "[input]\nwalk = \"telepathy\"\n"),
        ("a string for a number", "[numbers]\ngravity = \"heavy\"\n"),
    ] {
        assert!(draft(text).is_err(), "{what} should be an error");
    }
}

#[test]
fn the_docs_and_schema_name_every_section() {
    for section in [
        "numbers", "input", "events", "jump", "fall_cap", "gravity", "platform", "vine", "water", "ice",
        "slope", "ladder", "flip", "death", "bullet", "save", "misc", "sprites",
    ] {
        assert!(
            SCHEMA.contains(&format!("\"{section}\"")),
            "schema lacks {section}"
        );
        assert!(
            DOC.contains(&format!("`[{section}]`")),
            "physics.md lacks [{section}]"
        );
    }
}

#[test]
fn every_action_and_run_name_is_documented() {
    for name in [
        "create_defaults",
        "ensure_world",
        "create_bow",
        "first_play_save",
        "arm_play_timer",
        "gravity",
        "walk",
        "air_sprite",
        "fall_cap",
        "shoot",
        "jump",
        "release",
        "vines",
        "play_timer",
        "kill",
        "platform_gravity_zero",
        "fade_in",
        "block_standard",
        "platform_standard",
    ] {
        assert!(DOC.contains(&format!("`{name}`")), "physics.md lacks `{name}`");
        assert!(SCHEMA.contains(&format!("\"{name}\"")), "schema lacks \"{name}\"");
    }
}

#[test]
fn the_boundary_model_is_read_and_documented() {
    let mut d = draft("[events]\nboundary = \"none\"\n").expect("parses");
    let p = presets::apply(&mut d);
    assert_eq!(p.events.boundary, iwbtg_sim::def::physics::BoundaryModel::None);
    let mut d = draft("").expect("parses");
    assert_eq!(
        presets::apply(&mut d).events.boundary,
        iwbtg_sim::def::physics::BoundaryModel::IntersectOrigin,
        "the A1 preset handles Intersect Boundary"
    );
    assert!(draft("[events]\nboundary = \"sideways\"\n").is_err());
    for name in ["none", "intersect_origin", "outside_room_kill", "end_step_origin"] {
        assert!(SCHEMA.contains(&format!("\"{name}\"")), "schema lacks {name}");
        assert!(DOC.contains(&format!("`{name}`")), "physics.md lacks `{name}`");
    }
}
