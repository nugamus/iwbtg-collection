//! Pictures of rooms for the results' heatmap (#654): a room as it starts (colour, background
//! layers, tiles and the instances placed in its file, each at its first frame), drawn once by a
//! camera of its own into an image the page shows. Instances a room creates while it runs are
//! not in the picture. Nothing here touches the sim; it only reads the game's definition.

use std::collections::BTreeMap;

use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::sprite::Anchor;
use iwbtg_format::files::PackageFiles;
use iwbtg_sim::def::GameDef;

use crate::art::{self, ArtImages};
use crate::view::{Textures, place_sprite};

/// The first render layer the pictures use; each picture has its own, so no camera sees another
/// picture's sprites (the game and the UI draw on layer 0 or none).
const FIRST_LAYER: usize = 16;

/// What a picture is drawn from.
pub struct Sources<'a> {
    pub def: &'a GameDef,
    pub files: &'a PackageFiles,
    pub textures: Option<&'a Textures>,
}

/// The pictures drawn for the page that shows, by package room id.
#[derive(Resource, Default)]
pub struct Pictures {
    rooms: BTreeMap<String, Picture>,
}

struct Picture {
    image: Handle<Image>,
    size: Vec2,
    /// The camera and the sprites.
    entities: Vec<Entity>,
}

impl Pictures {
    /// The picture of room `name` and the room's size in pixels, drawn the first time it is
    /// asked for. `None` when the game has no such room.
    pub fn get_or_build(
        &mut self,
        commands: &mut Commands,
        src: &Sources,
        images: &mut Assets<Image>,
        cache: &mut ArtImages,
        name: &str,
    ) -> Option<(Handle<Image>, Vec2)> {
        if let Some(p) = self.rooms.get(name) {
            return Some((p.image.clone(), p.size));
        }
        let room = src.def.rooms.iter().find(|r| r.name == name)?;
        let size = Vec2::new(room.width.max(1) as f32, room.height.max(1) as f32);
        let mut target =
            Image::new_target_texture(size.x as u32, size.y as u32, TextureFormat::Rgba8UnormSrgb, None);
        // Shown at about a quarter of its size: smooth, not nearest.
        target.sampler = ImageSampler::linear();
        let image = images.add(target);
        let layer = RenderLayers::layer(FIRST_LAYER + self.rooms.len());
        let mut entities = vec![
            commands
                .spawn((
                    Camera2d,
                    Camera {
                        // Before the page's camera (order 20), which shows the image.
                        order: -50,
                        clear_color: ClearColorConfig::Custom(Color::BLACK),
                        ..default()
                    },
                    RenderTarget::Image(image.clone().into()),
                    layer.clone(),
                ))
                .id(),
        ];
        for (sprite, t) in art::still(src.files, images, cache, name, size) {
            entities.push(commands.spawn((sprite, Anchor::TOP_LEFT, t, layer.clone())).id());
        }
        for (i, placed) in room.instances.iter().enumerate() {
            let Some(kind) = src.def.kind(placed.kind) else {
                continue;
            };
            if !placed.visible.unwrap_or(kind.visible) {
                continue;
            }
            let Some(id) = kind.sprite else {
                continue;
            };
            let (Some(def), Some(Some((handle, strip)))) = (
                src.def.sprite(id),
                src.textures.and_then(|t| t.0.get(id.0 as usize)),
            ) else {
                continue;
            };
            let depth = placed.depth.unwrap_or(kind.depth).get() as f32;
            let (rect, t) = place_sprite(
                def,
                0,
                strip.as_vec2(),
                Vec2::new(placed.x.get() as f32, placed.y.get() as f32),
                Vec2::new(placed.xscale.get() as f32, placed.yscale.get() as f32),
                placed.angle.get() as f32,
                -depth / 1000.0 + i as f32 * 1e-6,
                size,
            );
            let sprite = Sprite {
                image: handle.clone(),
                rect: Some(rect),
                ..default()
            };
            entities.push(commands.spawn((sprite, t, layer.clone())).id());
        }
        self.rooms.insert(
            name.to_owned(),
            Picture {
                image: image.clone(),
                size,
                entities,
            },
        );
        Some((image, size))
    }

    /// Drops every picture (the page closed).
    pub fn clear(&mut self, commands: &mut Commands) {
        for (_, p) in std::mem::take(&mut self.rooms) {
            for e in p.entities {
                commands.entity(e).despawn();
            }
        }
    }
}
