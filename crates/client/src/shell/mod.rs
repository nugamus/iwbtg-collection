//! The app shell (docs/architecture/client/shell.md, #782): the client's screens as a Bevy state,
//! and the package loaded and unloaded at run time.
//!
//! - [`AppScreen::Select`] lists packages (a placeholder until the designed game select, #148);
//!   [`AppScreen::Run`] plays the [`LoadedPackage`].
//! - Everything that belongs to one run is built in `OnEnter(AppScreen::Run)` and gone after
//!   `OnExit(AppScreen::Run)`: resources through [`run_resource`], entities through
//!   [`RUN_ONLY`] (`DespawnOnExit`).
//! - Inside a run, [`RunScreen`] (a sub-state of `Run`) says what shows: the loading card, the
//!   game, the pause menu, the results or the script error screen (#839, #101). The sim ticks only in [`RunScreen::Playing`]
//!   (`SimTick` is gated here; Sudden Death, retry and frame advance add their own gates).
//! - Run systems sit in [`InRun`], which runs only in `Run`; the whole fixed loop runs only in
//!   `Run` too, so `FixedUpdate` systems need no condition and the fixed clock gathers no time on
//!   the select screen.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use bevy::app::RunFixedMainLoopSystems;
use bevy::prelude::*;
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::stripped::PlayMode;

use crate::package::{self, OpenError, Package, SpriteImage};

pub mod menu;
pub mod select;
pub mod shelf;

/// The screen the client shows.
#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum AppScreen {
    /// Pick a package (`--select`): Time Attack's game select.
    #[default]
    Select,
    /// The main menu (`menu.rs`): where a client started on a library opens, and where Back
    /// leads from game select.
    Menu,
    /// Classic's shelf (`shelf.rs`): one cover per game (m5 A4).
    Shelf,
    /// Play the [`LoadedPackage`].
    Run,
}

/// What a run shows (#839): a sub-state of [`AppScreen::Run`], gone outside it. Each run opens on
/// [`FirstScreen`]'s screen (the loading card, unless a `--screenshot` check skips it). The screens
/// do their covering in `OnEnter`/`OnExit`: the loading card and the results hide the game camera
/// and the HUD (`hud::Suspended`) and set the backdrop's dim; the pause menu holds the audio.
#[derive(SubStates, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[source(AppScreen = AppScreen::Run)]
pub enum RunScreen {
    /// The loading card (`loading/`).
    #[default]
    Loading,
    /// The game plays: the only screen the sim ticks in.
    Playing,
    /// The pause menu (`pause.rs`).
    Paused,
    /// The results page (`results/`).
    Results,
    /// The sim halted (a script error): the script error screen (`script_error/`, #101).
    Error,
}

/// The screen every run opens on: [`RunScreen::Loading`], or [`RunScreen::Playing`] when the
/// loading plugin skips the card (a `--screenshot` check of another screen).
#[derive(Resource, Clone, Copy, Debug)]
pub struct FirstScreen(pub RunScreen);

impl Default for FirstScreen {
    fn default() -> Self {
        FirstScreen(RunScreen::Loading)
    }
}

/// Asks for `to` on the next state transition (a system's `NextState<RunScreen>`, from a world).
pub fn go(world: &mut World, to: RunScreen) {
    if let Some(mut next) = world.get_resource_mut::<NextState<RunScreen>>() {
        next.set(to);
    }
}

/// The run's screen, `None` outside a run.
pub fn run_screen(world: &World) -> Option<RunScreen> {
    world.get_resource::<State<RunScreen>>().map(|s| *s.get())
}

/// Sets the sub-state a new run opens on: the opening transition takes a pending
/// `NextState<RunScreen>` over the default, and it must be set for a run that replaces a run (the
/// sub-state would otherwise stay what it was).
fn open_on_start(world: &mut World) {
    let first = world.get_resource::<FirstScreen>().copied().unwrap_or_default().0;
    go(world, first);
}

