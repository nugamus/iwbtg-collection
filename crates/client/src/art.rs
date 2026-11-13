//! Room art that is not an instance: the background colour, the background layers and the
//! decorative tile layers. `iwbtg_format::art` reads them (`rooms/<id>.toml`,
//! `rooms/<id>.tiles.toml`, `backgrounds/<id>.toml`; docs/format/v0.md); this module draws them.
//! The sim never sees any of it.
//!
//! Drawing order (research CLONE_SPEC, Rooms: drawing order): the colour, then the background
//! layers that are not foregrounds in list order, then instances and tiles by depth, then the
//! foreground layers in list order.
//!
//! No background (`--mods NB`, visual.rs) puts [`PlainBackdrop`]'s colour in place of the
//! room's colour and leaves out the background layers that are not foregrounds.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::{Anchor, SpriteImageMode};
use iwbtg_format::art::{Background, RoomArt, TileLayer, load_room_art, load_tiles};
use iwbtg_format::files::PackageFiles;
use iwbtg_sim::def::RoomId;

use crate::package::decode_png;
use crate::view::{Game, RoomSize};
use crate::visual::PlainBackdrop;

/// Where tiles sit in depth: GM draws a deeper layer first. Clamped inside the camera's range;
/// the backgrounds are behind every layer and the foregrounds in front of all of them.
fn layer_z(depth: f64) -> f32 {
    (-depth / 1000.0).clamp(-990.0, 990.0) as f32
}

const COLOR_Z: f32 = -996.0;
const BACKGROUND_Z: f32 = -995.0;
/// In front of every instance and tile, behind the overlay (z 1000).
const FOREGROUND_Z: f32 = 995.0;

#[derive(Resource)]
pub struct ArtFiles(pub PackageFiles);

/// Room backgrounds and tile layers for one package.
pub struct ArtPlugin(pub PackageFiles);

impl Plugin for ArtPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ArtFiles(self.0.clone()))
            .init_resource::<ArtImages>()
            .init_resource::<ShownArt>()
            .add_systems(
                Update,
                (sync_room_art, scroll_backgrounds)
                    .chain()
                    .after(crate::view::follow_view),
            );
    }
}

/// Decoded background and tileset images, by id, with their size in pixels.
#[derive(Resource, Default)]
pub struct ArtImages(HashMap<String, Option<(Handle<Image>, Vec2)>>);

/// The room the art on screen belongs to, and its entities.
#[derive(Resource, Default)]
pub struct ShownArt {
    room: Option<RoomId>,
    /// The sim's tick when the room's art was built: scrolling counts from here.
    start_tick: u64,
    /// The tick the backgrounds were last placed for.
    placed_tick: u64,
    art: RoomArt,
    /// Colour and tiles.
    entities: Vec<Entity>,
    /// The sprites of each background layer, in `art.backgrounds` order.
    backgrounds: Vec<Vec<Entity>>,
}

fn image(
    cache: &mut ArtImages,
    files: &PackageFiles,
    images: &mut Assets<Image>,
    id: &str,
) -> Option<(Handle<Image>, Vec2)> {
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
            Some((images.add(image), Vec2::new(s.size.0 as f32, s.size.1 as f32)))
        })
        .clone()
}

/// Whether layer `bg` is drawn: No background leaves out every layer behind the room.
fn shows(bg: &Background, plain: Option<&PlainBackdrop>) -> bool {
    plain.is_none() || bg.foreground
}

/// One piece of a background along one axis: room pixels `at .. at + len` show image pixels
/// `from .. to`, repeated when `repeat`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Segment {
    at: f32,
    len: f32,
    from: f32,
    to: f32,
    repeat: bool,
}

