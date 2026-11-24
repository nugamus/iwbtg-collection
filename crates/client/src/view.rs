//! The window: a camera on the room and one Bevy sprite per visible sim instance.

use std::collections::BTreeMap;
use std::sync::Arc;
use crate::time::{Duration, Instant};

use bevy::app::RunFixedMainLoopSystems;
use bevy::asset::RenderAssetUsages;
use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use iwbtg_sim::script::PreparedScripts;
use iwbtg_sim::sim::{Sim, SimOptions};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{InstId, SimStatus};

mod hud;
pub(crate) mod order;
pub(crate) mod transition;

use crate::bindings::PadInput;
use crate::input::{Collector, PadReader, key_code};
use crate::package::{Package, sprite_images};
use crate::shell::{AppScreen, InRun, LoadedPackage, RUN_ONLY, RunSteps};

/// The running game. Not `Send` (the script host shares state by `Rc`), so it is a non-send
/// resource and only main-thread systems touch it.
pub(crate) struct Game {
    pub(crate) sim: Sim,
}

/// The package's compiled scripts, prepared once at run start and reused by a quick retry
/// (#1093B). `PreparedScripts` is `!Send` (its Rhai ASTs are `Rc`), so it lives beside `Game` as
/// a non-send resource, never in the `Send + Sync` `RunStart`.
struct Prepared(PreparedScripts);

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
pub(crate) struct Textures(pub(crate) Vec<Option<(Handle<Image>, UVec2)>>);

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

/// The system that places the instances' sprites. The room art fills `order::Ladder` before it.
#[derive(SystemSet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct DrawInstances;

/// How fast the fixed clock runs against the rate the game asks for: 1 normally, less under Slow
/// motion (practice.rs). It changes only how often a tick runs, never what a tick computes, so
/// it is not in the replay. Sounds play at the same factor (audio.rs) to stay on the tick clock.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct ClockSpeed(pub f64);

impl Default for ClockSpeed {
    fn default() -> ClockSpeed {
        ClockSpeed(1.0)
    }
}

/// Marks the camera that draws the game.
#[derive(Component)]
pub(crate) struct GameCamera;

/// How many frames per second the window draws at most. The sim ticks at the game's rate
/// whatever this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameCap {
    /// At most this many frames per second.
    Fps(u32),
    /// No limit.
    Unlimited,
}

impl Default for FrameCap {
    /// The browser build's frame cap, the value clap gives when `--fps` is absent.
    fn default() -> FrameCap {
        FrameCap::Fps(60)
    }
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
pub(crate) struct Stats {
    pub(crate) window_start: Instant,
    pub(crate) ticks: u32,
    pub(crate) frames: u32,
    pub(crate) ticks_per_second: f64,
    pub(crate) frames_per_second: f64,
}

/// The view's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Clone, Default)]
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

/// The view's place in the module list (main.rs): its flags. `view::run` builds the app itself.
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = ViewArgs;
}

/// How runs start: from the view's flags, the same for every package opened.
#[derive(Resource, Clone, Copy)]
struct RunConfig {
    ranked: bool,
    /// Every run draws a new random seed (`replays::fresh_seed`); a `--screenshot` check keeps 0.
    fresh_seed: bool,
    interpolate: bool,
}

/// A run's sim and how it started, built but not yet in the world.
pub(crate) struct NewRun {
    game: Game,
    start: RunStart,
    prepared: Prepared,
}

impl NewRun {
    /// Puts the sim, its prepared scripts and how it started in `world`.
    pub(crate) fn install(self, world: &mut World) {
        world.insert_non_send(self.game);
        world.insert_non_send(self.prepared);
        world.insert_resource(self.start);
    }
}

