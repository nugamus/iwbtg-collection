//! The window: a camera on the room and one Bevy sprite per visible sim instance.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::asset::RenderAssetUsages;
use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::sim::{Sim, SimOptions};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{InstId, SimStatus};

use crate::input::{Collector, key_code};
use crate::package::{Package, SpriteImage, sprite_images};

/// The running game. Not `Send` (the script host shares state by `Rc`), so it is a non-send
/// resource and only main-thread systems touch it.
pub(crate) struct Game {
    pub(crate) sim: Sim,
}

/// The Bevy image of each sprite strip, by `SpriteId`, with the strip size.
#[derive(Resource)]
struct Textures(Vec<Option<(Handle<Image>, UVec2)>>);

/// Which Bevy entity draws which instance.
#[derive(Resource, Default)]
struct Drawn(BTreeMap<InstId, Entity>);

/// The room size in pixels.
#[derive(Resource)]
pub(crate) struct RoomSize(pub(crate) Vec2);

/// Decoded sprite strips, waiting for the Startup system to upload them.
#[derive(Resource)]
struct PendingImages(Vec<Option<SpriteImage>>);

/// How many frames per second the window draws at most. The sim ticks at the game's rate
/// whatever this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameCap {
    /// At most this many frames per second.
    Fps(u32),
    /// No limit.
    Unlimited,
}

impl std::str::FromStr for FrameCap {
    type Err = String;

    fn from_str(s: &str) -> Result<FrameCap, String> {
        if s.eq_ignore_ascii_case("unlimited") {
            return Ok(FrameCap::Unlimited);
        }
        match s.parse::<u32>() {
            Ok(n) if n >= 1 => Ok(FrameCap::Fps(n)),
            _ => Err(format!(
                "`{s}`: expected a number of frames per second (1 or more) or `unlimited`"
            )),
        }
    }
}

/// The frame cap in force.
#[derive(Resource)]
struct Cap(FrameCap);

/// Ticks and frames counted over the last full second of wall time, for the overlay.
#[derive(Resource)]
struct Stats {
    window_start: Instant,
    ticks: u32,
    frames: u32,
    ticks_per_second: f64,
    frames_per_second: f64,
}

/// The overlay text entity.
#[derive(Component)]
struct Overlay;

/// Opens the window and runs until it closes.
pub fn run(pkg: Package, cap: FrameCap, screenshot: Option<std::path::PathBuf>) -> Result<(), String> {
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
                    // The frame cap does the pacing; vsync would pin it to the monitor.
                    present_mode: bevy::window::PresentMode::AutoNoVsync,
                    ..default()
                }),
                ..default()
            }),
    )
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(RoomSize(Vec2::new(width, height)))
    .insert_resource(PendingImages(images))
    .insert_resource(crate::art::ArtFiles(pkg.files.clone()))
    .init_resource::<crate::art::ArtImages>()
    .init_resource::<crate::art::ShownArt>()
    .init_resource::<Drawn>()
    .insert_non_send(Game { sim })
    .insert_resource(Collector::new())
    .insert_resource(Cap(cap))
    .insert_resource(Stats {
        window_start: Instant::now(),
        ticks: 0,
        frames: 0,
        ticks_per_second: 0.0,
        frames_per_second: 0.0,
    })
    // The sim ticks at the game's rate on a fixed clock; frames only draw what the last tick left.
    .insert_resource(Time::<Fixed>::from_hz(f64::from(def.tick_rate.max(1))))
    .add_systems(Startup, setup)
    .add_systems(
        RunFixedMainLoop,
        collect_input.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
    )
    .add_systems(FixedUpdate, tick)
    .add_systems(Update, (crate::art::sync_room_art, sync_instances, overlay))
    .add_systems(Last, limit_frames);
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
        Text2d::new(""),
        TextFont {
            font_size: bevy::text::FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::srgb(1.0, 1.0, 0.4)),
        Anchor::TOP_LEFT,
        Transform::from_xyz(-room.0.x / 2.0 + 4.0, room.0.y / 2.0 - 4.0, 1000.0),
        Overlay,
    ));
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

/// A number with two decimals, without the sign of a tiny negative that rounds to zero.
fn two(v: f64) -> String {
    let text = format!("{v:.2}");
    if text == "-0.00" { "0.00".to_string() } else { text }
}