/// What a full-window screen (the loading card, the results) does on entering (`covered`) and on
/// leaving: the game camera, the HUD's suspend and, on entering, the backdrop's dim.
pub(crate) fn cover_game(
    covered: bool,
    backdrop_dim: Option<f32>,
    game_cameras: &mut Query<&mut Camera, With<crate::view::GameCamera>>,
    suspended: Option<ResMut<crate::hud::Suspended>>,
    backdrop: Option<ResMut<crate::ui::backdrop::Backdrop>>,
) {
    for mut camera in game_cameras.iter_mut() {
        camera.is_active = !covered;
    }
    if let Some(mut s) = suspended {
        s.0 = covered;
    }
    if let (Some(mut b), Some(d)) = (backdrop, backdrop_dim) {
        b.dim = d;
    }
}

/// The run's systems: they run only in [`AppScreen::Run`] (configured in `Update`, `PostUpdate`
/// and `Last`). In `OnEnter(Run)` the set runs after the run's resources are in place, in
/// `OnExit(Run)` before they are removed.
#[derive(SystemSet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct InRun;

/// The steps of opening and closing a run, around [`InRun`]: on enter `Core` (the view: sim,
/// clock, room size), `Resources` (each module's [`run_resource`]), then `InRun`; on exit the
/// other way round.
#[derive(SystemSet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RunSteps {
    Core,
    Resources,
}

/// An entity that belongs to the run: despawned when the run closes.
pub const RUN_ONLY: DespawnOnExit<AppScreen> = DespawnOnExit(AppScreen::Run);

/// The package being played, and where it came from.
#[derive(Resource)]
pub struct LoadedPackage {
    pub pkg: Package,
    /// The folder or `.iwbz` file it was opened from.
    pub path: PathBuf,
}

/// The shell's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Default)]
pub struct ShellArgs {
    /// Start on a placeholder game select listing the packages in this folder (package folders
    /// with a game.toml, and `.iwbz` files). Quitting a game goes back to the list.
    #[arg(long, value_name = "FOLDER", conflicts_with = "package")]
    pub select: Option<PathBuf>,
    /// Screenshot checks: open game select's mod select sheet at start (#497).
    #[arg(long, hide = true, requires = "select")]
    pub mod_select_preview: bool,
    /// Screenshot checks: open game select's practice panel at start (#39).
    #[arg(long, hide = true, requires = "select")]
    pub practice_preview: bool,
    /// Screenshot checks: open the main menu in this state (m5 B6, B7, A1, A2).
    #[arg(long, hide = true, value_enum, requires = "select")]
    pub menu_preview: Option<menu::MenuPreview>,
    /// Screenshot checks: open this mode's screen instead of the main menu: game select for
    /// Time Attack (m5 A3), the shelf for Classic (m5 A4).
    #[arg(long, hide = true, value_enum, requires = "select", conflicts_with = "menu_preview")]
    pub select_preview: Option<SelectPreview>,
}

/// `--select-preview`: the mode game select opens in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum SelectPreview {
    TimeAttack,
    Classic,
}

/// The shell's place in the module list (main.rs): its flags and both its plugins.
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = ShellArgs;

    fn add(
        app: &mut bevy::prelude::App,
        args: ShellArgs,
        startup: &crate::registration::Startup,
    ) {
        let library = args.select.clone();
        // A client on a library opens on the main menu, unless a check opens game select.
        let on_select = args.mod_select_preview
            || args.practice_preview
            || args.select_preview.is_some();
        let classic = args.select_preview == Some(SelectPreview::Classic);
        if classic {
            app.insert_resource(crate::ui::mode::Mode::Classic);
        }
        let start = if classic {
            AppScreen::Shelf
        } else if on_select {
            AppScreen::Select
        } else {
            AppScreen::Menu
        };
        app.add_plugins((
            ShellPlugin {
                library: args.select.clone(),
                trace_mode: startup.trace_mode,
                start,
            },
            shelf::ShelfPlugin {
                library: args.select,
            },
            select::SelectPlugin(library, args.mod_select_preview, args.practice_preview),
            menu::MenuPlugin {
                preview: args.menu_preview,
            },
        ));
    }
}