/// Cuts one axis of a background into the pieces that cover exactly the room (`0 .. room`), so
/// nothing is drawn beside it. `image` is the image's length, `cell` its length in the room
/// (the room's length when stretched), `pos` where its first copy starts.
fn axis(room: f32, image: f32, cell: f32, pos: f32, tile: bool) -> Vec<Segment> {
    let scale = cell / image;
    if !tile {
        let (start, end) = (pos.max(0.0), (pos + cell).min(room));
        if end <= start {
            return Vec::new();
        }
        return vec![Segment {
            at: start,
            len: end - start,
            from: (start - pos) / scale,
            to: (end - pos) / scale,
            repeat: false,
        }];
    }
    // The copy that covers the room's edge starts `phase` pixels before it.
    let phase = pos.rem_euclid(cell);
    let mut out = Vec::new();
    if phase > 0.0 {
        let len = phase.min(room);
        let from = image - phase / scale;
        out.push(Segment {
            at: 0.0,
            len,
            from,
            to: from + len / scale,
            repeat: false,
        });
    }
    if room > phase {
        out.push(Segment {
            at: phase,
            len: room - phase,
            from: 0.0,
            to: image,
            repeat: true,
        });
    }
    out
}

/// The sprites of one background layer `ticks` ticks after the room started, as
/// `(sprite, top-left in the room, scale)`.
fn background_sprites(
    bg: &Background,
    handle: &Handle<Image>,
    image: Vec2,
    room: Vec2,
    ticks: u64,
) -> Vec<(Sprite, Vec2, Vec2)> {
    let (px, py) = bg.position(ticks);
    let cell = if bg.stretch { room } else { image };
    let scale = cell / image;
    let xs = axis(room.x, image.x, cell.x, px as f32, bg.tile.0);
    let ys = axis(room.y, image.y, cell.y, py as f32, bg.tile.1);
    let mut out = Vec::new();
    for x in &xs {
        for y in &ys {
            let mut sprite = Sprite::from_image(handle.clone());
            sprite.rect = Some(Rect::new(x.from, y.from, x.to, y.to));
            sprite.custom_size = Some(Vec2::new(x.len / scale.x, y.len / scale.y));
            if x.repeat || y.repeat {
                sprite.image_mode = SpriteImageMode::Tiled {
                    tile_x: x.repeat,
                    tile_y: y.repeat,
                    stretch_value: 1.0,
                };
            }
            out.push((sprite, Vec2::new(x.at, y.at), scale));
        }
    }
    out
}

/// GM: y grows downward from the room corner. Bevy: y up from the centre.
fn corner(room: Vec2, at: Vec2, z: f32) -> Transform {
    Transform::from_xyz(at.x - room.x / 2.0, room.y / 2.0 - at.y, z)
}

fn background_z(bg: &Background, n: usize) -> f32 {
    let base = if bg.foreground { FOREGROUND_Z } else { BACKGROUND_Z };
    // Later layers draw on top.
    base + n as f32 * 1e-3
}

fn spawn_background(
    commands: &mut Commands,
    bg: &Background,
    n: usize,
    img: Option<&(Handle<Image>, Vec2)>,
    room: Vec2,
    ticks: u64,
) -> Vec<Entity> {
    let Some((handle, size)) = img else {
        return Vec::new();
    };
    background_sprites(bg, handle, *size, room, ticks)
        .into_iter()
        .map(|(sprite, at, scale)| {
            let t = corner(room, at, background_z(bg, n)).with_scale(scale.extend(1.0));
            commands.spawn((sprite, Anchor::TOP_LEFT, t)).id()
        })
        .collect()
}

/// Rebuilds the art when the sim's room changes.
#[allow(
    clippy::too_many_arguments,
    reason = "a Bevy system: each is one resource it reads"
)]
pub fn sync_room_art(
    mut commands: Commands,
    game: NonSend<Game>,
    files: Res<ArtFiles>,
    room_size: Res<RoomSize>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<ArtImages>,
    mut shown: ResMut<ShownArt>,
    plain: Option<Res<PlainBackdrop>>,
) {
    let plain = plain.as_deref();
    let room = game.sim.world().room;
    let tick = game.sim.world().tick;
    // A retry starts a new sim in the same room with the tick back at its start.
    if shown.room == Some(room) && tick >= shown.start_tick {
        return;
    }
    shown.room = Some(room);
    shown.start_tick = tick;
    shown.placed_tick = tick;
    let mut old = std::mem::take(&mut shown.entities);
    old.extend(shown.backgrounds.drain(..).flatten());
    for e in old {
        commands.entity(e).despawn();
    }
    shown.art = RoomArt::default();
    let Some(def) = game.sim.def().room(room) else {
        return;
    };
    // The loader checked both files; one that fails here draws nothing.
    let art = load_room_art(&files.0, &def.name).unwrap_or_default();
    let tiles = load_tiles(&files.0, &def.name).unwrap_or_default();
    let size = room_size.0;
    let mut spawned = Vec::new();
    let color = match plain {
        Some(p) => Some(p.0),
        None => art.color.map(|[r, g, b]| Color::srgb_u8(r, g, b)),
    };
    if let Some(color) = color {
        spawned.push(
            commands
                .spawn((
                    Sprite::from_color(color, size),
                    Transform::from_xyz(0.0, 0.0, COLOR_Z),
                ))
                .id(),
        );
    }
    shown.backgrounds = art
        .backgrounds
        .iter()
        .enumerate()
        .map(|(n, bg)| {
            if !shows(bg, plain) {
                return Vec::new();
            }
            let img = image(&mut cache, &files.0, &mut images, &bg.image);
            spawn_background(&mut commands, bg, n, img.as_ref(), size, 0)
        })
        .collect();
    spawn_tiles(
        &mut commands,
        &tiles,
        size,
        &files.0,
        &mut images,
        &mut cache,
        &mut spawned,
    );
    shown.entities = spawned;
    shown.art = art;
}