/// Builds the sim for `pkg` (`shell::load` calls this before the run opens, so a game that cannot
/// be set up never opens; nothing in `world` changes). A start that halts is not an error here.
/// `map` is the map id, `None` the first.
pub(crate) fn new_run(world: &World, pkg: &Package, map: Option<&str>) -> Result<NewRun, String> {
    let config = *world.resource::<RunConfig>();
    let def = Arc::clone(&pkg.def);
    let options = SimOptions {
        // A ranked run is always a new game; Play original resumes the save file when there is
        // one, else a new game in the game's `new_game_room`. The title and menus come with the
        // rest of the client; until then the first thing on screen is the first room.
        start: if config.ranked {
            iwbtg_sim::room::StartPoint::NewGame { difficulty: 0 }
        } else {
            crate::savefile::start_point(&def)
        },
        // Every run draws a random seed, as the originals do (docs/product.md; the replay keeps
        // it). A `--screenshot` check keeps seed 0, so it looks the same every time.
        seed: if config.fresh_seed {
            crate::replays::fresh_seed()
        } else {
            0
        },
        dll_mode: DllMode::Model,
        mode: if config.ranked {
            iwbtg_sim::stripped::PlayMode::Stripped
        } else {
            iwbtg_sim::stripped::PlayMode::Original
        },
        map: map.map(str::to_owned),
    };
    // The sim constructors report a map error before compiling scripts. Keep that precedence:
    // when the map is not there, let the unprepared constructor produce the error (it cannot
    // succeed without a map).
    if def.map_index(options.map.as_deref()).is_none() {
        return Err(Sim::new_or_halted(Arc::clone(&def), options)
            .err()
            .map_or_else(
                || "the game did not start: the map is missing".to_owned(),
                |e| format!("the game did not start: {e}"),
            ));
    }
    // Compile the scripts once and keep the artifact for the run, so a quick retry does not
    // compile them again (#1093B).
    let prepared = Sim::prepare(&def).map_err(|e| format!("the game did not start: {e}"))?;
    // A start that halts (a script error in a start-room Create, #946) is a run whose sim is
    // halted from tick 0: it opens on the script error screen (`script_error/`), as a halt in
    // play does. Only a sim that cannot be set up is an error.
    let sim = Sim::new_or_halted_prepared(Arc::clone(&def), options.clone(), &prepared)
        .map_err(|e| format!("the game did not start: {e}"))?;
    def.room(sim.world().room).ok_or("the start room is missing")?;
    Ok(NewRun {
        game: Game { sim },
        start: RunStart {
            def,
            options,
            fresh_seed: config.fresh_seed,
        },
        prepared: Prepared(prepared),
    })
}

/// The size of the sim's room, in pixels.
fn room_size(game: &Game) -> Vec2 {
    let sim = &game.sim;
    sim.def()
        .room(sim.world().room)
        .map_or(Vec2::new(800.0, 608.0), |r| {
            Vec2::new(r.width as f32, r.height as f32)
        })
}

/// The view's run: the resources and systems that draw and tick the sim while the shell is in
/// `AppScreen::Run` (`shell`, #782). The window is `run`'s.
pub(crate) struct ViewPlugin {
    pub(crate) ranked: bool,
    pub(crate) fresh_seed: bool,
    pub(crate) interpolate: bool,
}

impl ViewPlugin {
    fn config(&self) -> RunConfig {
        RunConfig {
            ranked: self.ranked,
            fresh_seed: self.fresh_seed,
            interpolate: self.interpolate,
        }
    }
}

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config())
            .insert_resource(Stats {
                window_start: Instant::now(),
                ticks: 0,
                frames: 0,
                ticks_per_second: 0.0,
                frames_per_second: 0.0,
            })
            .add_systems(OnEnter(AppScreen::Run), enter.in_set(RunSteps::Core))
            .add_systems(OnEnter(AppScreen::Run), (setup, hud::setup).in_set(InRun))
            .add_systems(OnExit(AppScreen::Run), leave.in_set(RunSteps::Core))
            .add_systems(
                RunFixedMainLoop,
                (collect_input, collect_pad)
                    .in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop)
                    .run_if(in_state(AppScreen::Run)),
            )
            .add_systems(FixedUpdate, (tick, follow_room_speed).chain().in_set(SimTick))
            .add_systems(
                Update,
                (
                    follow_view,
                    (set_alpha, sync_instances.in_set(DrawInstances))
                        .chain()
                        .after(follow_view),
                    hud::show_sign,
                    hud::show_countdown,
                )
                    .in_set(InRun),
            );
    }
}

