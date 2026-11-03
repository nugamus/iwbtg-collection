//! Room art that is not an instance: the background colour and image, and the decorative tile
//! layers (`rooms/<id>.toml` keys `background` and `background_color`, `rooms/<id>.tiles.toml`;
//! docs/format/v0.md). The sim never sees any of it.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::{Anchor, SpriteImageMode};
use iwbtg_format::files::PackageFiles;
use iwbtg_sim::def::RoomId;

use crate::package::decode_png;
use crate::view::{Game, RoomSize};

/// Tile size when the tileset has no sidecar: the IWBTG standard.
const DEFAULT_TILE: (u32, u32) = (32, 32);

/// What a room draws besides its instances.
#[derive(Debug, Default, PartialEq)]
pub struct RoomArt {
    /// Fill colour of the room.
    pub color: Option<[f32; 3]>,
    /// Background image id (`backgrounds/<id>.png`), repeated over the room.
    pub background: Option<String>,
    /// Tile layers.
    pub layers: Vec<Layer>,
}

#[derive(Debug, PartialEq)]
pub struct Layer {
    pub depth: f64,
    pub tileset: String,
    pub tiles: Vec<Tile>,
}

/// One tile; `run` repeats it over a grid of the tile size.
#[derive(Debug, PartialEq)]
pub struct Tile {
    pub pos: (f64, f64),
    pub src: (f64, f64),
    pub run: (u32, u32),
}

/// A TOML number, or the format's `{ bits = "<16 hex digits>" }` double.
fn number(v: &toml::Value) -> Option<f64> {
    match v {
        toml::Value::Float(f) => Some(*f),
        toml::Value::Integer(i) => Some(*i as f64),
        toml::Value::Table(t) => {
            let bits = u64::from_str_radix(t.get("bits")?.as_str()?, 16).ok()?;
            Some(f64::from_bits(bits))
        }
        _ => None,
    }
}

fn pair(v: &toml::Value) -> Option<(f64, f64)> {
    let a = v.as_array()?;
    Some((number(a.first()?)?, number(a.get(1)?)?))
}

/// `#rrggbb` as sRGB components.
fn hex_color(s: &str) -> Option<[f32; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let c = |i: usize| {
        u8::from_str_radix(&s[i..i + 2], 16)
            .ok()
            .map(|v| f32::from(v) / 255.0)
    };
    Some([c(0)?, c(2)?, c(4)?])
}

/// Reads the art keys of a room file and its optional tiles file. The loader already checked the
/// files, so anything unreadable here is skipped.
pub fn parse_room_art(room: &str, tiles: Option<&str>) -> RoomArt {
    let mut art = RoomArt::default();
    if let Ok(t) = room.parse::<toml::Table>() {
        art.color = t
            .get("background_color")
            .and_then(|v| v.as_str())
            .and_then(hex_color);
        art.background = t.get("background").and_then(|v| v.as_str()).map(String::from);
    }
    let Some(t) = tiles.and_then(|s| s.parse::<toml::Table>().ok()) else {
        return art;
    };
    for layer in t.get("layers").and_then(|v| v.as_array()).into_iter().flatten() {
        let (Some(depth), Some(tileset)) = (
            layer.get("depth").and_then(number),
            layer.get("tileset").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        let tiles = layer
            .get("tiles")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|t| {
                let run = t.get("run").and_then(pair).unwrap_or((1.0, 1.0));
                Some(Tile {
                    pos: (number(t.get("x")?)?, number(t.get("y")?)?),
                    src: pair(t.get("src")?)?,
                    run: (run.0 as u32, run.1 as u32),
                })
            })
            .collect();
        art.layers.push(Layer {
            depth,
            tileset: tileset.to_string(),
            tiles,
        });
    }
    art
}

/// The size of one tile of a tileset: `tile_size = [w, h]` in `backgrounds/<id>.toml`.
fn tile_size(files: &PackageFiles, tileset: &str) -> (u32, u32) {
    files
        .get(&format!("backgrounds/{tileset}.toml"))
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.parse::<toml::Table>().ok())
        .and_then(|t| t.get("tile_size").and_then(pair))
        .filter(|(w, h)| *w >= 1.0 && *h >= 1.0)
        .map_or(DEFAULT_TILE, |(w, h)| (w as u32, h as u32))
}

/// Where tiles sit in depth: GM draws a deeper layer first. Clamped inside the camera's range;
/// the background is behind every layer.
fn layer_z(depth: f64) -> f32 {
    (-depth / 1000.0).clamp(-990.0, 990.0) as f32
}

const BACKGROUND_Z: f32 = -995.0;
const COLOR_Z: f32 = -996.0;

#[derive(Resource)]
pub struct ArtFiles(pub PackageFiles);

/// Decoded background and tileset images, by id.
#[derive(Resource, Default)]
pub struct ArtImages(HashMap<String, Option<(Handle<Image>, UVec2)>>);

/// The room the art on screen belongs to, and its entities.
#[derive(Resource, Default)]
pub struct ShownArt {
    room: Option<RoomId>,
    entities: Vec<Entity>,
}