fn spawn_tiles(
    commands: &mut Commands,
    layers: &[TileLayer],
    room: Vec2,
    files: &PackageFiles,
    images: &mut Assets<Image>,
    cache: &mut ArtImages,
    spawned: &mut Vec<Entity>,
) {
    for (sprite, t) in tile_sprites(layers, room, files, images, cache) {
        spawned.push(commands.spawn((sprite, Anchor::TOP_LEFT, t)).id());
    }
}

/// Each tile as a sprite anchored at its top-left corner.
fn tile_sprites(
    layers: &[TileLayer],
    room: Vec2,
    files: &PackageFiles,
    images: &mut Assets<Image>,
    cache: &mut ArtImages,
) -> Vec<(Sprite, Transform)> {
    let mut out = Vec::new();
    for (n, layer) in layers.iter().enumerate() {
        let Some((handle, _)) = image(cache, files, images, &layer.tileset) else {
            continue;
        };
        // Later layers of the same depth draw on top.
        let z = layer_z(layer.depth) + n as f32 * 1e-4;
        for tile in &layer.tiles {
            let (sx, sy) = (tile.src.0 as f32, tile.src.1 as f32);
            let (w, h) = (tile.size.0 as f32, tile.size.1 as f32);
            for (x, y) in tile.positions() {
                let mut sprite = Sprite::from_image(handle.clone());
                sprite.rect = Some(Rect::new(sx, sy, sx + w, sy + h));
                let at = Vec2::new(x as f32, y as f32);
                out.push((sprite, corner(room, at, z)));
            }
        }
    }
    out
}

/// A still of room `name`'s art as it starts (the colour, the background layers at tick 0, the
/// tiles), for a picture of the room off screen (the results' heatmap, #654). Every sprite is
/// anchored at its top-left corner, in the same room-centred space the game draws in.
pub(crate) fn still(
    files: &PackageFiles,
    images: &mut Assets<Image>,
    cache: &mut ArtImages,
    name: &str,
    room: Vec2,
) -> Vec<(Sprite, Transform)> {
    let art = load_room_art(files, name).unwrap_or_default();
    let tiles = load_tiles(files, name).unwrap_or_default();
    let mut out = Vec::new();
    if let Some([r, g, b]) = art.color {
        let t = corner(room, Vec2::ZERO, COLOR_Z);
        out.push((Sprite::from_color(Color::srgb_u8(r, g, b), room), t));
    }
    for (n, bg) in art.backgrounds.iter().enumerate() {
        let Some((handle, size)) = image(cache, files, images, &bg.image) else {
            continue;
        };
        for (sprite, at, scale) in background_sprites(bg, &handle, size, room, 0) {
            let t = corner(room, at, background_z(bg, n)).with_scale(scale.extend(1.0));
            out.push((sprite, t));
        }
    }
    out.extend(tile_sprites(&tiles, room, files, images, cache));
    out
}