/// Opens the run's view: the room size, the fixed clock at the game's rate, what is drawn, the
/// window title. The sim is already there (`new_run`).
fn enter(world: &mut World) {
    let config = *world.resource::<RunConfig>();
    let size = room_size(world.non_send::<Game>());
    let def = Arc::clone(&world.resource::<RunStart>().def);
    let speed = world.get_resource::<ClockSpeed>().copied().unwrap_or_default();
    world.insert_resource(RoomSize(size));
    world.insert_resource(ViewRect::whole(size));
    world.init_resource::<Drawn>();
    world.insert_resource(Previous {
        on: config.interpolate,
        ..default()
    });
    world.init_resource::<hud::SignText>();
    // A `--hud-preview-sign` check raises the box at the run's start, as a sign that has just
    // spoken leaves it, so the screenshot shows the latch rather than needing a real Up press.
    if let Some(text) = crate::hud::sign_preview() {
        world.resource_mut::<hud::SignText>().raise(text.to_owned());
    }
    world.init_resource::<transition::Transition>();
    // The sim ticks at the game's rate on a fixed clock; frames only draw what the last tick
    // left. Slow motion (practice.rs) runs the first tick slow already; `follow_room_speed`
    // keeps it so after every tick.
    world.insert_resource(Time::<Fixed>::from_hz(f64::from(def.tick_rate.max(1)) * speed.0));
    let mut windows = world.query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>();
    for mut window in windows.iter_mut(world) {
        window.title = format!("IWBTG Collection: {}", def.id);
    }
}

/// Closes the run's view; the sim goes last, after every module's own exit systems.
fn leave(world: &mut World) {
    world.remove_resource::<Textures>();
    world.remove_resource::<Drawn>();
    world.remove_resource::<RoomSize>();
    world.remove_resource::<ViewRect>();
    world.remove_resource::<Previous>();
    world.remove_resource::<hud::SignText>();
    world.remove_resource::<transition::Transition>();
    world.remove_resource::<RunStart>();
    world.remove_non_send::<Prepared>();
    world.remove_non_send::<Game>();
}

/// The first package a client starts with: a path natively, or the browser's fetched files.
enum First<'a> {
    Path(&'a std::path::Path),
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    Package(crate::package::Package, std::path::PathBuf),
}

/// Opens the window and runs until it closes. `first` is the package from the command line,
/// played at once (the window takes its first room's size); without it the shell starts on the
/// select screen. `plugins` adds the other modules' plugins (main.rs lists them).
pub fn run(
    first: Option<&std::path::Path>,
    args: ViewArgs,
    trace_mode: bool,
    plugins: impl FnOnce(&mut App),
) -> Result<(), String> {
    run_inner(first.map(First::Path), args, trace_mode, plugins)
}

/// [`run`] with the first package already in memory: the browser fetched an `.iwbz`
/// (docs/architecture/client/web.md, #29). `source` names it in messages, its URL.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub fn run_package(
    first: Option<(crate::package::Package, std::path::PathBuf)>,
    args: ViewArgs,
    plugins: impl FnOnce(&mut App),
) -> Result<(), String> {
    run_inner(
        first.map(|(pkg, source)| First::Package(pkg, source)),
        args,
        false,
        plugins,
    )
}

