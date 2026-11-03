//! The window: a camera on the room and one Bevy sprite per visible sim instance.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::sim::{Sim, SimOptions};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::InstId;

use crate::package::{Package, SpriteImage, sprite_images};

/// The running game. Not `Send` (the script host shares state by `Rc`), so it is a non-send
/// resource and only main-thread systems touch it.
struct Game {
    sim: Sim,
}

/// The Bevy image of each sprite strip, by `SpriteId`, with the strip size.
#[derive(Resource)]
struct Textures(Vec<Option<(Handle<Image>, UVec2)>>);

/// Which Bevy entity draws which instance.
#[derive(Resource, Default)]
struct Drawn(BTreeMap<InstId, Entity>);

/// The room size in pixels.
#[derive(Resource)]
struct RoomSize(Vec2);

/// Decoded sprite strips, waiting for the Startup system to upload them.
#[derive(Resource)]
struct PendingImages(Vec<Option<SpriteImage>>);

/// Opens the window and runs until it closes.
pub fn run(pkg: Package, screenshot: Option<std::path::PathBuf>) -> Result<(), String> {
    let def = Arc::clone(&pkg.def);
    let options = SimOptions {
        // Like a replay: a new game, in the game's `new_game_room`. The title and menus come with
        // the rest of the client; until then the first thing on screen is the first room.
        start: StartPoint::NewGame { difficulty: 0 },
        seed: 0,
        dll_mode: DllMode::Model,
    };
    let sim = Sim::new(Arc::clone(&def), options).map_err(|e| format!("the game did not start: {e}"))?;
    let room = def.room(sim.world().room).ok_or("the start room is missing")?;
    let (width, height) = (room.width as f32, room.height as f32);
    let images = sprite_images(&pkg.files);
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("IWBTG Collection: {}", def.id),
                    resolution: (width as u32, height as u32).into(),
                    ..default()
                }),
                ..default()
            }),
    )
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(RoomSize(Vec2::new(width, height)))
    .insert_resource(PendingImages(images))
    .init_resource::<Drawn>()
    .insert_non_send(Game { sim })
    .add_systems(Startup, setup)
    .add_systems(Update, sync_instances);
    if let Some(path) = screenshot {
        app.insert_resource(ScreenshotTo(path))
            .add_systems(Update, take_screenshot);
    }
    app.run();
    Ok(())
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    pending: Res<PendingImages>,
    room: Res<RoomSize>,
) {
    // The room always fits the window, whatever its size: letterboxed, never cropped.
    commands.spawn((
        Camera2d,
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: room.0.x,
                min_height: room.0.y,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
    let textures = pending
        .0
        .iter()
        .map(|s| {
            s.as_ref().map(|s| {
                let image = Image::new(
                    Extent3d {
                        width: s.size.0,
                        height: s.size.1,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    s.rgba.clone(),
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::RENDER_WORLD,
                );
                (images.add(image), UVec2::new(s.size.0, s.size.1))
            })
        })
        .collect();
    commands.insert_resource(Textures(textures));
}

/// Makes the entities match the sim's instances: spawn, move, frame, remove.
fn sync_instances(
    mut commands: Commands,
    game: NonSend<Game>,
    textures: Option<Res<Textures>>,
    room: Res<RoomSize>,
    mut drawn: ResMut<Drawn>,
    mut query: Query<(&mut Sprite, &mut Transform)>,
) {
    let Some(textures) = textures else { return };
    let def = game.sim.def();
    let mut alive = BTreeMap::new();
    for inst in game.sim.world().instances() {
        if !inst.alive || !inst.visible {
            continue;
        }
        let Some(sprite_id) = inst.img.sprite() else {
            continue;
        };
        let Some(sprite_def) = def.sprites.get(sprite_id.0 as usize) else {
            continue;
        };
        let Some(Some((handle, strip))) = textures.0.get(sprite_id.0 as usize) else {
            continue;
        };
        let frames = f64::from(sprite_def.frames.max(1));
        let frame = inst.img.index().get().floor().rem_euclid(frames) as u32;
        let (w, h) = (sprite_def.size.0 as f32, sprite_def.size.1 as f32);
        let (sx, sy) = (inst.img.xscale().get() as f32, inst.img.yscale().get() as f32);
        let angle = (inst.img.angle().get() as f32).to_radians();
        // The sprite's centre relative to its origin, scaled and turned like the sprite.
        let off = Vec2::new(
            (w / 2.0 - sprite_def.origin.0 as f32) * sx,
            (h / 2.0 - sprite_def.origin.1 as f32) * sy,
        );
        let (sin, cos) = angle.sin_cos();
        let off = Vec2::new(off.x * cos + off.y * sin, -off.x * sin + off.y * cos);
        // GM: y grows downward from the room corner. Bevy: y up from the centre.
        let x = inst.m.x().get() as f32 + off.x - room.0.x / 2.0;
        let y = room.0.y / 2.0 - (inst.m.y().get() as f32 + off.y);
        let z = -(inst.depth.get() as f32) / 1000.0 + (inst.id.0 % 1000) as f32 * 1e-6;
        let rect = Rect::new(frame as f32 * w, 0.0, (frame + 1) as f32 * w, strip.y as f32);
        let transform = Transform::from_xyz(x, y, z)
            .with_rotation(Quat::from_rotation_z(angle))
            .with_scale(Vec3::new(sx, sy, 1.0));
        let color = Color::srgba(1.0, 1.0, 1.0, inst.img.alpha().get() as f32);
        let entity = match drawn.0.get(&inst.id).copied() {
            Some(entity) => {
                if let Ok((mut sprite, mut t)) = query.get_mut(entity) {
                    sprite.image = handle.clone();
                    sprite.rect = Some(rect);
                    sprite.color = color;
                    *t = transform;
                }
                entity
            }
            None => commands
                .spawn((
                    Sprite {
                        image: handle.clone(),
                        rect: Some(rect),
                        color,
                        ..default()
                    },
                    transform,
                ))
                .id(),
        };
        alive.insert(inst.id, entity);
    }
    for (id, entity) in &drawn.0 {
        if !alive.contains_key(id) {
            commands.entity(*entity).despawn();
        }
    }
    drawn.0 = alive;
}

/// Where `--screenshot` writes.
#[derive(Resource)]
struct ScreenshotTo(std::path::PathBuf);

/// Frame 30 asks for a screenshot; frame 90 exits (the file is written by then).
fn take_screenshot(
    mut commands: Commands,
    to: Res<ScreenshotTo>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;
    if *frame == 30 {
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(to.0.clone()));
    }
    if *frame == 90 {
        exit.write(AppExit::Success);
    }
}