/// What the shell was started with.
#[derive(Resource)]
pub(crate) struct Shell {
    /// `--select`: the folder the list shows. `None`: the client plays one package and leaving it
    /// exits.
    library: Option<PathBuf>,
    /// `--trace-mode`, for every package opened.
    trace_mode: bool,
}

impl Shell {
    /// Leaving a run goes back to the select screen (else it exits).
    pub(crate) fn goes_back(&self) -> bool {
        self.library.is_some()
    }
}

/// Adds the state, the sets and the select screen's list. Starts in `Run` when a
/// [`LoadedPackage`] is already in the world (a package on the command line), else on the main
/// menu (`menu`) or in `Select`.
pub struct ShellPlugin {
    pub library: Option<PathBuf>,
    pub trace_mode: bool,
    /// The screen it opens on without a package on the command line (the main menu; game select
    /// or the shelf for a check).
    pub start: AppScreen,
}

impl Plugin for ShellPlugin {
    fn build(&self, app: &mut App) {
        let first = if app.world().contains_resource::<LoadedPackage>() {
            AppScreen::Run
        } else {
            self.start
        };
        let in_run = in_state(AppScreen::Run);
        app.insert_state(first)
            .add_sub_state::<RunScreen>()
            .init_resource::<FirstScreen>()
            .add_message::<StartFailed>()
            .add_systems(Update, (poll_start, mode_screen))
            .insert_resource(Shell {
                library: self.library.clone(),
                trace_mode: self.trace_mode,
            })
            .configure_sets(Update, InRun.run_if(in_run.clone()))
            .configure_sets(PostUpdate, InRun.run_if(in_run.clone()))
            .configure_sets(Last, InRun.run_if(in_run.clone()))
            .configure_sets(
                RunFixedMainLoop,
                RunFixedMainLoopSystems::FixedMainLoop.run_if(in_run),
            )
            .configure_sets(
                FixedUpdate,
                crate::view::SimTick.run_if(in_state(RunScreen::Playing)),
            )
            .configure_sets(
                OnEnter(AppScreen::Run),
                (RunSteps::Core, RunSteps::Resources, InRun).chain(),
            )
            .configure_sets(
                OnExit(AppScreen::Run),
                (InRun, RunSteps::Resources, RunSteps::Core).chain(),
            )
            .add_systems(OnEnter(AppScreen::Run), open_next.before(RunSteps::Core))
            .add_systems(
                OnExit(AppScreen::Run),
                (|world: &mut World| {
                    world.remove_resource::<LoadedPackage>();
                })
                .after(RunSteps::Core),
            );
    }

    /// A run on the command line opens with the app: its first screen is set once every plugin
    /// has said what it is ([`FirstScreen`]).
    fn finish(&self, app: &mut App) {
        if *app.world().resource::<State<AppScreen>>().get() == AppScreen::Run {
            open_on_start(app.world_mut());
        }
    }
}

/// Inserts `make(world)` when a run opens (after the view's core: the sim, the room size and the
/// [`LoadedPackage`] are there) and removes it when the run closes (after the run's own exit
/// systems). Dropping it drops the handles it holds, so its images and sounds unload.
pub fn run_resource<R: Resource>(app: &mut App, make: impl Fn(&mut World) -> R + Send + Sync + 'static) {
    app.add_systems(
        OnEnter(AppScreen::Run),
        (move |world: &mut World| {
            let r = make(world);
            world.insert_resource(r);
        })
        .in_set(RunSteps::Resources),
    )
    .add_systems(
        OnExit(AppScreen::Run),
        (|world: &mut World| {
            world.remove_resource::<R>();
        })
        .in_set(RunSteps::Resources),
    );
}

/// Sets `R` back to its default when a run opens, for run state that app-level systems may still
/// read between runs (it holds no handles).
pub fn reset_on_enter<R: Resource + Default>(app: &mut App) {
    app.add_systems(
        OnEnter(AppScreen::Run),
        (|world: &mut World| world.insert_resource(R::default())).in_set(RunSteps::Resources),
    );
}

