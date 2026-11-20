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

#[path = "builtins_boss_item.rs"]
mod builtins_boss_item;

#[path = "builtins_drop_flip_edge.rs"]
mod builtins_drop_flip_edge;

#[path = "builtins_kid_modifiers.rs"]
mod builtins_kid_modifiers;

#[path = "builtins_misc.rs"]
mod builtins_misc;

#[path = "builtins_music.rs"]
mod builtins_music;

#[path = "builtins_playerface_water.rs"]
mod builtins_playerface_water;

#[path = "builtins_rely_a.rs"]
mod builtins_rely_a;

#[path = "builtins_rely_b.rs"]
mod builtins_rely_b;

#[path = "builtins_rely_c.rs"]
mod builtins_rely_c;

#[path = "builtins_tier1.rs"]
mod builtins_tier1;

#[path = "builtins_tier3.rs"]
mod builtins_tier3;

#[path = "builtins_trap_block.rs"]
mod builtins_trap_block;

#[path = "builtins_world.rs"]
mod builtins_world;

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

#[path = "kid_a2_a3.rs"]
mod kid_a2_a3;

#[path = "kid_a4_a5.rs"]
mod kid_a4_a5;

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

#[path = "kid_ice_ladder.rs"]
mod kid_ice_ladder;

#[path = "kid_moving_block.rs"]
mod kid_moving_block;

#[path = "kid_restart.rs"]
mod kid_restart;

#[path = "kid_slope.rs"]
mod kid_slope;

#[path = "kid_sounds.rs"]
mod kid_sounds;

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

#[path = "restart_save_view.rs"]
mod restart_save_view;

#[path = "rng.rs"]
mod rng;

#[path = "room.rs"]
mod room;

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

#[path = "timing.rs"]
mod timing;

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