/// Moves the scrolling background layers once per tick: each one is rebuilt where it is now.
#[allow(
    clippy::too_many_arguments,
    reason = "a Bevy system: each is one resource it reads"
)]
pub fn scroll_backgrounds(
    mut commands: Commands,
    game: NonSend<Game>,
    files: Res<ArtFiles>,
    room_size: Res<RoomSize>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<ArtImages>,
    mut shown: ResMut<ShownArt>,
    plain: Option<Res<PlainBackdrop>>,
) {
    let plain = plain.as_deref();
    let tick = game.sim.world().tick;
    if tick == shown.placed_tick || shown.room.is_none() {
        return;
    }
    shown.placed_tick = tick;
    let ticks = tick.saturating_sub(shown.start_tick);
    let shown = &mut *shown;
    for (n, bg) in shown.art.backgrounds.iter().enumerate() {
        if bg.speed == (0.0, 0.0) || !shows(bg, plain) {
            continue;
        }
        for e in shown.backgrounds[n].drain(..) {
            commands.entity(e).despawn();
        }
        let img = image(&mut cache, &files.0, &mut images, &bg.image);
        shown.backgrounds[n] = spawn_background(&mut commands, bg, n, img.as_ref(), room_size.0, ticks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_background_keeps_only_the_foreground_layers() {
        let behind = Background::new("sky", 0..3);
        let front = Background {
            foreground: true,
            ..Background::new("stone", 0..5)
        };
        let plain = PlainBackdrop(crate::visual::PLAIN);
        assert!(shows(&behind, None) && shows(&front, None));
        assert!(!shows(&behind, Some(&plain)));
        assert!(shows(&front, Some(&plain)));
    }

    #[test]
    fn deeper_layers_are_farther_back_and_never_past_the_backgrounds() {
        assert!(layer_z(1_000_000.0) < layer_z(0.0));
        assert!(layer_z(-50.0) > layer_z(0.0));
        assert!(layer_z(1e12) > BACKGROUND_Z + 8e-3);
        assert!(layer_z(-1e12) < FOREGROUND_Z);
    }

    #[test]
    fn a_tiled_axis_covers_the_room_exactly() {
        // Image 64 px in a 100 px room, starting at 0: one repeated piece.
        assert_eq!(
            axis(100.0, 64.0, 64.0, 0.0, true),
            [Segment {
                at: 0.0,
                len: 100.0,
                from: 0.0,
                to: 64.0,
                repeat: true
            }]
        );
        // Scrolled 10 px right: the last 10 px of the image first, then the repeat from 10.
        assert_eq!(
            axis(100.0, 64.0, 64.0, 10.0, true),
            [
                Segment {
                    at: 0.0,
                    len: 10.0,
                    from: 54.0,
                    to: 64.0,
                    repeat: false
                },
                Segment {
                    at: 10.0,
                    len: 90.0,
                    from: 0.0,
                    to: 64.0,
                    repeat: true
                },
            ]
        );
        // Scrolled 10 px left is the same as 54 px right.
        assert_eq!(axis(100.0, 64.0, 64.0, -10.0, true)[0].from, 10.0);
    }

    #[test]
    fn an_untiled_axis_is_one_copy_cut_at_the_room_edge() {
        let s = axis(100.0, 64.0, 64.0, 60.0, false);
        assert_eq!(
            s,
            [Segment {
                at: 60.0,
                len: 40.0,
                from: 0.0,
                to: 40.0,
                repeat: false
            }]
        );
        assert!(axis(100.0, 64.0, 64.0, 100.0, false).is_empty());
        assert!(axis(100.0, 64.0, 64.0, -64.0, false).is_empty());
        // Stretched to the room: the whole image over the whole room.
        let s = axis(100.0, 50.0, 100.0, 0.0, false);
        assert_eq!((s[0].len, s[0].from, s[0].to), (100.0, 0.0, 50.0));
    }

    #[test]
    fn a_stretched_background_scales_its_sprite() {
        let mut bg = Background::new("sky", 0..0);
        bg.stretch = true;
        bg.tile = (false, false);
        let sprites = background_sprites(
            &bg,
            &Handle::default(),
            Vec2::new(50.0, 40.0),
            Vec2::new(800.0, 608.0),
            0,
        );
        assert_eq!(sprites.len(), 1);
        assert_eq!(sprites[0].2, Vec2::new(16.0, 15.2));
        assert_eq!(sprites[0].0.custom_size, Some(Vec2::new(50.0, 40.0)));
    }
}