/// Why a package did not start.
#[derive(Debug)]
pub struct StartError {
    /// The folder or `.iwbz` file.
    pub path: PathBuf,
    pub why: StartFailure,
}

#[derive(Debug)]
pub enum StartFailure {
    /// It did not open (`package::open`).
    Open(OpenError),
    /// It opened, but its game did not start.
    Run(String),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let why: &dyn std::fmt::Display = match &self.why {
            StartFailure::Open(e) => e,
            StartFailure::Run(e) => e,
        };
        write!(f, "{}: {why}", self.path.display())
    }
}

/// A run built and not yet playing: the package and its sim. [`start`] leaves it in the world as
/// a non-send resource; [`open_next`] puts it in place when the run opens.
struct NextRun {
    loaded: LoadedPackage,
    run: crate::view::NewRun,
}

impl NextRun {
    fn install(self, world: &mut World) {
        self.run.install(world);
        world.insert_resource(self.loaded);
    }
}

/// The package work a staged start does off the UI thread: opening/parsing and PNG decode are
/// `Send`; the sim and its Rhai ASTs are not, so [`finish`] stays on the main thread.
struct Staged {
    pkg: Package,
    sprites: Vec<Option<SpriteImage>>,
}

/// A worker's answer: its request generation and the staged package or why it failed.
type StageResult = (u64, Result<Staged, StartError>);

/// A start requested but not committed yet. The worker's result arrives on `rx`; a newer request
/// replaces the resource and its `generation`, so a stale worker's result is ignored.
#[derive(Resource)]
pub(crate) struct PendingStart {
    generation: u64,
    path: PathBuf,
    map: Option<String>,
    /// The run's play mode (game select's two buttons, #43).
    mode: PlayMode,
    /// An explicit start point (game select's practice pickers, #39).
    start: Option<StartPoint>,
    /// A `Receiver` is `Send` but not `Sync`, so it sits behind a mutex to be a resource; the
    /// poll takes the lock for one `try_recv` and never holds it while building the sim.
    rx: Mutex<Receiver<StageResult>>,
}

impl PendingStart {
    /// What the select screen says while the package prepares.
    pub(crate) fn opening(&self) -> String {
        let name = self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        format!("Opening {name}…")
    }
}

/// A staged start that did not commit: the package did not open, or its game did not start. The
/// screen that asked shows it; the run on screen, if any, is unchanged.
#[derive(Message, Debug)]
pub struct StartFailed(pub StartError);

/// The id of the newest [`PendingStart`].
static NEXT_START: AtomicU64 = AtomicU64::new(0);

/// Opens the package at `path` (`trace_mode`: research converter output) and decodes its sprites:
/// the work a staged start does off the UI thread. Nothing in `world` changes.
fn stage(path: &Path, trace_mode: bool) -> Result<Staged, StartError> {
    let fail = |why| StartError {
        path: path.to_owned(),
        why,
    };
    let pkg = package::open(path, trace_mode).map_err(|e| fail(StartFailure::Open(e)))?;
    let sprites = package::sprite_images(&pkg.files);
    Ok(Staged { pkg, sprites })
}

/// Builds the sim for a staged package and pairs it with the loaded package. Nothing in `world`
/// changes, so a package that does not open, or a game that does not start, is an error before
/// anything else happens. `map` is the map id to play (`None`: the package's first map).
fn finish(
    world: &World,
    staged: Staged,
    path: &Path,
    map: Option<&str>,
    mode: Option<PlayMode>,
    start: Option<StartPoint>,
) -> Result<NextRun, StartError> {
    let run = crate::view::new_run(world, &staged.pkg, map, mode, start, staged.sprites)
        .map_err(|why| StartError {
            path: path.to_owned(),
            why: StartFailure::Run(why),
        })?;
    Ok(NextRun {
        loaded: LoadedPackage {
            pkg: staged.pkg,
            path: path.to_owned(),
        },
        run,
    })
}

