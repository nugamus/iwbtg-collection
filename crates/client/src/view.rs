//! The window: a camera on the room and one Bevy sprite per visible sim instance.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::asset::RenderAssetUsages;
use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;
use iwbtg_sim::sim::{Sim, SimOptions};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{InstId, SimStatus};

mod hud;

use crate::bindings::PadInput;
use crate::input::{Collector, StickGate, key_code};
use crate::package::{Package, SpriteImage, sprite_images};

/// The running game. Not `Send` (the script host shares state by `Rc`), so it is a non-send
/// resource and only main-thread systems touch it.
pub(crate) struct Game {
    pub(crate) sim: Sim,
}

/// How the run started, so a retry can start it again the same way.
#[derive(Resource)]
struct RunStart {
    def: Arc<iwbtg_sim::def::GameDef>,
    options: SimOptions,
    /// Every run draws a new random seed (`replays::fresh_seed`); a `--screenshot` check keeps 0.
    fresh_seed: bool,
}

/// The Bevy image of each sprite strip, by `SpriteId`, with the strip size.
#[derive(Resource)]
struct Textures(Vec<Option<(Handle<Image>, UVec2)>>);

/// Which Bevy entity draws which instance.
#[derive(Resource, Default)]
struct Drawn(BTreeMap<InstId, Entity>);

/// The size in pixels of the room the sim is in (kept in step with the sim's room).
#[derive(Resource)]
pub(crate) struct RoomSize(pub(crate) Vec2);

/// What the screen shows of the room: view 0's rectangle in room pixels (GM coordinates, y down)
/// and its angle in degrees, or the whole room when the room has no enabled, visible view 0.
/// `w` and `h` are the size the game is scaled to fit in the window.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct ViewRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) angle: f32,
}

impl ViewRect {
    /// The whole room.
    pub(crate) fn whole(room: Vec2) -> ViewRect {
        ViewRect {
            x: 0.0,
            y: 0.0,
            w: room.x,
            h: room.y,
            angle: 0.0,
        }
    }

    /// The view's centre in the room-centred, y-up world the sprites are placed in.
    fn centre(&self, room: Vec2) -> Vec2 {
        Vec2::new(
            self.x + self.w / 2.0 - room.x / 2.0,
            room.y / 2.0 - (self.y + self.h / 2.0),
        )
    }
}

/// The fixed-clock systems that advance the sim (the tick and the clock retune). Other modules
/// gate them with a run condition, as Sudden Death does when the run is over (mods.rs).
#[derive(SystemSet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct SimTick;

/// Marks the camera that draws the game.
#[derive(Component)]
pub(crate) struct GameCamera;

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

/// Frames blend between the last two ticks with `--interpolate`. Off by default: the originals
/// draw every step, so a blended frame shows positions the game never had. `pos` is where each
/// instance was before the last tick.
#[derive(Resource, Default)]
pub(crate) struct Previous {
    on: bool,
    /// How far the fixed clock is into the next tick, 0 to 1.
    alpha: f64,
    room: Option<iwbtg_sim::def::RoomId>,
    pos: BTreeMap<InstId, (f64, f64)>,
    /// View 0 before the last tick.
    view: Option<ViewRect>,
}

/// An instance that moves farther than this in one tick (a warp, a teleport) is drawn at its
/// new place at once instead of sliding across the room.
const MAX_BLEND_DISTANCE: f64 = 32.0;

/// The drawn position `alpha` (0 to 1) of the way from `prev` to `cur`. No blend without a
/// previous position or after a jump.
fn blend(prev: Option<(f64, f64)>, cur: (f64, f64), alpha: f64) -> (f64, f64) {
    match prev {
        Some((px, py))
            if (cur.0 - px).abs() <= MAX_BLEND_DISTANCE && (cur.1 - py).abs() <= MAX_BLEND_DISTANCE =>
        {
            (px + (cur.0 - px) * alpha, py + (cur.1 - py) * alpha)
        }
        _ => cur,
    }
}