/// Adds this frame's key events to the collector.
fn collect_input(keys: Res<ButtonInput<KeyCode>>, mut collector: ResMut<Collector>) {
    for key in keys.get_just_pressed().filter_map(|k| key_code(*k)) {
        collector.press(key);
    }
    for key in keys.get_just_released().filter_map(|k| key_code(*k)) {
        collector.release(key);
    }
}

/// One sim tick: runs on the fixed clock, as many times per frame as the clock is behind.
fn tick(mut game: NonSendMut<Game>, mut collector: ResMut<Collector>, mut stats: ResMut<Stats>) {
    let frame = collector.take_frame();
    if game.sim.step(frame).status == SimStatus::Running {
        stats.ticks += 1;
    }
}

/// F3 shows or hides the overlay; once a second it recounts ticks and frames per second.
fn overlay(
    game: NonSend<Game>,
    keys: Res<ButtonInput<KeyCode>>,
    mut stats: ResMut<Stats>,
    mut text: Single<(&mut Text2d, &mut Visibility), With<Overlay>>,
) {
    stats.frames += 1;
    let elapsed = stats.window_start.elapsed();
    if elapsed >= Duration::from_secs(1) {
        let secs = elapsed.as_secs_f64();
        stats.ticks_per_second = f64::from(stats.ticks) / secs;
        stats.frames_per_second = f64::from(stats.frames) / secs;
        stats.ticks = 0;
        stats.frames = 0;
        stats.window_start = Instant::now();
    }
    let (label, visibility) = &mut *text;
    if keys.just_pressed(KeyCode::F3) {
        **visibility = if **visibility == Visibility::Hidden {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let status = match &game.sim.world().status {
        SimStatus::Running => String::new(),
        SimStatus::Halted(e) => format!("  HALTED: {e}"),
        SimStatus::Ended => "  ended".to_string(),
    };
    let world = game.sim.world();
    let room = game.sim.def().room(world.room).map_or_else(
        || "?".to_string(),
        |r| format!("{} ({}x{})", r.name, r.width, r.height),
    );
    let kid = match world.kid(&game.sim.rules().tick).and_then(|id| world.get(id)) {
        Some(k) => format!(
            "kid x {}  y {}  hspeed {}  vspeed {}",
            two(k.m.x().get()),
            two(k.m.y().get()),
            two(k.m.hspeed().get()),
            two(k.m.vspeed().get()),
        ),
        None => "no kid".to_string(),
    };
    label.0 = format!(
        "ticks/s {:.1} (game {})  fps {:.0}  tick {}{status}  [F3 hides]
room {room}
{kid}",
        stats.ticks_per_second,
        game.sim.def().tick_rate,
        stats.frames_per_second,
        world.tick,
    );
}

/// Holds the frame back to the cap: sleeps most of the wait, spins the last stretch.
fn limit_frames(cap: Res<Cap>, mut last: Local<Option<Instant>>) {
    let FrameCap::Fps(fps) = cap.0 else {
        *last = None;
        return;
    };
    let target = Duration::from_secs_f64(1.0 / f64::from(fps));
    let start = *last.get_or_insert_with(Instant::now);
    let spin = Duration::from_micros(1500);
    loop {
        let spent = start.elapsed();
        if spent >= target {
            break;
        }
        let left = target - spent;
        if left > spin {
            std::thread::sleep(left - spin);
        } else {
            std::hint::spin_loop();
        }
    }
    *last = Some(Instant::now());
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

/// Frame 90 asks for a screenshot (the overlay has counted a second by then); frame 150 exits (the file is written by then).
fn take_screenshot(
    mut commands: Commands,
    to: Res<ScreenshotTo>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;
    if *frame == 90 {
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(to.0.clone()));
    }
    if *frame == 150 {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_cap_parses() {
        assert_eq!("60".parse(), Ok(FrameCap::Fps(60)));
        assert_eq!("Unlimited".parse(), Ok(FrameCap::Unlimited));
        assert!("0".parse::<FrameCap>().is_err());
        assert!("fast".parse::<FrameCap>().is_err());
    }

    #[test]
    fn two_decimals_never_show_a_negative_zero() {
        assert_eq!(two(-0.000_001), "0.00");
        assert_eq!(two(-1.5), "-1.50");
        assert_eq!(two(567.4), "567.40");
    }
}
