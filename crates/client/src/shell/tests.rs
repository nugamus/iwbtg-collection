//! The shell without a window: runs open and close and leave nothing behind.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::audio::AudioSource;
use bevy::ecs::resource::IsResource;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::window::FileDragAndDrop;
use iwbtg_sim::mods::ModSet;

use super::*;
use crate::view::{Game, GameCamera, Textures};

fn demo(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
}

/// The run modules that need no window or renderer, on a headless app started on the select
/// screen. Each update is one 20 ms frame, one tick at 50 Hz.
fn app() -> App {
    let mods = ModSet::parse("HB").unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
        .init_asset::<Image>()
        .init_asset::<AudioSource>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<crate::hud::HudLayout>()
        .init_resource::<crate::hud::Suspended>()
        .init_resource::<crate::hud::Paused>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(20)))
        .add_plugins((
            crate::view::ViewPlugin {
                ranked: false,
                fresh_seed: false,
                interpolate: true,
            },
            ShellPlugin {
                library: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
                trace_mode: false,
            },
            crate::bindings::BindingsPlugin(crate::bindings::Bindings::default()),
            crate::art::ArtPlugin,
            crate::audio::AudioPlugin::audible(),
            crate::mods::ModsPlugin(mods),
            crate::visual::VisualPlugin(mods),
            crate::practice::PracticePlugin {
                mods,
                slow_speed: 100,
            },
            crate::retry::RetryPlugin(crate::retry::RetryArgs::default()),
            crate::skip::SkipPlugin,
            crate::pause::PausePlugin {
                args: crate::pause::PauseArgs::default(),
            },
            crate::stats::StatsPlugin { keep: false, mods },
            crate::replays::ReplaysPlugin { keep: false, mods },
            crate::loading::LoadingPlugin {
                args: crate::loading::LoadingArgs::default(),
                skip: true,
            },
            crate::import::ImportPlugin { keep: false },
        ))
        // The window plugin adds it in the client.
        .add_message::<FileDragAndDrop>();
    app.finish();
    app.cleanup();
    app
}

/// Entities (not counting the ones Bevy keeps resources on), images and sounds in the world.
fn counts(app: &mut App) -> (usize, usize, usize) {
    let world = app.world_mut();
    let entities = world
        .query_filtered::<Entity, Without<IsResource>>()
        .iter(world)
        .count();
    let images = world.resource::<Assets<Image>>().len();
    let sounds = world.resource::<Assets<AudioSource>>().len();
    (entities, images, sounds)
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update();
    }
}

/// Plays `pkg` for two seconds and checks it is running, then leaves it.
fn play_and_leave(app: &mut App, pkg: &str) {
    start(app.world_mut(), &demo(pkg)).unwrap();
    frames(app, 100);
    let world = app.world();
    assert_eq!(*world.resource::<State<AppScreen>>().get(), AppScreen::Run);
    let tick = world.non_send::<Game>().sim.world().tick;
    assert!(tick > 50, "{pkg}: the sim ticked ({tick})");
    assert!(world.contains_resource::<Textures>());
    assert!(world.contains_resource::<LoadedPackage>());
    leave(app.world_mut());
    frames(app, 3);
    let world = app.world();
    assert_eq!(*world.resource::<State<AppScreen>>().get(), AppScreen::Select);
    assert!(world.get_non_send::<Game>().is_none());
    assert!(!world.contains_resource::<LoadedPackage>());
    assert!(!world.contains_resource::<Textures>());
    assert!(!world.contains_resource::<crate::art::ArtFiles>());
    assert!(!world.contains_resource::<crate::audio::Sounds>());
    assert!(!world.contains_resource::<crate::replays::Recorder>());
}

/// Package A, back to select, package B, back to select: every entity, image and sound the runs
/// made is gone, and the fixed clock did not run on the select screen.
#[test]
fn two_runs_leave_nothing_behind() {
    let mut app = app();
    frames(&mut app, 3);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select
    );
    let before = counts(&mut app);
    play_and_leave(&mut app, "tests/fixtures/audio.pkg");
    assert_eq!(counts(&mut app), before, "after A: (entities, images, sounds)");
    play_and_leave(&mut app, "tests/fixtures/clear.pkg");
    assert_eq!(counts(&mut app), before, "after B: (entities, images, sounds)");
    // The select screen gathers no fixed time: the next run starts from its first tick.
    frames(&mut app, 50);
    start(app.world_mut(), &demo("tests/fixtures/clear.pkg")).unwrap();
    frames(&mut app, 1);
    let tick = app.world().non_send::<Game>().sim.world().tick;
    assert!(tick <= 2, "the new run's first frame ran {tick} ticks");
}

