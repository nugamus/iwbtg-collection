//! Every integration test of iwbtg-sim in one executable (#517): one test binary links and
//! starts much faster than one per file. Each former `tests/<file>.rs` is a module here, so its
//! tests are named `<file>::...`; run one file with `cargo test -p iwbtg-sim --test it <file>::`.
//! Kept as their own targets (Cargo.toml): `wp1_determinism`, `golden_corpus`, which CI runs on wasm targets or
//! which regenerate pinned files.
//!
//! A new test file goes into this list (`autotests = false` in Cargo.toml, so cargo won't find it alone).

// Shared helpers, loaded once here and used by the modules below.
#[path = "wp1_support.rs"]
mod wp1_support;
#[path = "wp3_support/mod.rs"]
mod wp3_support;

#[path = "death_sound.rs"]
mod death_sound;
#[path = "dll_sound_length.rs"]
mod dll_sound_length;
#[path = "first_step_output.rs"]
mod first_step_output;
#[path = "hash_vectors.rs"]
mod hash_vectors;
#[path = "kid_sounds.rs"]
mod kid_sounds;
#[path = "lint_gm_compare.rs"]
mod lint_gm_compare;
#[path = "ranked_time.rs"]
mod ranked_time;
#[path = "save_reverse.rs"]
mod save_reverse;
#[path = "script_conformance/main.rs"]
mod script_conformance;
#[path = "script_int_args.rs"]
mod script_int_args;
#[path = "wp10_sound.rs"]
mod wp10_sound;
#[path = "wp1_pipeline.rs"]
mod wp1_pipeline;
#[path = "wp1_world.rs"]
mod wp1_world;
#[path = "wp2_motion.rs"]
mod wp2_motion;
#[path = "wp2_numbers.rs"]
mod wp2_numbers;
#[path = "wp2_path.rs"]
mod wp2_path;
#[path = "wp2_rng.rs"]
mod wp2_rng;
#[path = "wp2_slip.rs"]
mod wp2_slip;
#[path = "wp2_traces.rs"]
mod wp2_traces;
#[path = "wp3_bbox.rs"]
mod wp3_bbox;
#[path = "wp3_grid.rs"]
mod wp3_grid;
#[path = "wp3_query.rs"]
mod wp3_query;
#[path = "wp3_shapes.rs"]
mod wp3_shapes;
#[path = "wp4a_room.rs"]
mod wp4a_room;
#[path = "wp4b_restart_save_view.rs"]
mod wp4b_restart_save_view;
#[path = "wp5a_actions.rs"]
mod wp5a_actions;
#[path = "wp5a_api.rs"]
mod wp5a_api;
#[path = "wp5a_boundary.rs"]
mod wp5a_boundary;
#[path = "wp5a_collisions.rs"]
mod wp5a_collisions;
#[path = "wp5a_death.rs"]
mod wp5a_death;
#[path = "wp5a_gravity.rs"]
mod wp5a_gravity;
#[path = "wp5a_restart.rs"]
mod wp5a_restart;
#[path = "wp5b_collisions.rs"]
mod wp5b_collisions;
#[path = "wp5b_core.rs"]
mod wp5b_core;
#[path = "wp5b_death.rs"]
mod wp5b_death;
#[path = "wp5b_flip.rs"]
mod wp5b_flip;
#[path = "wp5b_preset.rs"]
mod wp5b_preset;
#[path = "wp5b_slope.rs"]
mod wp5b_slope;
#[path = "wp5c_a2a3.rs"]
mod wp5c_a2a3;
#[path = "wp5c_a4a5.rs"]
mod wp5c_a4a5;
#[path = "wp5c_c.rs"]
mod wp5c_c;
#[path = "wp5c_c_key.rs"]
mod wp5c_c_key;
#[path = "wp5c_flip_swap.rs"]
mod wp5c_flip_swap;
#[path = "wp5c_ice_ladder.rs"]
mod wp5c_ice_ladder;
#[path = "wp5c_moving_block.rs"]
mod wp5c_moving_block;
#[path = "wp5c_slope.rs"]
mod wp5c_slope;
#[path = "wp5c_water_setters.rs"]
mod wp5c_water_setters;
#[path = "wp6a_rely_slice_a.rs"]
mod wp6a_rely_slice_a;
#[path = "wp6a_rely_slice_b.rs"]
mod wp6a_rely_slice_b;
#[path = "wp6a_rely_slice_c.rs"]
mod wp6a_rely_slice_c;
#[path = "wp6a_tier1_rest.rs"]
mod wp6a_tier1_rest;
#[path = "wp6a_world_extras.rs"]
mod wp6a_world_extras;
#[path = "wp6b_boss_item_clear_music.rs"]
mod wp6b_boss_item_clear_music;
#[path = "wp6b_drop_flip_edge_blocker_slope.rs"]
mod wp6b_drop_flip_edge_blocker_slope;
#[path = "wp6b_music_sound.rs"]
mod wp6b_music_sound;
#[path = "wp6b_playerface_platform_water_vine.rs"]
mod wp6b_playerface_platform_water_vine;
#[path = "wp6b_trap_block_trigger.rs"]
mod wp6b_trap_block_trigger;
#[path = "wp6c_misc_objects.rs"]
mod wp6c_misc_objects;
#[path = "wp6c_tier3_kid_modifiers.rs"]
mod wp6c_tier3_kid_modifiers;
#[path = "wp6c_tier3_objects.rs"]
mod wp6c_tier3_objects;
#[path = "wp8b_timing.rs"]
mod wp8b_timing;
#[path = "wp9_doc_examples.rs"]
mod wp9_doc_examples;
#[path = "wp9_engine.rs"]
mod wp9_engine;
#[path = "wp9_host.rs"]
mod wp9_host;
#[path = "wp9_validate.rs"]
mod wp9_validate;

/// `autotests = false` means a new `tests/*.rs` file runs only once it is listed here or in Cargo.toml.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[allow(clippy::disallowed_methods, reason = "reads the test folder, not sim code")]
fn every_test_file_is_listed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let here = include_str!("it.rs");
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(dir.join("tests")).unwrap() {
        let entry = entry.unwrap();
        let mut name = entry.file_name().into_string().unwrap();
        if entry.path().join("main.rs").is_file() {
            name.push_str("/main.rs"); // a directory test target, like tests/script_conformance/
        }
        if name.ends_with(".rs") {
            let listed = here.contains(&format!("#[path = \"{name}\"]"))
                || manifest.contains(&format!("path = \"tests/{name}\""));
            if !listed {
                missing.push(name);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "add these to iwbtg-sim's tests/it.rs (or Cargo.toml): {missing:?}"
    );
}