fn run_inner(
    first: Option<First>,
    args: ViewArgs,
    trace_mode: bool,
    plugins: impl FnOnce(&mut App),
) -> Result<(), String> {
    let ViewArgs {
        fps: cap,
        interpolate,
        screenshot,
        ranked,
    } = args;
    let view = ViewPlugin {
        ranked,
        fresh_seed: screenshot.is_none(),
        interpolate,
    };
    let mut app = App::new();
    // `shell::load` builds the first sim with it, before the plugins are in.
    app.insert_resource(view.config());
    let (title, (width, height)) = match first {
        Some(First::Path(path)) => {
            crate::shell::load(app.world_mut(), path, trace_mode).map_err(|e| e.to_string())?;
            first_title(app.world())
        }
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        Some(First::Package(pkg, source)) => {
            crate::shell::load_package(app.world_mut(), pkg, source).map_err(|e| e.to_string())?;
            first_title(app.world())
        }
        None => (crate::shell::select::TITLE.to_owned(), (800, 608)),
    };
    app.add_plugins(
        DefaultPlugins
            // Drops the ICU4X segmentation notice (logging.rs, #734).
            .set(crate::logging::log_plugin())
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title,
                    resolution: (width, height).into(),
                    // The frame cap does the pacing; vsync would pin it to the monitor.
                    present_mode: bevy::window::PresentMode::AutoNoVsync,
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins((crate::render_fix::plugin, view))
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(Cap(cap))
    .add_systems(Last, limit_frames);
    plugins(&mut app);
    if let Some(path) = screenshot {
        // `screen` pins the size when `--window` gives one (`ShotSize`).
        add_screenshot(&mut app, path, None);
    }
    shot_result(app.run())
}

/// The window title and size of the first package, which is already the loaded one.
fn first_title(world: &World) -> (String, (u32, u32)) {
    let size = room_size(world.non_send::<Game>());
    let id = world.resource::<RunStart>().def.id.clone();
    (format!("IWBTG Collection: {id}"), (size.x as u32, size.y as u32))
}

/// The game camera and the sprite images.
fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    loaded: Res<LoadedPackage>,
    room: Res<RoomSize>,
) {
    // The room always fits the window, whatever its size: letterboxed, never cropped.
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
        RUN_ONLY,
    ));
    let textures = sprite_images(&loaded.pkg.files)
        .into_iter()
        .map(|s| {
            s.map(|s| {
                let size = UVec2::new(s.size.0, s.size.1);
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
                (images.add(image), size)
            })
        })
        .collect();
    commands.insert_resource(Textures(textures));
}

/// Points the camera at view 0 of the sim's room (the runner's view follow already moved it this
/// tick): the camera centres on the view and turns with its angle. Rooms without an enabled,
/// visible view 0 show the whole room. Also keeps `RoomSize` equal to the current room, so rooms
/// of different sizes place their sprites right.
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
    mut camera: Single<&mut Transform, With<GameCamera>>,
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
}