/// [`stage`] then [`finish`], on the caller's thread: the command-line start and a dropped
/// package (a drop during play must not change when the old run stops).
fn prepare(
    world: &World,
    path: &Path,
    trace_mode: bool,
    map: Option<&str>,
    mode: Option<PlayMode>,
    start: Option<StartPoint>,
) -> Result<NextRun, StartError> {
    let staged = stage(path, trace_mode)?;
    finish(world, staged, path, map, mode, start)
}

/// Opens the package at `path` (`trace_mode`: research converter output) and makes it the loaded
/// one, before the app runs (a package on the command line). The caller starts in `Run`.
pub fn load(world: &mut World, path: &Path, trace_mode: bool) -> Result<(), StartError> {
    prepare(world, path, trace_mode, None, None, None)?.install(world);
    Ok(())
}

/// Makes a package already in memory the loaded one, instead of opening a path ([`load`]): the
/// browser fetches it (docs/architecture/client/web.md). `source` names it in messages, its URL.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub fn load_package(
    world: &mut World,
    pkg: package::Package,
    source: PathBuf,
) -> Result<(), StartError> {
    let clock = crate::startup_profile::start();
    let sprites = package::sprite_images(&pkg.files);
    crate::startup_profile::mark("sprite decode", clock);
    let run = crate::view::new_run(world, &pkg, None, None, None, sprites).map_err(|why| StartError {
        path: source.clone(),
        why: StartFailure::Run(why),
    })?;
    NextRun {
        loaded: LoadedPackage { pkg, path: source },
        run,
    }
    .install(world);
    Ok(())
}

/// Opens the package at `path` and plays it, in place of the run that is playing (a dropped
/// package, `import.rs`). A package that does not start is an error and nothing changes.
/// Otherwise the state goes to `Run` even when it is `Run` already: the run that plays closes on
/// `OnExit(Run)` (its replay and stats are finished there, its entities and assets dropped), then
/// [`open_next`] puts the new one in place on `OnEnter(Run)`.
///
/// This is synchronous so a drop reports its errors to the drop handler. Game select's start is
/// staged: [`start_map`].
pub fn start(world: &mut World, path: &Path) -> Result<(), StartError> {
    let trace_mode = world.get_resource::<Shell>().is_some_and(|s| s.trace_mode);
    let next = prepare(world, path, trace_mode, None, None, None)?;
    install(next, world);
    Ok(())
}

/// Stages the package at `path` on the map with id `map` (game select's pick, #880; `None` is the
/// package's first map) in the play `mode` (game select's two buttons, #43) and with an
/// explicit `start` (game select's practice pickers, #39), and returns at once: the select
/// screen keeps drawing `Opening <name>…` while [`stage`] runs on a worker thread. [`poll_start`]
/// builds the sim and commits the run when the worker is done; a newer call replaces this one, so
/// its result is ignored. A failure arrives as [`StartFailed`] and the run on screen is unchanged.
pub fn start_map(
    world: &mut World,
    path: &Path,
    map: Option<&str>,
    mode: PlayMode,
    start: Option<StartPoint>,
) {
    let trace_mode = world.get_resource::<Shell>().is_some_and(|s| s.trace_mode);
    let generation = NEXT_START.fetch_add(1, Ordering::Relaxed) + 1;
    let rx = spawn_stage(path.to_owned(), trace_mode, generation);
    world.insert_resource(PendingStart {
        generation,
        path: path.to_owned(),
        map: map.map(str::to_owned),
        mode,
        start,
        rx: Mutex::new(rx),
    });
}

/// Starts the worker that runs [`stage`], or, in the browser build (no threads), runs it at once
/// and delivers the result on the same channel, so the staged path is the same code everywhere.
fn spawn_stage(path: PathBuf, trace_mode: bool, generation: u64) -> Receiver<StageResult> {
    let (tx, rx) = mpsc::channel();
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        let fallback = tx.clone();
        let worker_path = path.clone();
        let sent = std::thread::Builder::new()
            .name("iwbtg-start".to_owned())
            .spawn(move || {
                let _ = tx.send((generation, stage(&worker_path, trace_mode)));
            });
        if sent.is_err() {
            // No thread available: stage here rather than drop the request.
            let _ = fallback.send((generation, stage(&path, trace_mode)));
        }
    }
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        let _ = tx.send((generation, stage(&path, trace_mode)));
    }
    rx
}

