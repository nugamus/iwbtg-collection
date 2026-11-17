//! Loader tests for engine packs (docs/format/engine.md). Split from `wp7a_load.rs` (#792), whose
//! package helpers they use.

use crate::wp7a_load::*;
use iwbtg_format::PackageFiles;
use iwbtg_format::load::load_report;

// -- Engine packs (docs/format/engine.md) ------------------------------------------------------

const PACK_TOML: &str = "#:schema ../engine.schema.json\nid = \"a1\"\ntitle = \"Pack A1\"\nfamily = \"A1\"\n\n[[creators]]\nname = \"Someone\"\nrole = \"art\"\n";

/// The basic package with its kid sprite moved into a bundled pack `a1`.
pub(crate) fn with_pack(edit: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> PackageFiles {
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
