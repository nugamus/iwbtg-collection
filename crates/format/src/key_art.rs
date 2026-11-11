//! Key art: the picture the background system and the game select list show for a game or one
//! of its maps (docs/format/v0.md, Key art). `[key_art]` in `game.toml` names it; without it the
//! art falls back to the first ranked room's background image or colour.
//!
//! The manifest check (`Manifest::validate`) validates the paths and images; [`resolve`] tells
//! the client what to draw. Key art never changes the sim, so it is outside the `gameplay_hash`.

use std::io::Cursor;

use crate::files::PackageFiles;
use crate::manifest::Manifest;
use crate::sprites::{MAX_IMAGE_SIDE, decode_rgba};

/// Recommended key art size in px (16:9, the size the M4 mockups are drawn at).
pub const RECOMMENDED_SIZE: (u32, u32) = (1920, 1080);

/// Smallest key art size without a warning.
pub const MIN_SIZE: (u32, u32) = (1280, 720);

/// What to draw for a game or map, in fallback order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyArt {
    /// The package's key art: a PNG path inside the package, drawn to cover the screen.
    Image(String),
    /// No key art: the first ranked room's background image (`backgrounds/<id>.png`).
    RoomBackground(String),
    /// No key art and no room background image: the first ranked room's `background_color`
    /// (sRGB).
    Colour([u8; 3]),
    /// Nothing to go on: the client draws its own default backdrop.
    None,
}

/// The key art of `map` (a map id; `None` for the game itself): the map's override, else
/// `[key_art].image`, else the first ranked room's background ([`first_ranked_room`]).
///
/// Assumes the package passed `check`; a path or room that is missing here gives the next
/// fallback.
pub fn resolve(files: &PackageFiles, manifest: &Manifest, map: Option<&str>) -> KeyArt {
    if let Some(section) = &manifest.key_art {
        let over = map.and_then(|m| {
            section
                .maps
                .iter()
                .find(|(id, _)| id.get_ref() == m)
                .map(|(_, path)| path.get_ref())
        });
        if let Some(path) = over.or(section.image.as_ref().map(|p| p.get_ref()))
            && files.contains(path)
        {
            return KeyArt::Image(path.clone());
        }
    }
    let Ok(art) = crate::art::load_room_art(files, first_ranked_room(manifest)) else {
        return KeyArt::None;
    };
    if let Some(bg) = art.backgrounds.iter().find(|b| !b.foreground) {
        let path = format!("backgrounds/{}.png", bg.image);
        if files.contains(&path) {
            return KeyArt::RoomBackground(path);
        }
    }
    art.color.map_or(KeyArt::None, KeyArt::Colour)
}

/// The first room of a ranked run, for the key art fallback: `[stripped].start.room` (the ranked
/// start point), else the room of the `[timing].start` trigger when it is `enter_room`, else
/// `[engine].new_game_room`, else `start_room`.
///
/// OPEN (#31, maps): with maps, each map's own start decides its room.
pub fn first_ranked_room(manifest: &Manifest) -> &str {
    if let Some(room) = manifest
        .stripped
        .as_ref()
        .and_then(|s| s.start.as_ref())
        .and_then(|s| s.room.as_ref())
    {
        return room.get_ref();
    }
    let start = &manifest.timing.get_ref().start;
    if start.trigger.get_ref() == "enter_room"
        && let Some(room) = &start.room
    {
        return room.get_ref();
    }
    manifest
        .engine
        .as_ref()
        .and_then(|e| e.get_ref().new_game_room.as_ref())
        .map_or(manifest.start_room.get_ref(), |r| r.get_ref())
}

/// `#rrggbb` as sRGB bytes.
pub fn hex_colour(s: &str) -> Option<[u8; 3]> {
    crate::art::parse_color(s)
}

/// Width and height of a key art PNG. Images within [`MAX_IMAGE_SIDE`] are decoded in full, so
/// damaged pixel data is an error too; a larger image is only measured.
pub fn image_size(png: &[u8]) -> Result<(u32, u32), String> {
    let reader = png::Decoder::new(Cursor::new(png))
        .read_info()
        .map_err(|e| format!("not a readable PNG: {e}"))?;
    let (w, h) = (reader.info().width, reader.info().height);
    if w > MAX_IMAGE_SIDE || h > MAX_IMAGE_SIDE {
        return Ok((w, h));
    }
    decode_rgba(png).map(|(_, w, h)| (w, h))
}