/// A run made sounds, so the counts above checked something.
#[test]
fn a_run_holds_images_and_sounds() {
    let mut app = app();
    frames(&mut app, 3);
    let before = counts(&mut app);
    start(app.world_mut(), &demo("tests/fixtures/audio.pkg")).unwrap();
    frames(&mut app, 100);
    let during = counts(&mut app);
    assert!(during.0 > before.0, "entities {during:?} vs {before:?}");
    assert!(during.1 > before.1, "images {during:?} vs {before:?}");
    assert!(during.2 > before.2, "sounds {during:?} vs {before:?}");
}

/// A package that does not open is an error, and the shell stays where it was.
#[test]
fn a_broken_package_does_not_open() {
    let mut app = app();
    frames(&mut app, 2);
    let err = start(app.world_mut(), &demo("Cargo.toml")).unwrap_err();
    assert!(err.to_string().contains("Cargo.toml"), "{err}");
    frames(&mut app, 2);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select
    );
}

/// Drops `name` on the window, as the OS does.
fn drop_file(app: &mut App, name: &str) {
    app.world_mut().write_message(FileDragAndDrop::DroppedFile {
        window: Entity::PLACEHOLDER,
        path_buf: demo(name),
    });
}

fn package_id(app: &App) -> String {
    app.world().resource::<LoadedPackage>().pkg.def.id.clone()
}

fn tick(app: &App) -> u64 {
    app.world().non_send::<Game>().sim.world().tick as u64
}

/// A package dropped during a run replaces it in the same app (#841): the old run closes, the new
/// one starts from its first tick with one game camera, and leaving it leaves nothing of either.
#[test]
fn a_drop_during_a_run_plays_the_dropped_package() {
    let mut app = app();
    frames(&mut app, 3);
    let before = counts(&mut app);
    start(app.world_mut(), &demo("tests/fixtures/audio.pkg")).unwrap();
    frames(&mut app, 100);
    assert_eq!(package_id(&app), "demo_audio");
    assert!(tick(&app) > 50);
    drop_file(&mut app, "tests/fixtures/clear.pkg");
    frames(&mut app, 3);
    assert_eq!(*app.world().resource::<State<AppScreen>>().get(), AppScreen::Run);
    assert_eq!(package_id(&app), "demo_clear", "the dropped package plays");
    assert!(tick(&app) <= 4, "a new sim, not the old one ({})", tick(&app));
    let world = app.world_mut();
    let cameras = world.query_filtered::<(), With<GameCamera>>().iter(world).count();
    assert_eq!(cameras, 1, "the old run's camera is gone, the new one is there");
    leave(app.world_mut());
    frames(&mut app, 3);
    assert_eq!(counts(&mut app), before, "after both: (entities, images, sounds)");
}

/// A drop that is not a package, or does not open, changes nothing: the run goes on.
#[test]
fn a_broken_drop_during_a_run_changes_nothing() {
    let mut app = app();
    frames(&mut app, 3);
    start(app.world_mut(), &demo("tests/fixtures/audio.pkg")).unwrap();
    frames(&mut app, 50);
    let ticked = tick(&app);
    drop_file(&mut app, "Cargo.toml");
    drop_file(&mut app, "tests/fixtures");
    frames(&mut app, 3);
    assert_eq!(*app.world().resource::<State<AppScreen>>().get(), AppScreen::Run);
    assert_eq!(package_id(&app), "demo_audio");
    assert!(tick(&app) > ticked, "the same sim ticks on");
}

/// A drop on the select screen plays the package.
#[test]
fn a_drop_on_the_select_screen_plays_it() {
    let mut app = app();
    frames(&mut app, 3);
    drop_file(&mut app, "tests/fixtures/clear.pkg");
    frames(&mut app, 3);
    assert_eq!(*app.world().resource::<State<AppScreen>>().get(), AppScreen::Run);
    assert_eq!(package_id(&app), "demo_clear");
}
