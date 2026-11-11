//! Room art (#462): background layers, the tileset sidecar and tile files are read by
//! `iwbtg_format::art` and checked by the loader, which finds a missing background or tileset
//! image. Each test adds files to `tests/valid/minimal`.

use std::path::Path;

use iwbtg_format::art::{Background, load_room_art, load_tiles};
use iwbtg_format::load::{LoadMode, load_report};
use iwbtg_format::{PackageFiles, Severity};

fn package(extra: &[(&str, &str)]) -> PackageFiles {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/valid/minimal");
    let mut files = PackageFiles::load_dir(&dir).expect("minimal loads");
    for (path, text) in extra {
        files.insert(*path, text.as_bytes().to_vec()).unwrap();
    }
    files
}

/// Every error of a load in a room or background file, as `file:line:column: message`
/// (`minimal` itself has `object_order` errors that are not this test's business).
fn errors(files: &PackageFiles) -> Vec<String> {
    load_report(files, LoadMode::Strict)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .filter(|d| d.file.starts_with("rooms/") || d.file.starts_with("backgrounds/"))
        .map(|d| format!("{}:{}:{}: {}", d.file, d.line, d.column, d.message))
        .collect()
}

const LAYERS: &str = r##"size = [800, 608]
background_color = "#102030"
backgrounds = [
  { image = "sky", speed = [1, 0] },
  { image = "fog", foreground = true, offset = [0, 560], tile = [true, false], stretch = true },
]
instances = []
"##;

#[test]
fn background_layers_are_read_with_their_defaults() {
    let files = package(&[
        ("rooms/title.toml", LAYERS),
        ("backgrounds/sky.png", ""),
        ("backgrounds/fog.png", ""),
    ]);
    assert_eq!(errors(&files), Vec::<String>::new());
    let art = load_room_art(&files, "title").unwrap();
    assert_eq!(art.color, Some([0x10, 0x20, 0x30]));
    let [sky, fog] = &art.backgrounds[..] else {
        panic!("two layers: {art:?}")
    };
    assert_eq!(
        (sky.foreground, sky.offset, sky.speed, sky.tile, sky.stretch),
        (false, (0.0, 0.0), (1.0, 0.0), (true, true), false)
    );
    assert_eq!(
        (fog.foreground, fog.offset, fog.tile, fog.stretch),
        (true, (0.0, 560.0), (true, false), true)
    );
}

#[test]
fn background_is_short_for_one_default_layer() {
    let room = "size = [800, 608]\nbackground = \"sky\"\ninstances = []\n";
    let files = package(&[("rooms/title.toml", room), ("backgrounds/sky.png", "")]);
    let art = load_room_art(&files, "title").unwrap();
    let span = 31..36;
    assert_eq!(art.backgrounds, [Background::new("sky", span)]);
}

#[test]
fn a_missing_background_image_is_an_error_at_its_id() {
    let files = package(&[("rooms/title.toml", LAYERS), ("backgrounds/sky.png", "")]);
    assert_eq!(
        errors(&files),
        ["rooms/title.toml:5:13: background `fog` has no image `backgrounds/fog.png`"]
    );
}

#[test]
fn bad_room_art_keys_are_errors() {
    let room = |keys: &str| format!("size = [800, 608]\n{keys}\ninstances = []\n");
    let one = |keys: &str| {
        let files = package(&[("rooms/title.toml", &room(keys)), ("backgrounds/sky.png", "")]);
        errors(&files).join("\n")
    };
    assert!(one("background_color = \"red\"").contains("is not `#rrggbb`"));
    assert!(one("background = \"sky\"\nbackgrounds = [{ image = \"sky\" }]").contains("not both"));
    assert!(one("backgrounds = [{ image = \"sky\", scroll = 1 }]").contains("unknown field `scroll`"));
    let nine = ["{ image = \"sky\" }"; 9].join(", ");
    assert!(one(&format!("backgrounds = [{nine}]")).contains("at most 8 backgrounds"));
}

const TILES: &str = r#"layers = [
  { depth = 100, tileset = "stone", tiles = [
    { x = 0, y = 0, src = [0, 0], run = [2, 1] },
    { x = 64, y = 0, src = [16, 0, 8, 8], run = [3, 1], step = [10, 0] },
  ] },
]
"#;

#[test]
fn tiles_take_their_size_from_the_tileset_sidecar() {
    let files = package(&[
        ("rooms/title.tiles.toml", TILES),
        ("backgrounds/stone.png", ""),
        (
            "backgrounds/stone.toml",
            "tile_size = [16, 24]\nseparation = [2, 2]\n",
        ),
    ]);
    assert_eq!(errors(&files), Vec::<String>::new());
    let layers = load_tiles(&files, "title").unwrap();
    let [a, b] = &layers[0].tiles[..] else {
        panic!("two tiles")
    };
    assert_eq!((a.size, a.step), ((16.0, 24.0), (16.0, 24.0)));
    assert_eq!((b.size, b.step), ((8.0, 8.0), (10.0, 0.0)));
    assert_eq!(b.positions().last(), Some((84.0, 0.0)));
}

#[test]
fn a_missing_tileset_image_and_a_bad_sidecar_are_errors() {
    let files = package(&[
        ("rooms/title.tiles.toml", TILES),
        ("backgrounds/stone.toml", "tile_size = [0, 24]\n"),
    ]);
    assert_eq!(
        errors(&files),
        [
            "backgrounds/stone.toml:1:13: tile_size must be at least 1 x 1",
            "rooms/title.tiles.toml:2:28: tileset `stone` has no image `backgrounds/stone.png`",
        ]
    );
}