/// True for 16:9 (within 1%) and at least [`MIN_SIZE`].
pub fn is_recommended_size(w: u32, h: u32) -> bool {
    let (w64, h64) = (u64::from(w), u64::from(h));
    w >= MIN_SIZE.0 && h >= MIN_SIZE.1 && (w64 * 9).abs_diff(h64 * 16) * 100 <= h64 * 16
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"format = 0
id = "minimal"
title = "Minimal"
version = "0.1.0"
engine_rev = 1
runner = "gm80"
physics = "physics.toml"
start_room = "title"
max_ticks = 180_000
object_order = ["kid"]
[[creators]]
name = "IWBTG Collection"
role = "author"
[source]
url = "https://example.com/minimal"
permission = "granted"
"#;

    fn package(extra: &str, files: &[(&str, &str)]) -> (PackageFiles, Manifest) {
        let text = format!("{MANIFEST}{extra}");
        let mut pkg = PackageFiles::new();
        for (path, body) in files {
            pkg.insert(*path, body.as_bytes().to_vec()).unwrap();
        }
        (pkg, Manifest::parse(&text).unwrap())
    }

    const TIMING: &str = "[timing]
start = { trigger = \"run_start\" }
end = { trigger = \"run_end\" }
";

    #[test]
    fn map_override_then_image() {
        let extra = format!(
            "{TIMING}[key_art]
image = \"art/key.png\"
maps = {{ hard = \"art/hard.png\" }}
"
        );
        let (files, m) = package(&extra, &[("art/key.png", ""), ("art/hard.png", "")]);
        assert_eq!(resolve(&files, &m, None), KeyArt::Image("art/key.png".into()));
        assert_eq!(
            resolve(&files, &m, Some("medium")),
            KeyArt::Image("art/key.png".into())
        );
        assert_eq!(
            resolve(&files, &m, Some("hard")),
            KeyArt::Image("art/hard.png".into())
        );
    }

    #[test]
    fn falls_back_to_the_first_ranked_room() {
        let room = "size = [800, 608]
background = \"sky\"
background_color = \"#102030\"
";
        // `start_room` when nothing else names the first room.
        let (files, m) = package(TIMING, &[("rooms/title.toml", room), ("backgrounds/sky.png", "")]);
        assert_eq!(first_ranked_room(&m), "title");
        assert_eq!(
            resolve(&files, &m, None),
            KeyArt::RoomBackground("backgrounds/sky.png".into())
        );
        // No background image: the colour.
        let (files, m) = package(TIMING, &[("rooms/title.toml", room)]);
        assert_eq!(resolve(&files, &m, None), KeyArt::Colour([0x10, 0x20, 0x30]));
        // `new_game_room` beats `start_room`; an `enter_room` start beats both.
        let extra = format!(
            "[engine]
new_game_room = \"r1\"
{TIMING}"
        );
        let (files, m) = package(
            &extra,
            &[(
                "rooms/r1.toml",
                "size = [800, 608]
",
            )],
        );
        assert_eq!(first_ranked_room(&m), "r1");
        assert_eq!(resolve(&files, &m, None), KeyArt::None);
        let extra = "[engine]
new_game_room = \"r1\"
[timing]
start = { trigger = \"enter_room\", room = \"r2\" }
end = { trigger = \"run_end\" }
";
        let (_, m) = package(extra, &[]);
        assert_eq!(first_ranked_room(&m), "r2");
    }

    #[test]
    fn sizes() {
        assert!(is_recommended_size(1920, 1080));
        assert!(is_recommended_size(1280, 720));
        assert!(is_recommended_size(1366, 768));
        assert!(!is_recommended_size(1024, 768));
        assert!(!is_recommended_size(960, 540));
    }

    #[test]
    fn colours() {
        assert_eq!(hex_colour("#ff8000"), Some([255, 128, 0]));
        assert_eq!(hex_colour("ff8000"), None);
        assert_eq!(hex_colour("#ff80"), None);
    }
}
