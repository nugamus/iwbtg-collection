//! Every integration test of iwbtg-sim in one executable (#517): one test binary links and
//! starts much faster than one per file. Each `tests/<file>.rs` is a module here, so its
//! tests are named `<file>::...`; run one file with `cargo test -p iwbtg-sim --test it <file>::`.
//! Files are named after the module they test (#985); the shared fixtures are `support.rs`
//! and `collide_support/mod.rs`. Kept as their own targets (Cargo.toml): `wp1_determinism`
//! (file `determinism.rs`, the target name CI runs) and `golden_corpus`, which CI runs on
//! wasm targets or which regenerate pinned files.
//!
//! A new test file goes into this list (`autotests = false` in Cargo.toml, so cargo won't find it alone).

// Shared helpers, loaded once here and used by the modules below.

#[path = "support.rs"]
mod support;

#[path = "collide_support/mod.rs"]
mod collide_support;

#[path = "bbox.rs"]
mod bbox;

#[path = "bounce.rs"]
mod bounce;

#[path = "builtins_block.rs"]
mod builtins_block;

#[path = "builtins_block_proximity.rs"]
mod builtins_block_proximity;

#[path = "builtins_blood.rs"]
mod builtins_blood;

#[path = "builtins_blood_emitter.rs"]
mod builtins_blood_emitter;

#[path = "builtins_boss_gate.rs"]
mod builtins_boss_gate;

#[path = "builtins_boss_item.rs"]
mod builtins_boss_item;

#[path = "builtins_boss_item_support.rs"]
mod builtins_boss_item_support;

#[path = "builtins_bow.rs"]
mod builtins_bow;

#[path = "builtins_breakable_block.rs"]
mod builtins_breakable_block;

#[path = "builtins_bullet_blocker.rs"]
mod builtins_bullet_blocker;

#[path = "builtins_camera.rs"]
mod builtins_camera;

#[path = "builtins_conveyor.rs"]
mod builtins_conveyor;

#[path = "builtins_difficulty_warp.rs"]
mod builtins_difficulty_warp;

#[path = "builtins_drop_flip_edge_support.rs"]
mod builtins_drop_flip_edge_support;

#[path = "builtins_drop_platform.rs"]
mod builtins_drop_platform;

#[path = "builtins_edge_warp.rs"]
mod builtins_edge_warp;

#[path = "builtins_fake_block.rs"]
mod builtins_fake_block;

#[path = "builtins_flags.rs"]
mod builtins_flags;

#[path = "builtins_game_clear.rs"]
mod builtins_game_clear;

#[path = "builtins_game_over.rs"]
mod builtins_game_over;

#[path = "builtins_gravity_flip.rs"]
mod builtins_gravity_flip;

#[path = "builtins_hidden_block.rs"]
mod builtins_hidden_block;

#[path = "builtins_item_viewer.rs"]
mod builtins_item_viewer;

#[path = "builtins_jump_refresher.rs"]
mod builtins_jump_refresher;

#[path = "builtins_kid_bullet.rs"]
mod builtins_kid_bullet;

#[path = "builtins_kid_modifiers_support.rs"]
mod builtins_kid_modifiers_support;

#[path = "builtins_kid_param_zone.rs"]
mod builtins_kid_param_zone;

#[path = "builtins_kid_start.rs"]
mod builtins_kid_start;

#[path = "builtins_killer.rs"]
mod builtins_killer;

#[path = "builtins_ladder_decor.rs"]
mod builtins_ladder_decor;

#[path = "builtins_misc_support.rs"]
mod builtins_misc_support;

#[path = "builtins_moving_platform.rs"]
mod builtins_moving_platform;

#[path = "builtins_music.rs"]
mod builtins_music;

#[path = "builtins_music_params.rs"]
mod builtins_music_params;

#[path = "builtins_orbit.rs"]
mod builtins_orbit;

#[path = "builtins_path_platform.rs"]
mod builtins_path_platform;

#[path = "builtins_platform.rs"]
mod builtins_platform;

#[path = "builtins_playerface.rs"]
mod builtins_playerface;

#[path = "builtins_playerface_water_support.rs"]
mod builtins_playerface_water_support;

#[path = "builtins_rely_a_support.rs"]
mod builtins_rely_a_support;

#[path = "builtins_rely_b_support.rs"]
mod builtins_rely_b_support;

#[path = "builtins_rely_c_support.rs"]
mod builtins_rely_c_support;

#[path = "builtins_save.rs"]
mod builtins_save;

#[path = "builtins_secret_item.rs"]
mod builtins_secret_item;

#[path = "builtins_sign.rs"]
mod builtins_sign;

#[path = "builtins_slope.rs"]
mod builtins_slope;

#[path = "builtins_spike.rs"]
mod builtins_spike;

#[path = "builtins_spring.rs"]
mod builtins_spring;

#[path = "builtins_tier1_support.rs"]
mod builtins_tier1_support;

#[path = "builtins_tier3_support.rs"]
mod builtins_tier3_support;

#[path = "builtins_time_limit.rs"]
mod builtins_time_limit;

#[path = "builtins_trap.rs"]
mod builtins_trap;

#[path = "builtins_trap_block_support.rs"]
mod builtins_trap_block_support;