/// Remembers how far the fixed clock is into the next tick, for `--interpolate`.
fn set_alpha(fixed: Res<Time<Fixed>>, mut previous: ResMut<Previous>) {
    previous.alpha = f64::from(fixed.overstep_fraction());
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

/// Feeds the active controller's state to the collector (`PadReader`, the same on native and web).
fn collect_pad(
    pads: Query<(Entity, &Gamepad)>,
    mut collector: ResMut<Collector>,
    mut reader: Local<PadReader>,
) {
    let pads: Vec<(Entity, &Gamepad)> = pads.iter().collect();
    let down = reader.read(&pads);
    for input in PadInput::ALL {
        collector.set_pad(input, down[input as usize]);
    }
}

/// Starts the run again from where it started (quick-retry and auto-retry, `retry.rs`): a new
/// sim with the same options (and a new random seed), every drawn sprite gone, no pending input edges, no sign. The new sim's
/// room-start output stays in `output` for its first step (#466). The fixed clock keeps its rate until the next
/// tick's `follow_room_speed`.
pub(crate) fn restart_run(world: &mut World) -> Result<(), String> {
    let start = world.resource::<RunStart>();
    // The scripts were prepared at run start; a retry reuses that artifact (#1093B).
    let prepared = world.non_send::<Prepared>();
    let mut options = start.options.clone();
    if start.fresh_seed {
        options.seed = crate::replays::fresh_seed();
    }
    // A start that halts comes back halted, so the script error screen opens again (#946).
    let sim = Sim::new_or_halted_prepared(Arc::clone(&start.def), options, &prepared.0)
        .map_err(|e| format!("the game did not start: {e}"))?;
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
    world.resource_mut::<hud::SignText>().clear();
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
    let prepared = Sim::prepare(&pkg.def).unwrap();
    let sim = Sim::new_prepared(Arc::clone(&pkg.def), options.clone(), &prepared).unwrap();
    let mut world = World::new();
    world.insert_non_send(Game { sim });
    world.insert_non_send(Prepared(prepared));
    world.insert_resource(RunStart {
        def: Arc::clone(&pkg.def),
        options,
        fresh_seed: false,
    });
    world.init_resource::<Drawn>();
    world.init_resource::<Previous>();
    world.insert_resource(Collector::new(crate::bindings::Bindings::default()));
    world.init_resource::<hud::SignText>();
    world.init_resource::<transition::Transition>();
    world
}

/// One sim tick: runs on the fixed clock, as many times per frame as the clock is behind.
/// `pub(crate)` so other modules can order against it (skip.rs runs first).
#[allow(
    clippy::too_many_arguments,
    reason = "a Bevy system: one parameter per resource"
)]
pub(crate) fn tick(
    mut game: NonSendMut<Game>,
    mut collector: ResMut<Collector>,
    mut stats: ResMut<Stats>,
    mut audio: ResMut<crate::audio::Queue>,
    mut shake: ResMut<crate::audio::Shake>,
    mut previous: ResMut<Previous>,
    mut sign: ResMut<hud::SignText>,
    mut transition: ResMut<transition::Transition>,
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
        transition.observe(output, Instant::now());
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

/// Sets the fixed clock to the rate `room_speed` asks for, times the clock speed, when it is not
/// already that.
fn retune(fixed: &mut Time<Fixed>, room_speed: f64, speed: ClockSpeed) {
    let hz = room_hz(room_speed) * speed.0;
    if (fixed.timestep().as_secs_f64().recip() - hz).abs() > 1e-6 {
        fixed.set_timestep_hz(hz);
    }
}

/// Retunes the fixed clock when the sim's `room_speed` changes (a room's file speed on entry, or a
/// game setting it), so the game ticks at the rate it asks for. Runs after the sim's tick, so the
/// next tick already uses the new rate.
fn follow_room_speed(game: NonSend<Game>, speed: Option<Res<ClockSpeed>>, mut fixed: ResMut<Time<Fixed>>) {
    let speed = speed.map_or_else(ClockSpeed::default, |s| *s);
    retune(&mut fixed, game.sim.world().room_speed.get(), speed);
}

/// Holds the frame back to the cap: sleeps most of the wait, spins the last stretch.
fn limit_frames(cap: Res<Cap>, last: Local<Option<Instant>>) {
    // A browser paces the window with animation frames, and `std::thread::sleep` panics on
    // wasm32-unknown-unknown; the sim's fixed clock does not depend on the frame rate anyway.
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        let _ = (cap, last);
    }
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        let mut last = last;
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
}

