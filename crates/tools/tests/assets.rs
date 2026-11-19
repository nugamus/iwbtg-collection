//! `iwbtg assets index` (#46): keys by content hash, duplicates as `also in`, deterministic.
//!
//! The games repo has no packages yet, so every test builds packages in a temp folder. The
//! command runs as the real binary, like the User check.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use iwbtg_format::PackageFiles;

const IWBTG: &str = env!("CARGO_BIN_EXE_iwbtg");

/// A fresh temp folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iwbtg-assets-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes one package folder: `game.toml` plus the given assets.
fn write_package(root: &Path, game: &str, assets: &[(&str, &[u8])]) {
    let dir = root.join(game);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("game.toml"), format!("format = 0\nid = \"{game}\"\n")).unwrap();
    for (path, bytes) in assets {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }
}

/// Runs the real `iwbtg` binary.
fn iwbtg(args: &[&str]) -> Output {
    let output = Command::new(IWBTG).args(args).output().expect("iwbtg runs");
    assert!(
        output.status.code().is_some(),
        "iwbtg did not exit normally: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// The format's content hash over one file, as the index documents it: lowercase hex.
fn expected_hash(bytes: &[u8]) -> String {
    let mut one = PackageFiles::new();
    one.insert("asset", bytes.to_vec()).unwrap();
    one.content_hash().iter().map(|b| format!("{b:02x}")).collect()
}

/// Two packages sharing one asset and each with a unique one.
fn two_packages(name: &str) -> PathBuf {
    let root = scratch(name);
    write_package(
        &root,
        "game_a",
        &[
            ("sprites/shared.png", b"shared"),
            ("sprites/only_a.png", b"only a"),
        ],
    );
    write_package(
        &root,
        "game_b",
        &[
            ("sprites/shared.png", b"shared"),
            ("sprites/only_b.png", b"only b"),
        ],
    );
    root
}

fn found_pairs(assets: &toml::Table, hash: &str) -> Vec<(String, String)> {
    assets[hash]["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["game"].as_str().unwrap().to_owned(),
                f["path"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn keys_by_content_hash_and_lists_every_game() {
    let root = two_packages("keys");
    let out = iwbtg(&["assets", "index", root.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("indexed 4 assets, 3 unique, 1 duplicate groups"),
        "{stdout}"
    );
    assert!(
        stdout.contains("duplicate: game_a/sprites/shared.png also in game_b"),
        "{stdout}"
    );

    let text = std::fs::read_to_string(root.join("assets-index.toml")).unwrap();
    let index: toml::Value = toml::from_str(&text).unwrap();
    assert_eq!(index["format"].as_integer(), Some(0));
    let assets = index["assets"].as_table().unwrap();
    assert_eq!(assets.len(), 3, "one entry per distinct content");

    let shared = expected_hash(b"shared");
    assert_eq!(
        found_pairs(assets, &shared),
        vec![
            ("game_a".to_owned(), "sprites/shared.png".to_owned()),
            ("game_b".to_owned(), "sprites/shared.png".to_owned()),
        ]
    );
    let entry = &assets[&shared];
    assert_eq!(entry["size"].as_integer(), Some(6));
    assert_eq!(entry["tags"].as_array().unwrap().len(), 0, "tags start empty");

    // The unique assets have one place each and their own keys.
    for (bytes, game, path) in [
        (b"only a".as_slice(), "game_a", "sprites/only_a.png"),
        (b"only b".as_slice(), "game_b", "sprites/only_b.png"),
    ] {
        assert_eq!(
            found_pairs(assets, &expected_hash(bytes)),
            vec![(game.to_owned(), path.to_owned())]
        );
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_rerun_is_deterministic_and_keeps_tags() {
    let root = two_packages("rerun");
    assert_eq!(
        iwbtg(&["assets", "index", root.to_str().unwrap()]).status.code(),
        Some(0)
    );
    let index = root.join("assets-index.toml");
    let first = std::fs::read(&index).unwrap();

    assert_eq!(
        iwbtg(&["assets", "index", root.to_str().unwrap()]).status.code(),
        Some(0)
    );
    assert_eq!(std::fs::read(&index).unwrap(), first, "a rerun changes nothing");

    // Tag all three entries, as the porting checklist will (#104); a rebuild keeps them.
    let text = String::from_utf8(first).unwrap();
    assert!(text.contains("tags = []"), "{text}");
    let tagged = text.replace("tags = []", "tags = [\"kid\"]");
    std::fs::write(&index, &tagged).unwrap();
    assert_eq!(
        iwbtg(&["assets", "index", root.to_str().unwrap()]).status.code(),
        Some(0)
    );
    let rebuilt = std::fs::read_to_string(&index).unwrap();
    assert_eq!(rebuilt.matches("tags = [\"kid\"]").count(), 3, "{rebuilt}");

    let check = iwbtg(&["assets", "index", root.to_str().unwrap(), "--check"]);
    assert_eq!(
        check.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );
    assert!(String::from_utf8_lossy(&check.stdout).contains("up to date"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn check_reports_a_stale_or_missing_index() {
    let root = two_packages("check");
    let games = root.to_str().unwrap();

    let missing = iwbtg(&["assets", "index", games, "--check"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stdout).contains("missing:"));
    assert!(!root.join("assets-index.toml").exists(), "--check writes nothing");

    assert_eq!(iwbtg(&["assets", "index", games]).status.code(), Some(0));
    write_package(&root, "game_a", &[("sounds/new.ogg", b"new")]);
    let stale = iwbtg(&["assets", "index", games, "--check"]);
    assert_eq!(stale.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&stale.stdout);
    assert!(stdout.contains("stale:"), "{stdout}");
    assert!(
        stdout.contains("indexed 5 assets, 4 unique, 1 duplicate groups"),
        "{stdout}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_folder_without_packages_is_an_error() {
    let root = scratch("empty");
    let out = iwbtg(&["assets", "index", root.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no packages"));
    std::fs::remove_dir_all(&root).ok();
}