#[path = "builtins_trap_stopper.rs"]
mod builtins_trap_stopper;

#[path = "builtins_trigger.rs"]
mod builtins_trigger;

#[path = "builtins_vine.rs"]
mod builtins_vine;

#[path = "builtins_warp.rs"]
mod builtins_warp;

#[path = "builtins_water.rs"]
mod builtins_water;

#[path = "builtins_world.rs"]
mod builtins_world;

#[path = "builtins_world_events.rs"]
mod builtins_world_events;

#[path = "builtins_world_triggers.rs"]
mod builtins_world_triggers;

#[path = "death_sound.rs"]
mod death_sound;

#[path = "dll_sound_length.rs"]
mod dll_sound_length;

#[path = "empty_mask.rs"]
mod empty_mask;

#[path = "first_step_output.rs"]
mod first_step_output;

#[path = "grid.rs"]
mod grid;

#[path = "hash_vectors.rs"]
mod hash_vectors;

#[path = "kid_a1_actions.rs"]
mod kid_a1_actions;

#[path = "kid_a1_boundary.rs"]
mod kid_a1_boundary;

#[path = "kid_a1_collisions.rs"]
mod kid_a1_collisions;

#[path = "kid_a1_death.rs"]
mod kid_a1_death;

#[path = "kid_a1_gravity.rs"]
mod kid_a1_gravity;

#[path = "kid_a2.rs"]
mod kid_a2;

#[path = "kid_a2_a3_support.rs"]
mod kid_a2_a3_support;

#[path = "kid_a3.rs"]
mod kid_a3;

#[path = "kid_a4.rs"]
mod kid_a4;

#[path = "kid_a4_a5_support.rs"]
mod kid_a4_a5_support;

#[path = "kid_a5.rs"]
mod kid_a5;

#[path = "kid_api.rs"]
mod kid_api;

#[path = "kid_b_collisions.rs"]
mod kid_b_collisions;

#[path = "kid_b_core.rs"]
mod kid_b_core;

#[path = "kid_b_death.rs"]
mod kid_b_death;

#[path = "kid_b_flip.rs"]
mod kid_b_flip;

#[path = "kid_b_preset.rs"]
mod kid_b_preset;

#[path = "kid_b_slope.rs"]
mod kid_b_slope;

#[path = "kid_c.rs"]
mod kid_c;

#[path = "kid_c_key.rs"]
mod kid_c_key;

#[path = "kid_flip_swap.rs"]
mod kid_flip_swap;

#[path = "kid_ice.rs"]
mod kid_ice;

#[path = "kid_ladder.rs"]
mod kid_ladder;

#[path = "kid_motion_support.rs"]
mod kid_motion_support;

#[path = "kid_moving_block.rs"]
mod kid_moving_block;

#[path = "kid_restart.rs"]
mod kid_restart;

#[path = "kid_slope.rs"]
mod kid_slope;

#[path = "kid_sounds.rs"]
mod kid_sounds;

#[path = "kid_temp.rs"]
mod kid_temp;

#[path = "kid_water_setters.rs"]
mod kid_water_setters;

#[path = "lint_gm_compare.rs"]
mod lint_gm_compare;

#[path = "motion.rs"]
mod motion;

#[path = "motion_traces.rs"]
mod motion_traces;

#[path = "num.rs"]
mod num;

#[path = "pairs.rs"]
mod pairs;

#[path = "particles_rng.rs"]
mod particles_rng;

#[path = "path.rs"]
mod path;

#[path = "pipeline.rs"]
mod pipeline;

#[path = "placed_ids.rs"]
mod placed_ids;

#[path = "query.rs"]
mod query;

#[path = "ranked_time.rs"]
mod ranked_time;

#[path = "restart.rs"]
mod restart;

#[path = "rng.rs"]
mod rng;

#[path = "room_persistent.rs"]
mod room_persistent;

#[path = "room_requests.rs"]
mod room_requests;

#[path = "room_restart.rs"]
mod room_restart;

#[path = "room_rows.rs"]
mod room_rows;

#[path = "room_sequence.rs"]
mod room_sequence;

#[path = "room_start.rs"]
mod room_start;

#[path = "room_support.rs"]
mod room_support;

#[path = "save.rs"]
mod save;

#[path = "save_reverse.rs"]
mod save_reverse;

#[path = "script_conformance/main.rs"]
mod script_conformance;

#[path = "script_derived_kinds.rs"]
mod script_derived_kinds;

#[path = "script_doc_examples.rs"]
mod script_doc_examples;

#[path = "script_engine.rs"]
mod script_engine;

#[path = "script_host.rs"]
mod script_host;

#[path = "script_int_args.rs"]
mod script_int_args;

#[path = "script_prep.rs"]
mod script_prep;

#[path = "script_retry.rs"]
mod script_retry;

#[path = "script_validate.rs"]
mod script_validate;

#[path = "shapes.rs"]
mod shapes;

#[path = "sim_version_bump.rs"]
mod sim_version_bump;

#[path = "slip.rs"]
mod slip;

#[path = "sound.rs"]
mod sound;

#[path = "state_support.rs"]
mod state_support;

#[path = "timing.rs"]
mod timing;

#[path = "view.rs"]
mod view;

#[path = "world.rs"]
mod world;

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