/// Makes the entities match the sim's instances: spawn, move, frame, remove. Each one's z is its
/// place in the sim's draw order among the room's tile layers (`order`, #789).
#[allow(
    clippy::too_many_arguments,
    reason = "a Bevy system: each is one resource it reads"
)]
fn sync_instances(
    mut commands: Commands,
    game: NonSend<Game>,
    textures: Option<Res<Textures>>,
    room: Res<RoomSize>,
    mut drawn: ResMut<Drawn>,
    previous: Res<Previous>,
    ladder: Option<Res<order::Ladder>>,
    mut query: Query<(&mut Sprite, &mut Transform)>,
) {
    let Some(textures) = textures else { return };
    let def = game.sim.def();
    let world = game.sim.world();
    let alpha = previous.alpha;
    let same_room = previous.room == Some(world.room);
    let mut alive = BTreeMap::new();
    let none = order::Ladder::default();
    let ladder = ladder.as_deref().unwrap_or(&none);
    for (id, z) in order::instance_z(world, ladder, |i| i.img.sprite().is_some()) {
        let Some(inst) = world.get(id) else { continue };
        let Some(sprite_id) = inst.img.sprite() else {
            continue;
        };
        let Some(sprite_def) = def.sprites.get(sprite_id.0 as usize) else {
            continue;
        };
        let Some(Some((handle, strip))) = textures.0.get(sprite_id.0 as usize) else {
            continue;
        };
        let frame = u32::from(inst.img.subimage(sprite_def.frames).unwrap_or(0));
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
        let (rect, transform) = place_sprite(
            sprite_def,
            frame,
            strip.as_vec2(),
            Vec2::new(px as f32, py as f32),
            Vec2::new(inst.img.xscale().get() as f32, inst.img.yscale().get() as f32),
            inst.img.angle().get() as f32,
            z,
            room.0,
        );
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
                    RUN_ONLY,
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

/// Where frame `frame` of a sprite drawn at room position `at` (GM: y down from the room corner)
/// with `scale`, `angle` (degrees, counter-clockwise) and `z` goes: the part of its image `strip`
/// and the transform in the room-centred space the game camera sees.
#[allow(
    clippy::too_many_arguments,
    reason = "each is one of the sprite's draw values"
)]
pub(crate) fn place_sprite(
    sprite_def: &iwbtg_sim::def::SpriteDef,
    frame: u32,
    strip: Vec2,
    at: Vec2,
    scale: Vec2,
    angle: f32,
    z: f32,
    room: Vec2,
) -> (Rect, Transform) {
    let (w, h) = (sprite_def.size.0 as f32, sprite_def.size.1 as f32);
    let angle = angle.to_radians();
    // The sprite's centre relative to its origin, scaled and turned like the sprite.
    let off = Vec2::new(
        (w / 2.0 - sprite_def.origin.0 as f32) * scale.x,
        (h / 2.0 - sprite_def.origin.1 as f32) * scale.y,
    );
    let (sin, cos) = angle.sin_cos();
    let off = Vec2::new(off.x * cos + off.y * sin, -off.x * sin + off.y * cos);
    // GM: y grows downward from the room corner. Bevy: y up from the centre.
    let x = at.x + off.x - room.x / 2.0;
    let y = room.y / 2.0 - (at.y + off.y);
    let transform = Transform::from_xyz(x, y, z)
        .with_rotation(Quat::from_rotation_z(angle))
        .with_scale(scale.extend(1.0));
    (frame_rect(frame, (w, h), strip), transform)
}

/// Where `--screenshot` writes.
#[derive(Resource)]
pub(crate) struct ScreenshotTo(pub(crate) std::path::PathBuf);

/// The size in physical px a `--screenshot` picture must have (`--window`, or the gallery's
/// `--ratio`). The window is held at it and a capture of another size is not saved (#861).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShotSize(pub(crate) UVec2);

/// Why the capture was not saved; the run then exits with an error.
#[derive(Resource, Default)]
struct ShotFailed(Option<String>);