/// Commits a finished [`PendingStart`]: builds the sim on this thread (it is not `Send`), then
/// puts it in `NextRun` and asks for `Run` in one step. The run on screen keeps playing until
/// then; a failure leaves it and the state untouched.
fn poll_start(world: &mut World) {
    let Some(pending) = world.get_resource::<PendingStart>() else {
        return;
    };
    let received = pending
        .rx
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .try_recv();
    let result = match received {
        Ok((generation, result)) if generation == pending.generation => result,
        Ok(_) => return,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => {
            let path = pending.path.clone();
            world.remove_resource::<PendingStart>();
            world.write_message(StartFailed(StartError {
                path,
                why: StartFailure::Run("the package preparation stopped".to_owned()),
            }));
            return;
        }
    };
    let path = pending.path.clone();
    let map = pending.map.clone();
    let mode = pending.mode;
    let start = pending.start.clone();
    world.remove_resource::<PendingStart>();
    match result {
        Err(e) => {
            world.write_message(StartFailed(e));
        }
        Ok(staged) => match finish(world, staged, &path, map.as_deref(), Some(mode), start) {
            Ok(next) => {
                world.insert_non_send(next);
                // `set`, not `set_if_different`: Run to Run closes the old run and opens the new one.
                world.resource_mut::<NextState<AppScreen>>().set(AppScreen::Run);
                open_on_start(world);
            }
            Err(e) => {
                world.write_message(StartFailed(e));
            }
        },
    }
}

/// Puts a built run in place: `NextRun` first, then the state change (`set` also for Run to Run).
fn install(next: NextRun, world: &mut World) {
    world.insert_non_send(next);
    world.resource_mut::<NextState<AppScreen>>().set(AppScreen::Run);
    open_on_start(world);
}

/// Puts the run [`start`] built in place, first thing on `OnEnter(Run)`. A run from the command
/// line was put in place by [`load`] already.
fn open_next(world: &mut World) {
    if let Some(next) = world.remove_non_send::<NextRun>() {
        next.install(world);
    }
}

/// The screen of `mode`: game select for Time Attack, the shelf for Classic (m5 A1).
pub fn mode_home(mode: Option<crate::ui::mode::Mode>) -> AppScreen {
    match mode.unwrap_or_default() {
        crate::ui::mode::Mode::TimeAttack => AppScreen::Select,
        crate::ui::mode::Mode::Classic => AppScreen::Shelf,
    }
}

/// The toolbar's mode switch on a mode's screen opens the other mode's screen.
fn mode_screen(
    mode: Option<Res<crate::ui::mode::Mode>>,
    screen: Res<State<AppScreen>>,
    mut next: ResMut<NextState<AppScreen>>,
) {
    let Some(mode) = mode else {
        return;
    };
    if !mode.is_changed() || mode.is_added() {
        return;
    }
    let here = *screen.get();
    if matches!(here, AppScreen::Select | AppScreen::Shelf) {
        let want = mode_home(Some(*mode));
        if want != here {
            next.set(want);
        }
    }
}

/// Ends the run: back to the select screen, or, when the client was started on one package,
/// exits (the replay and stats are finished either way: on `OnExit(Run)`, or on exit as when the
/// window closes).
pub fn leave(world: &mut World) {
    if world.get_resource::<Shell>().is_some_and(|s| s.library.is_some()) {
        let to = mode_home(world.get_resource::<crate::ui::mode::Mode>().copied());
        world.resource_mut::<NextState<AppScreen>>().set(to);
    } else {
        world.write_message(AppExit::Success);
    }
}

#[cfg(test)]
mod tests;