/// Blends the view `alpha` of the way from where it was before the last tick to where it is.
/// No blend without a previous view, when the size changed, or after a jump (a room change, a
/// snap to the kid, a turn of more than 45 degrees in one tick).
fn blend_view(prev: Option<ViewRect>, cur: ViewRect, alpha: f64) -> ViewRect {
    let Some(p) = prev else { return cur };
    let near = |a: f32, b: f32, max: f64| f64::from((a - b).abs()) <= max;
    let same_size = near(p.w, cur.w, 0.0) && near(p.h, cur.h, 0.0);
    if !same_size
        || !near(p.x, cur.x, MAX_BLEND_DISTANCE)
        || !near(p.y, cur.y, MAX_BLEND_DISTANCE)
        || !near(p.angle, cur.angle, 45.0)
    {
        return cur;
    }
    let mix = |a: f32, b: f32| a + (b - a) * alpha as f32;
    ViewRect {
        x: mix(p.x, cur.x),
        y: mix(p.y, cur.y),
        angle: mix(p.angle, cur.angle),
        ..cur
    }
}

/// View 0 of the sim's current room as drawn: enabled and visible, else the whole room.
fn view_of(world: &iwbtg_sim::world::World, size: Vec2) -> ViewRect {
    match world.views.first() {
        Some(v) if v.enabled && v.visible && v.w.get() >= 1.0 && v.h.get() >= 1.0 => ViewRect {
            x: v.x.get() as f32,
            y: v.y.get() as f32,
            w: v.w.get() as f32,
            h: v.h.get() as f32,
            angle: v.angle.get() as f32,
        },
        _ => ViewRect::whole(size),
    }
}