/// The time one frame of a `--screenshot` run advances the clocks by (#942).
pub(crate) const SHOT_FRAME: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// Sets up a `--screenshot` run: the capture at frame 90, the exit at frame 150 and, with `size`,
/// the window held at that size. Finish with [`shot_result`] on what `App::run` returns.
///
/// Every frame advances time by [`SHOT_FRAME`], not by the real frame time, so the fixed clock
/// (the sim, the HUD timer, the paused timer) and the real-time animations (the loading cherry,
/// tweens) stand at the same point at frame 90 on any build and machine (#942).
pub(crate) fn add_screenshot(app: &mut App, path: std::path::PathBuf, size: Option<UVec2>) {
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(SHOT_FRAME))
        .insert_resource(ScreenshotTo(path))
        .init_resource::<ShotFailed>()
        .add_systems(Update, (hold_shot_size, take_screenshot).chain());
    if let Some(size) = size {
        app.insert_resource(ShotSize(size));
    }
}

/// The run's outcome: an error when the screenshot was not saved.
pub(crate) fn shot_result(exit: AppExit) -> Result<(), String> {
    match exit {
        AppExit::Success => Ok(()),
        AppExit::Error(_) => Err("error: the screenshot was not saved".to_string()),
    }
}

/// Keeps a `--screenshot` window at the size asked for. The window cannot be resized or
/// maximized, and a size change from outside (a maximize from the OS or the desktop, a snap) is
/// undone: on 2026-11-16 a maximized 800x608 check saved a 7680x2071 picture (#861).
fn hold_shot_size(
    size: Option<Res<ShotSize>>,
    mut window: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    let (Some(size), Ok(mut window)) = (size, window.single_mut()) else {
        return;
    };
    if window.resizable || window.enabled_buttons.maximize {
        window.resizable = false;
        window.enabled_buttons.maximize = false;
    }
    if window.physical_size() != size.0 {
        window.set_maximized(false);
        window.resolution.set_physical_resolution(size.0.x, size.0.y);
    }
}

/// `None` when a capture of `got` px may be saved, else why not.
fn shot_size_problem(got: UVec2, want: Option<UVec2>) -> Option<String> {
    let want = want?;
    (got != want).then(|| {
        format!(
            "error: screenshot is {}x{} px, not the {}x{} asked for (was the window resized or maximized?); not saved",
            got.x, got.y, want.x, want.y
        )
    })
}

