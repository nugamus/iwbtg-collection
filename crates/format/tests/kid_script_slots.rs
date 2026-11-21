//! Kid script slots in `physics.toml` (decision 0004, #1074): `[script] file`/`vars`, the slot
//! table in an event list and a collision row, resolved at load.

use crate::wp7a_load::*;
use iwbtg_format::PackageFiles;
use iwbtg_format::load::load_report;
use iwbtg_sim::def::physics::{KidScriptSlot, ScriptSlotDo};
use iwbtg_sim::kid::{KidAction, KidCollisionRun};

/// The basic package's `physics.toml` with a dash script and its slots.
fn profile(events: &str, script: &str) -> PackageFiles {
    package(|f| {
        set(
            f,
            "physics.toml",
            format!("preset = \"A1\"\n{script}\n[events]\n{events}"),
        );
        f.push((
            "scripts/kid.rhai".into(),
            b"fn dash() { }\nfn touch(o) { }\n".to_vec(),
        ));
    })
}

#[test]
fn a_slot_resolves_the_file_and_interns_its_vars() {
    let files = profile(
        "step = [\"gravity\", { do = \"script\", fn = \"dash\" }]\n\
         collisions = [{ with = \"block\", run = \"block_standard\" }, { with = \"block\", run = { do = \"script\", fn = \"touch\" } }]",
        "[script]\nfile = \"kid\"\nvars = [\"dash_dir\", \"dash_dir\"]",
    );
    let (def, _) = load_ok(&files, STRICT);
    let sid = def.physics.script.file.expect("resolved script id");
    assert_eq!(def.scripts[sid.index().expect("index")].name, "scripts/kid.rhai");
    assert_eq!(def.physics.script.vars, ["dash_dir"], "deduped in written order");
    assert!(
        def.var_names.contains_key("dash_dir"),
        "`[script] vars` are interned at load"
    );
    assert!(matches!(
        def.physics.events.step.last(),
        Some(KidAction::Script(KidScriptSlot { action: ScriptSlotDo::Script, function }))
            if function == "dash"
    ));
    assert!(matches!(
        def.physics.events.collisions.last().map(|c| &c.run),
        Some(KidCollisionRun::Script(KidScriptSlot { function, .. })) if function == "touch"
    ));
}

#[test]
fn a_slot_naming_a_function_the_script_lacks_is_an_error() {
    let files = profile(
        "step = [{ do = \"script\", fn = \"missing\" }]",
        "[script]\nfile = \"kid\"",
    );
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.contains("has no function `missing`") && e.contains("physics.toml")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_slot_with_the_wrong_arity_is_an_error() {
    // `touch` takes one argument, so a list-event slot cannot use it.
    let files = profile(
        "step = [{ do = \"script\", fn = \"touch\" }]",
        "[script]\nfile = \"kid\"",
    );
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|e| e.contains("takes 1 argument, not 0")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_slot_without_a_script_file_is_an_error() {
    let files = profile("step = [{ do = \"script\", fn = \"dash\" }]", "");
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|e| e.contains("names no `[script] file`")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_missing_script_file_is_an_error() {
    let files = package(|f| {
        set(
            f,
            "physics.toml",
            "preset = \"A1\"\n[script]\nfile = \"nowhere\"\n[events]\nstep = [{ do = \"script\", fn = \"dash\" }]\n",
        );
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.contains("`scripts/nowhere.rhai` does not exist")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_slot_table_whose_do_is_not_script_is_a_parse_error() {
    let files = profile(
        "step = [{ do = \"scrpt\", fn = \"dash\" }]",
        "[script]\nfile = \"kid\"",
    );
    let r = load_report(&files, STRICT);
    assert!(!r.ok(), "a typo in `do` is refused: {}", render(&r));
}
