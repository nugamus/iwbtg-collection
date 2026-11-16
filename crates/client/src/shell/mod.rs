//! The app shell (docs/architecture/client/shell.md, #782): the client's screens as a Bevy state,
//! and the package loaded and unloaded at run time.
//!
//! - [`AppScreen::Select`] lists packages (a placeholder until the designed game select, #148);
//!   [`AppScreen::Run`] plays the [`LoadedPackage`].
//! - Everything that belongs to one run is built in `OnEnter(AppScreen::Run)` and gone after
//!   `OnExit(AppScreen::Run)`: resources through [`run_resource`], entities through
//!   [`RUN_ONLY`] (`DespawnOnExit`). The loading card, pause and results stay overlays inside the
//!   run.
//! - Run systems sit in [`InRun`], which runs only in `Run`; the whole fixed loop runs only in
//!   `Run` too, so `FixedUpdate` systems need no condition and the fixed clock gathers no time on
//!   the select screen.

use std::path::{Path, PathBuf};

use bevy::app::RunFixedMainLoopSystems;
use bevy::prelude::*;

use crate::package::{self, OpenError, Package};

pub mod select;

/// The screen the client shows.
#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum AppScreen {
    /// Pick a package (`--select`).
    #[default]
    Select,
    /// Play the [`LoadedPackage`].
    Run,
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
#[derive(clap::Args, Debug)]
pub struct ShellArgs {
    /// Start on a placeholder game select listing the packages in this folder (package folders
    /// with a game.toml, and `.iwbz` files). Quitting a game goes back to the list.
    #[arg(long, value_name = "FOLDER", conflicts_with = "package")]
    pub select: Option<PathBuf>,
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
/// [`LoadedPackage`] is already in the world (a package on the command line), else in `Select`.
pub struct ShellPlugin {
    pub library: Option<PathBuf>,
    pub trace_mode: bool,
}

impl Plugin for ShellPlugin {
    fn build(&self, app: &mut App) {
        let first = if app.world().contains_resource::<LoadedPackage>() {
            AppScreen::Run
        } else {
            AppScreen::Select
        };
        let in_run = in_state(AppScreen::Run);
        app.insert_state(first)
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

/// Opens the package at `path` and builds its sim. Nothing in `world` changes, so a package that
/// does not open, or a game that does not start, is an error before anything else happens.
/// `map` is the map id to play (`None`: the package's first map).
fn prepare(world: &World, path: &Path, trace_mode: bool, map: Option<&str>) -> Result<NextRun, StartError> {
    let fail = |why| StartError {
        path: path.to_owned(),
        why,
    };
    let pkg = package::open(path, trace_mode).map_err(|e| fail(StartFailure::Open(e)))?;
    let run = crate::view::new_run(world, &pkg, map).map_err(|e| fail(StartFailure::Run(e)))?;
    Ok(NextRun {
        loaded: LoadedPackage {
            pkg,
            path: path.to_owned(),
        },
        run,
    })
}

/// Opens the package at `path` (`trace_mode`: research converter output) and makes it the loaded
/// one, before the app runs (a package on the command line). The caller starts in `Run`.
pub fn load(world: &mut World, path: &Path, trace_mode: bool) -> Result<(), StartError> {
    prepare(world, path, trace_mode, None)?.install(world);
    Ok(())
}

/// Opens the package at `path` and plays it, from the select screen or in place of the run that
/// is playing (a dropped package, `import.rs`). A package that does not start is an error and
/// nothing changes. Otherwise the state goes to `Run` even when it is `Run` already: the run that
/// plays closes on `OnExit(Run)` (its replay and stats are finished there, its entities and
/// assets dropped), then [`open_next`] puts the new one in place on `OnEnter(Run)`.
pub fn start(world: &mut World, path: &Path) -> Result<(), StartError> {
    start_map(world, path, None)
}

/// [`start`] on the map with id `map` (game select's pick, #880); `None` is the package's first
/// map. A map the package does not have is a [`StartFailure::Run`] error.
pub fn start_map(world: &mut World, path: &Path, map: Option<&str>) -> Result<(), StartError> {
    let trace_mode = world.get_resource::<Shell>().is_some_and(|s| s.trace_mode);
    let next = prepare(world, path, trace_mode, map)?;
    world.insert_non_send(next);
    // `set`, not `set_if_different`: Run to Run closes the old run and opens the new one.
    world.resource_mut::<NextState<AppScreen>>().set(AppScreen::Run);
    Ok(())
}

/// Puts the run [`start`] built in place, first thing on `OnEnter(Run)`. A run from the command
/// line was put in place by [`load`] already.
fn open_next(world: &mut World) {
    if let Some(next) = world.remove_non_send::<NextRun>() {
        next.install(world);
    }
}

/// Ends the run: back to the select screen, or, when the client was started on one package,
/// exits (the replay and stats are finished either way: on `OnExit(Run)`, or on exit as when the
/// window closes).
pub fn leave(world: &mut World) {
    if world.get_resource::<Shell>().is_some_and(|s| s.library.is_some()) {
        world
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::Select);
    } else {
        world.write_message(AppExit::Success);
    }
}

#[cfg(test)]
mod tests;
