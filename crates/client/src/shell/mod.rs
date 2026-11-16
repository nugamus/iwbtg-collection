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

use crate::package::{self, Package};

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
struct Shell {
    /// `--select`: the folder the list shows. `None`: the client plays one package and leaving it
    /// exits.
    library: Option<PathBuf>,
    /// `--trace-mode`, for every package opened.
    trace_mode: bool,
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

/// Opens the package at `path` (`trace_mode`: research converter output) and makes it the loaded
/// one. The run's sim is built here, so a game that does not start is an error before anything
/// changes. The caller switches to `Run`.
pub fn load(world: &mut World, path: &Path, trace_mode: bool) -> Result<(), String> {
    let pkg = package::open(path, trace_mode).map_err(|e| format!("{}: {e}", path.display()))?;
    crate::view::new_run(world, &pkg)?;
    world.insert_resource(LoadedPackage {
        pkg,
        path: path.to_owned(),
    });
    Ok(())
}

/// Opens the package at `path` and plays it.
pub fn start(world: &mut World, path: &Path) -> Result<(), String> {
    let trace_mode = world.get_resource::<Shell>().is_some_and(|s| s.trace_mode);
    load(world, path, trace_mode)?;
    world
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Run);
    Ok(())
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