/// The part of the image holding frame `frame`. The image is a grid read row by row; the frames
/// per row follow from the image width (the sim does not carry `columns`), so a plain strip is
/// one row.
fn frame_rect(frame: u32, (w, h): (f32, f32), image: Vec2) -> Rect {
    let columns = ((image.x / w).round() as u32).max(1);
    let (x, y) = ((frame % columns) as f32 * w, (frame / columns) as f32 * h);
    Rect::new(x, y, x + w, y + h)
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
pub(crate) struct Overlay;

/// The view's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug)]
pub struct ViewArgs {
    /// Frame cap: a number of frames per second, or `unlimited`. The sim ticks at the game's
    /// rate either way.
    #[arg(long, value_name = "FPS", default_value = "60")]
    pub fps: FrameCap,
    /// Blend frames between the last two ticks for smoother motion on fast displays. Off by
    /// default: the originals draw every step, so a blended frame shows positions the game never had.
    #[arg(long)]
    pub interpolate: bool,
    /// Save a screenshot of the window to this PNG after a few frames and exit (for checks).
    #[arg(long, value_name = "PNG")]
    pub screenshot: Option<std::path::PathBuf>,
    /// Play the ranked (stripped) mode: always a new game, never the save file (docs/product.md,
    /// Two ways to play each game). The default is Play original.
    #[arg(long)]
    pub ranked: bool,
}

/// Opens the window and runs until it closes. `plugins` adds the other modules' plugins (main.rs
/// lists them), after the window and the view's own resources are in place.
pub fn run(pkg: Package, args: ViewArgs, plugins: impl FnOnce(&mut App, &Package)) -> Result<(), String> {
    let ViewArgs {
        fps: cap,
        interpolate,
        screenshot,
        ranked,
    } = args;
    let def = Arc::clone(&pkg.def);
    // Every run draws a random seed, as the originals do (docs/product.md; the replay keeps it).
    // A `--screenshot` check keeps seed 0, so it looks the same every time.
    let fresh_seed = screenshot.is_none();
    let options = SimOptions {
        // A ranked run is always a new game; Play original resumes the save file when there is
        // one, else a new game in the game's `new_game_room`. The title and menus come with the
        // rest of the client; until then the first thing on screen is the first room.
        start: if ranked {
            iwbtg_sim::room::StartPoint::NewGame { difficulty: 0 }
        } else {
            crate::savefile::start_point(&def)
        },
        seed: if fresh_seed {
            crate::replays::fresh_seed()
        } else {
            0
        },
        dll_mode: DllMode::Model,
        mode: if ranked {
            iwbtg_sim::stripped::PlayMode::Stripped
        } else {
            iwbtg_sim::stripped::PlayMode::Original
        },
        map: None,
    };
    let sim =
        Sim::new(Arc::clone(&def), options.clone()).map_err(|e| format!("the game did not start: {e}"))?;
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
    .insert_resource(ViewRect::whole(Vec2::new(width, height)))
    .insert_resource(PendingImages(images))
    .init_resource::<Drawn>()
    .insert_non_send(Game { sim })
    .insert_resource(RunStart {
        def: Arc::clone(&def),
        options,
        fresh_seed,
    })
    .insert_resource(Cap(cap))
    .insert_resource(Previous {
        on: interpolate,
        ..default()
    })
    .insert_resource(Stats {
        window_start: Instant::now(),
        ticks: 0,
        frames: 0,
        ticks_per_second: 0.0,
        frames_per_second: 0.0,
    })
    // The sim ticks at the game's rate on a fixed clock; frames only draw what the last tick left.
    .insert_resource(Time::<Fixed>::from_hz(f64::from(def.tick_rate.max(1))))
    .init_resource::<hud::SignText>()
    .add_systems(Startup, (setup, hud::setup))
    .add_systems(
        RunFixedMainLoop,
        (collect_input, collect_pad).in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
    )
    .add_systems(FixedUpdate, (tick, follow_room_speed).chain().in_set(SimTick))
    .add_systems(
        Update,
        (
            follow_view,
            (set_alpha, sync_instances, overlay).chain().after(follow_view),
            hud::show_sign,
            hud::show_countdown,
        ),
    )
    .add_systems(Last, limit_frames);
    plugins(&mut app, &pkg);
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
    screenshot: Option<Res<ScreenshotTo>>,
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
        // A `--screenshot` check never draws the overlay, whatever the setting.
        if crate::settings::get().overlay && screenshot.is_none() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
        Overlay,
    ));
    commands.spawn((
        Camera2d,
        GameCamera,
        // The backdrop's own camera draws first; this one draws the game over it.
        Camera {
            clear_color: ClearColorConfig::None,
            ..default()
        },
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

/// Points the camera at view 0 of the sim's room (the runner's view follow already moved it this
/// tick): the camera centres on the view and turns with its angle, and the overlay stays at the
/// view's top-left corner. Rooms without an enabled, visible view 0 show the whole room. Also keeps
/// `RoomSize` equal to the current room, so rooms of different sizes place their sprites right.
///
/// With `--interpolate` the view is blended between its place before the last tick and its place
/// now, like the instances, so a scrolling room does not jitter against moving sprites.
///
/// OPEN: the direction of `view_angle` is taken as counter-clockwise on screen, as the room's
/// angle column in the gravity-flip trace only logs the value
/// (nugamus/iwbtg-collection-research#94).
pub(crate) fn follow_view(
    game: NonSend<Game>,
    previous: Res<Previous>,
    fixed: Res<Time<Fixed>>,
    mut room: ResMut<RoomSize>,
    mut rect: ResMut<ViewRect>,
    mut camera: Single<&mut Transform, (With<GameCamera>, Without<Overlay>)>,
    mut overlay: Single<&mut Transform, (With<Overlay>, Without<GameCamera>)>,
) {
    let world = game.sim.world();
    let Some(def) = game.sim.def().room(world.room) else {
        return;
    };
    let size = Vec2::new(def.width as f32, def.height as f32);
    if room.0 != size {
        room.0 = size;
    }
    let mut next = view_of(world, size);
    if previous.on && previous.room == Some(world.room) {
        next = blend_view(previous.view, next, f64::from(fixed.overstep_fraction()));
    }
    if *rect != next {
        *rect = next;
    }
    let turn = Quat::from_rotation_z(next.angle.to_radians());
    let centre = next.centre(size);
    camera.translation.x = centre.x;
    camera.translation.y = centre.y;
    camera.rotation = turn;
    // The overlay sits 4 px inside the view's top-left corner and stays upright on screen.
    let corner = turn * Vec3::new(-next.w / 2.0 + 4.0, next.h / 2.0 - 4.0, 0.0);
    overlay.translation.x = centre.x + corner.x;
    overlay.translation.y = centre.y + corner.y;
    overlay.rotation = turn;
}

/// Remembers how far the fixed clock is into the next tick, for `--interpolate`.
fn set_alpha(fixed: Res<Time<Fixed>>, mut previous: ResMut<Previous>) {
    previous.alpha = f64::from(fixed.overstep_fraction());
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

/// Feeds the controller's state to the collector. Only the active controller counts: the one that
/// last had a button pressed. A DualSense over Bluetooth can show up as a second device (motion
/// sensors) whose axes never rest at zero, and that must not walk the kid right. The stick goes
/// through `StickGate`, so an axis that never rests at the centre is ignored.
fn collect_pad(
    pads: Query<(Entity, &Gamepad)>,
    mut collector: ResMut<Collector>,
    mut active: Local<Option<Entity>>,
    mut gates: Local<[StickGate; 4]>,
) {
    use bevy::input::gamepad::{GamepadAxis as A, GamepadButton as B};
    if let Some((entity, _)) = pads.iter().find(|(_, g)| g.get_pressed().next().is_some())
        && *active != Some(entity)
    {
        *active = Some(entity);
        *gates = Default::default();
    }
    let pad = active.and_then(|e| pads.get(e).ok()).map(|(_, g)| g);
    let Some(g) = pad else {
        for input in PadInput::ALL {
            collector.set_pad(input, false);
        }
        return;
    };
    let axis = |a| g.get(a).unwrap_or(0.0);
    let stick = [
        gates[0].update(axis(A::LeftStickY)),
        gates[1].update(-axis(A::LeftStickY)),
        gates[2].update(-axis(A::LeftStickX)),
        gates[3].update(axis(A::LeftStickX)),
    ];
    for input in PadInput::ALL {
        let button = |b| g.pressed(b);
        let down = match input {
            PadInput::South => button(B::South),
            PadInput::East => button(B::East),
            PadInput::West => button(B::West),
            PadInput::North => button(B::North),
            PadInput::LeftBumper => button(B::LeftTrigger),
            PadInput::RightBumper => button(B::RightTrigger),
            PadInput::LeftTrigger => button(B::LeftTrigger2),
            PadInput::RightTrigger => button(B::RightTrigger2),
            PadInput::Select => button(B::Select),
            PadInput::Start => button(B::Start),
            PadInput::DPadUp => button(B::DPadUp),
            PadInput::DPadDown => button(B::DPadDown),
            PadInput::DPadLeft => button(B::DPadLeft),
            PadInput::DPadRight => button(B::DPadRight),
            PadInput::StickUp => stick[0],
            PadInput::StickDown => stick[1],
            PadInput::StickLeft => stick[2],
            PadInput::StickRight => stick[3],
        };
        collector.set_pad(input, down);
    }
}

/// Starts the run again from where it started (quick-retry and auto-retry, `retry.rs`): a new
/// sim with the same options (and a new random seed), every drawn sprite gone, no pending input edges, no sign. The new sim's
/// room-start output stays in `output` for its first step (#466). The fixed clock keeps its rate until the next
/// tick's `follow_room_speed`.
pub(crate) fn restart_run(world: &mut World) -> Result<(), String> {
    let start = world.resource::<RunStart>();
    let mut options = start.options.clone();
    if start.fresh_seed {
        options.seed = crate::replays::fresh_seed();
    }
    let sim =
        Sim::new(Arc::clone(&start.def), options).map_err(|e| format!("the game did not start: {e}"))?;
    world.non_send_mut::<Game>().sim = sim;
    let drawn = std::mem::take(&mut world.resource_mut::<Drawn>().0);
    for entity in drawn.into_values() {
        world.despawn(entity);
    }
    {
        let mut previous = world.resource_mut::<Previous>();
        previous.room = None;
        previous.pos.clear();
    }
    // Keys held through the retry stay held; edges from before it are dropped.
    world.resource_mut::<Collector>().take_frame();
    world.resource_mut::<hud::SignText>().0 = None;
    Ok(())
}

/// The view's run resources without a window, for tests of the systems that use them.
#[cfg(test)]
pub(crate) fn test_world(pkg: &Package) -> World {
    let options = SimOptions {
        start: iwbtg_sim::room::StartPoint::NewGame { difficulty: 0 },
        seed: 0,
        dll_mode: DllMode::Model,
        mode: iwbtg_sim::stripped::PlayMode::Original,
        map: None,
    };
    let sim = Sim::new(Arc::clone(&pkg.def), options.clone()).unwrap();
    let mut world = World::new();
    world.insert_non_send(Game { sim });
    world.insert_resource(RunStart {
        def: Arc::clone(&pkg.def),
        options,
        fresh_seed: false,
    });
    world.init_resource::<Drawn>();
    world.init_resource::<Previous>();
    world.insert_resource(Collector::new(crate::bindings::Bindings::default()));
    world.init_resource::<hud::SignText>();
    world
}

/// One sim tick: runs on the fixed clock, as many times per frame as the clock is behind.
#[allow(
    clippy::too_many_arguments,
    reason = "a Bevy system: one parameter per resource"
)]
fn tick(
    mut game: NonSendMut<Game>,
    mut collector: ResMut<Collector>,
    mut stats: ResMut<Stats>,
    mut audio: ResMut<crate::audio::Queue>,
    mut shake: ResMut<crate::audio::Shake>,
    mut previous: ResMut<Previous>,
    mut sign: ResMut<hud::SignText>,
    mut replay: Option<ResMut<crate::replays::Recorder>>,
) {
    if previous.on {
        let world = game.sim.world();
        previous.room = Some(world.room);
        previous.view = game
            .sim
            .def()
            .room(world.room)
            .map(|d| view_of(world, Vec2::new(d.width as f32, d.height as f32)));
        previous.pos = world
            .instances()
            .iter()
            .filter(|i| i.alive)
            .map(|i| (i.id, (i.m.x().get(), i.m.y().get())))
            .collect();
    }
    let frame = collector.take_frame();
    let was_running = game.sim.world().status == SimStatus::Running;
    let step = game.sim.step(frame);
    if was_running && let Some(replay) = replay.as_deref_mut() {
        replay.record(&game.sim, frame, &step.splits);
    }
    if step.status == SimStatus::Running {
        stats.ticks += 1;
        let output = &game.sim.world().output;
        hud::note_tick(&mut sign, output);
        audio.push(Instant::now(), output);
        shake.tick(output);
    }
}

/// The ticks per second the sim's current room speed asks for: `room_speed` as the room file or
/// the game set it, at least 1 (a game that sets 0 or less would stop the clock) and at most 1000.
fn room_hz(room_speed: f64) -> f64 {
    if room_speed.is_finite() {
        room_speed.clamp(1.0, 1000.0)
    } else {
        1.0
    }
}

/// Sets the fixed clock to the rate `room_speed` asks for, when it is not already that.
fn retune(fixed: &mut Time<Fixed>, room_speed: f64) {
    let hz = room_hz(room_speed);
    if (fixed.timestep().as_secs_f64().recip() - hz).abs() > 1e-6 {
        fixed.set_timestep_hz(hz);
    }
}

/// Retunes the fixed clock when the sim's `room_speed` changes (a room's file speed on entry, or a
/// game setting it), so the game ticks at the rate it asks for. Runs after the sim's tick, so the
/// next tick already uses the new rate.
fn follow_room_speed(game: NonSend<Game>, mut fixed: ResMut<Time<Fixed>>) {
    retune(&mut fixed, game.sim.world().room_speed.get());
}

/// F3 shows or hides the overlay; once a second it recounts ticks and frames per second.
fn overlay(
    game: NonSend<Game>,
    keys: Res<ButtonInput<KeyCode>>,
    fixed: Res<Time<Fixed>>,
    mut stats: ResMut<Stats>,
    collector: Res<Collector>,
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
        crate::settings::update(|s| s.overlay = **visibility != Visibility::Hidden);
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
{kid}
{input}",
        stats.ticks_per_second,
        fixed.timestep().as_secs_f64().recip().round(),
        stats.frames_per_second,
        world.tick,
        input = collector.describe(),
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
    previous: Res<Previous>,
    mut query: Query<(&mut Sprite, &mut Transform)>,
) {
    let Some(textures) = textures else { return };
    let def = game.sim.def();
    let alpha = previous.alpha;
    let same_room = previous.room == Some(game.sim.world().room);
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
        let cur = (inst.m.x().get(), inst.m.y().get());
        let (px, py) = if previous.on {
            blend(
                previous.pos.get(&inst.id).copied().filter(|_| same_room),
                cur,
                alpha,
            )
        } else {
            cur
        };
        let x = px as f32 + off.x - room.0.x / 2.0;
        let y = room.0.y / 2.0 - (py as f32 + off.y);
        let z = -(inst.depth.get() as f32) / 1000.0 + (inst.id.0 % 1000) as f32 * 1e-6;
        let rect = frame_rect(frame, (w, h), strip.as_vec2());
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
pub(crate) struct ScreenshotTo(pub(crate) std::path::PathBuf);

/// Frame 90 asks for a screenshot (the overlay has counted a second by then); frame 150 exits (the file is written by then).
pub(crate) fn take_screenshot(
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
    fn blend_moves_part_of_the_way_and_never_across_a_jump() {
        assert_eq!(blend(Some((0.0, 0.0)), (10.0, 20.0), 0.5), (5.0, 10.0));
        assert_eq!(blend(Some((0.0, 0.0)), (10.0, 20.0), 1.0), (10.0, 20.0));
        assert_eq!(blend(Some((0.0, 0.0)), (10.0, 20.0), 0.0), (0.0, 0.0));
        assert_eq!(blend(None, (10.0, 20.0), 0.5), (10.0, 20.0));
        assert_eq!(blend(Some((0.0, 0.0)), (300.0, 0.0), 0.5), (300.0, 0.0));
    }

    #[test]
    fn frames_come_from_the_grid_row_by_row() {
        let strip = Vec2::new(40.0, 10.0);
        assert_eq!(
            frame_rect(3, (10.0, 10.0), strip),
            Rect::new(30.0, 0.0, 40.0, 10.0)
        );
        let grid = Vec2::new(30.0, 20.0);
        assert_eq!(
            frame_rect(4, (10.0, 10.0), grid),
            Rect::new(10.0, 10.0, 20.0, 20.0)
        );
    }

    #[test]
    fn two_decimals_never_show_a_negative_zero() {
        assert_eq!(two(-0.000_001), "0.00");
        assert_eq!(two(-1.5), "-1.50");
        assert_eq!(two(567.4), "567.40");
    }

    #[test]
    fn the_clock_follows_room_speed_within_sane_bounds() {
        assert_eq!(room_hz(50.0), 50.0);
        assert_eq!(room_hz(25.0), 25.0);
        // A game that sets 0 or a negative speed must not stop or reverse the clock.
        assert_eq!(room_hz(0.0), 1.0);
        assert_eq!(room_hz(-30.0), 1.0);
        assert_eq!(room_hz(1e9), 1000.0);
        assert_eq!(room_hz(f64::NAN), 1.0);
    }

    #[test]
    fn the_fixed_clock_is_retuned_when_room_speed_changes_and_left_alone_otherwise() {
        let mut fixed = Time::<Fixed>::from_hz(50.0);
        retune(&mut fixed, 50.0);
        assert_eq!(fixed.timestep(), Duration::from_millis(20));
        // A room at 25 steps per second: 40 ms per tick.
        retune(&mut fixed, 25.0);
        assert_eq!(fixed.timestep(), Duration::from_millis(40));
        // Back to the game's rate on the next room.
        retune(&mut fixed, 50.0);
        assert_eq!(fixed.timestep(), Duration::from_millis(20));
        // A game that sets a silly speed gets the clamped one.
        retune(&mut fixed, 0.0);
        assert_eq!(fixed.timestep(), Duration::from_secs(1));
    }

    #[test]
    fn a_view_centres_the_camera_in_the_room_centred_world() {
        let room = Vec2::new(2400.0, 608.0);
        // The whole room: the world's origin.
        assert_eq!(ViewRect::whole(room).centre(room), Vec2::ZERO);
        // An 800 x 608 view at the room's left edge is 800 px left of the room's centre.
        let v = ViewRect {
            x: 0.0,
            y: 0.0,
            w: 800.0,
            h: 608.0,
            angle: 0.0,
        };
        assert_eq!(v.centre(room), Vec2::new(-800.0, 0.0));
        // Scrolled 100 px right and 50 px down: right and down in GM, so +x and -y here.
        let v = ViewRect {
            x: 100.0,
            y: 50.0,
            ..v
        };
        assert_eq!(v.centre(room), Vec2::new(-700.0, -50.0));
    }

    fn view(x: f32, y: f32) -> ViewRect {
        ViewRect {
            x,
            y,
            w: 800.0,
            h: 608.0,
            angle: 0.0,
        }
    }

    #[test]
    fn the_view_moves_part_of_the_way_between_ticks() {
        let mid = blend_view(Some(view(100.0, 0.0)), view(108.0, 4.0), 0.5);
        assert_eq!((mid.x, mid.y), (104.0, 2.0));
        assert_eq!(
            blend_view(Some(view(100.0, 0.0)), view(108.0, 4.0), 1.0),
            view(108.0, 4.0)
        );
        assert_eq!(
            blend_view(Some(view(100.0, 0.0)), view(108.0, 4.0), 0.0),
            view(100.0, 0.0)
        );
        // The turn is blended too.
        let turned = ViewRect {
            angle: 12.0,
            ..view(0.0, 0.0)
        };
        assert_eq!(blend_view(Some(view(0.0, 0.0)), turned, 0.5).angle, 6.0);
    }

    #[test]
    fn the_view_never_slides_across_a_jump_or_a_resize() {
        // No previous view, a snap of the view, a resize and a half turn all land at once.
        assert_eq!(blend_view(None, view(500.0, 0.0), 0.5), view(500.0, 0.0));
        assert_eq!(
            blend_view(Some(view(0.0, 0.0)), view(500.0, 0.0), 0.5),
            view(500.0, 0.0)
        );
        let small = ViewRect {
            w: 400.0,
            ..view(0.0, 0.0)
        };
        assert_eq!(blend_view(Some(view(0.0, 0.0)), small, 0.5), small);
        let flipped = ViewRect {
            angle: 180.0,
            ..view(0.0, 0.0)
        };
        assert_eq!(blend_view(Some(view(0.0, 0.0)), flipped, 0.5), flipped);
    }
}