/// Frame 90 asks for a screenshot (the overlay has counted a second by then); frame 150 exits (the file is written by then).
fn take_screenshot(
    mut commands: Commands,
    to: Res<ScreenshotTo>,
    size: Option<Res<ShotSize>>,
    failed: Res<ShotFailed>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
    *frame += 1;
    if *frame == 90 {
        let want = size.map(|s| s.0);
        let mut save = save_to_disk(to.0.clone());
        commands.spawn(Screenshot::primary_window()).observe(
            move |shot: On<ScreenshotCaptured>, mut failed: ResMut<ShotFailed>| {
                let got = shot.image.size();
                match shot_size_problem(got, want) {
                    None => save(shot),
                    Some(e) => {
                        eprintln!("{e}");
                        failed.0 = Some(e);
                    }
                }
            },
        );
    }
    if *frame == 150 {
        exit.write(if failed.0.is_some() {
            AppExit::error()
        } else {
            AppExit::Success
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screenshot_run_advances_time_by_frames_not_by_the_wall_clock() {
        use bevy::time::{TimePlugin, TimeUpdateStrategy};
        let mut app = App::new();
        app.add_plugins(TimePlugin);
        add_screenshot(&mut app, "unused.png".into(), None);
        let strategy = app.world().resource::<TimeUpdateStrategy>();
        assert!(matches!(strategy, TimeUpdateStrategy::ManualDuration(d) if *d == SHOT_FRAME));
        // However long a frame really takes, the fixed clock gets the same ticks: the first frame
        // starts the clock, the other 89 at 1/60 s each are 1.483 s, 74 ticks at 50 Hz (#942).
        app.insert_resource(Time::<Fixed>::from_hz(50.0));
        #[derive(Resource, Default)]
        struct Ticks(u32);
        app.init_resource::<Ticks>()
            .add_systems(FixedUpdate, |mut t: ResMut<Ticks>| t.0 += 1);
        for _ in 0..90 {
            app.update();
            std::thread::sleep(Duration::from_micros(200));
        }
        assert_eq!(app.world().resource::<Ticks>().0, 74);
    }

    #[test]
    fn a_capture_of_another_size_is_not_saved() {
        let want = Some(UVec2::new(800, 608));
        assert_eq!(shot_size_problem(UVec2::new(800, 608), want), None);
        let e = shot_size_problem(UVec2::new(7680, 2071), want).unwrap();
        assert!(e.contains("7680x2071") && e.contains("800x608"), "{e}");
        // Without a size asked for, any capture is saved.
        assert_eq!(shot_size_problem(UVec2::new(7680, 2071), None), None);
    }

    #[test]
    fn a_screenshot_window_is_held_at_its_size() {
        let mut app = App::new();
        app.insert_resource(ShotSize(UVec2::new(800, 608)))
            .add_systems(Update, hold_shot_size);
        // As a maximize on a 7680x2160 monitor at 125% leaves it (#861).
        let mut window = Window::default();
        window.resolution.set_physical_resolution(7680, 2071);
        let id = app.world_mut().spawn((window, bevy::window::PrimaryWindow)).id();
        app.update();
        let window = app.world().get::<Window>(id).unwrap();
        assert_eq!(window.physical_size(), UVec2::new(800, 608));
        assert!(!window.resizable && !window.enabled_buttons.maximize);
    }

    #[test]
    fn the_run_fails_when_the_screenshot_was_not_saved() {
        assert_eq!(shot_result(AppExit::Success), Ok(()));
        assert!(shot_result(AppExit::error()).is_err());
    }

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
        let normal = ClockSpeed::default();
        retune(&mut fixed, 50.0, normal);
        assert_eq!(fixed.timestep(), Duration::from_millis(20));
        // A room at 25 steps per second: 40 ms per tick.
        retune(&mut fixed, 25.0, normal);
        assert_eq!(fixed.timestep(), Duration::from_millis(40));
        // Back to the game's rate on the next room.
        retune(&mut fixed, 50.0, normal);
        assert_eq!(fixed.timestep(), Duration::from_millis(20));
        // A game that sets a silly speed gets the clamped one.
        retune(&mut fixed, 0.0, normal);
        assert_eq!(fixed.timestep(), Duration::from_secs(1));
    }

    #[test]
    fn slow_motion_stretches_every_tick() {
        let mut fixed = Time::<Fixed>::from_hz(50.0);
        retune(&mut fixed, 50.0, ClockSpeed(0.5));
        assert_eq!(fixed.timestep(), Duration::from_millis(40));
        retune(&mut fixed, 50.0, ClockSpeed(0.75));
        assert!((fixed.timestep().as_secs_f64().recip() - 37.5).abs() < 1e-6);
        // A room at 25 steps per second, at half speed: 80 ms per tick.
        retune(&mut fixed, 25.0, ClockSpeed(0.5));
        assert_eq!(fixed.timestep(), Duration::from_millis(80));
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

    /// A quick retry keeps the artifact prepared at run start instead of compiling the scripts
    /// again (#1093B). A rewrite that returned to `Sim::new_or_halted` would replace it.
    #[test]
    fn a_retry_keeps_the_prepared_scripts() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = test_world(&pkg);
        let before: *const Prepared = world.non_send::<Prepared>();
        // Step the run so the retry has something to reset.
        world
            .non_send_mut::<Game>()
            .sim
            .step(iwbtg_sim::input::InputFrame::EMPTY);
        restart_run(&mut world).unwrap();
        let after: *const Prepared = world.non_send::<Prepared>();
        assert!(
            std::ptr::eq(before, after),
            "the prepared artifact is kept, not rebuilt"
        );
        assert_eq!(world.non_send::<Game>().sim.world().tick, 0);
    }
}