fn image(
    cache: &mut ArtImages,
    files: &PackageFiles,
    images: &mut Assets<Image>,
    id: &str,
) -> Option<Handle<Image>> {
    cache
        .0
        .entry(id.to_string())
        .or_insert_with(|| {
            let s = files.get(&format!("backgrounds/{id}.png")).and_then(decode_png)?;
            let image = Image::new(
                Extent3d {
                    width: s.size.0,
                    height: s.size.1,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                s.rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            );
            Some((images.add(image), UVec2::new(s.size.0, s.size.1)))
        })
        .as_ref()
        .map(|(h, _)| h.clone())
}

/// Rebuilds the art when the sim's room changes.
pub fn sync_room_art(
    mut commands: Commands,
    game: NonSend<Game>,
    files: Res<ArtFiles>,
    room_size: Res<RoomSize>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<ArtImages>,
    mut shown: ResMut<ShownArt>,
) {
    let room = game.sim.world().room;
    if shown.room == Some(room) {
        return;
    }
    shown.room = Some(room);
    for e in shown.entities.drain(..) {
        commands.entity(e).despawn();
    }
    let Some(def) = game.sim.def().room(room) else {
        return;
    };
    let text = |path: String| {
        files
            .0
            .get(&path)
            .and_then(|b| std::str::from_utf8(b).ok().map(String::from))
    };
    let art = parse_room_art(
        &text(format!("rooms/{}.toml", def.name)).unwrap_or_default(),
        text(format!("rooms/{}.tiles.toml", def.name)).as_deref(),
    );
    let (w, h) = (room_size.0.x, room_size.0.y);
    // GM: y grows downward from the room corner. Bevy: y up from the centre.
    let corner = |x: f32, y: f32, z: f32| Transform::from_xyz(x - w / 2.0, h / 2.0 - y, z);
    let mut spawned = Vec::new();
    if let Some([r, g, b]) = art.color {
        spawned.push(
            commands
                .spawn((
                    Sprite::from_color(Color::srgb(r, g, b), Vec2::new(w, h)),
                    Transform::from_xyz(0.0, 0.0, COLOR_Z),
                ))
                .id(),
        );
    }
    if let Some(handle) = art
        .background
        .as_deref()
        .and_then(|id| image(&mut cache, &files.0, &mut images, id))
    {
        let mut sprite = Sprite::from_image(handle);
        sprite.custom_size = Some(Vec2::new(w, h));
        sprite.image_mode = SpriteImageMode::Tiled {
            tile_x: true,
            tile_y: true,
            stretch_value: 1.0,
        };
        spawned.push(
            commands
                .spawn((sprite, Anchor::TOP_LEFT, corner(0.0, 0.0, BACKGROUND_Z)))
                .id(),
        );
    }
    for (n, layer) in art.layers.iter().enumerate() {
        let Some(handle) = image(&mut cache, &files.0, &mut images, &layer.tileset) else {
            continue;
        };
        let (tw, th) = tile_size(&files.0, &layer.tileset);
        // Later layers of the same depth draw on top.
        let z = layer_z(layer.depth) + n as f32 * 1e-4;
        for tile in &layer.tiles {
            for row in 0..tile.run.1 {
                for col in 0..tile.run.0 {
                    let x = tile.pos.0 as f32 + (col * tw) as f32;
                    let y = tile.pos.1 as f32 + (row * th) as f32;
                    let mut sprite = Sprite::from_image(handle.clone());
                    sprite.rect = Some(Rect::new(
                        tile.src.0 as f32,
                        tile.src.1 as f32,
                        tile.src.0 as f32 + tw as f32,
                        tile.src.1 as f32 + th as f32,
                    ));
                    spawned.push(commands.spawn((sprite, Anchor::TOP_LEFT, corner(x, y, z))).id());
                }
            }
        }
    }
    shown.entities = spawned;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_art_keys_are_read() {
        let art = parse_room_art(
            "size = [800, 608]\nbackground = \"sky\"\nbackground_color = \"#ff8000\"\n",
            Some(
                "layers = [\n  { depth = 1000000, tileset = \"t\", tiles = [\n    { x = 0, y = 576, src = [32, 0], run = [25, 1] },\n    { x = 8, y = 8, src = [0, 0] },\n  ] },\n]\n",
            ),
        );
        assert_eq!(art.background.as_deref(), Some("sky"));
        assert_eq!(art.color, Some([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(art.layers.len(), 1);
        assert_eq!(art.layers[0].depth, 1_000_000.0);
        assert_eq!(art.layers[0].tiles[0].run, (25, 1));
        assert_eq!(art.layers[0].tiles[0].src, (32.0, 0.0));
        assert_eq!(art.layers[0].tiles[1].run, (1, 1));
    }

    #[test]
    fn a_room_without_art_draws_nothing_extra() {
        assert_eq!(parse_room_art("size = [32, 32]\n", None), RoomArt::default());
    }

    #[test]
    fn deeper_layers_are_farther_back_and_never_past_the_background() {
        assert!(layer_z(1_000_000.0) < layer_z(0.0));
        assert!(layer_z(-50.0) > layer_z(0.0));
        assert!(layer_z(1e12) > BACKGROUND_Z);
    }
}
