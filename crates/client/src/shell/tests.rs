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
/// screen. Each update is one 20 ms frame, one tick at 50 Hz. Runs skip the loading card.
fn app() -> App {
    app_with(true)
}

/// [`app`]; `skip_loading: false` opens every run on the loading card, as the client does.
fn app_with(skip_loading: bool) -> App {
    let mods = ModSet::parse("HB").unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
        .init_asset::<Image>()
        .init_asset::<AudioSource>()
        // The results' heatmap draws the room through it (the UI plugin adds it in the client).
        .init_asset::<crate::ui::filter::FilterMaterial>()
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
            crate::visual::VisualPlugin,
            crate::practice::PracticePlugin { slow_speed: 75 },
            crate::retry::RetryPlugin(crate::retry::RetryArgs::default()),
            crate::skip::SkipPlugin,
            crate::pause::PausePlugin {
                args: crate::pause::PauseArgs::default(),
            },
            crate::stats::StatsPlugin { keep: false },
            crate::replays::ReplaysPlugin { keep: false },
            crate::loading::LoadingPlugin {
                args: crate::loading::LoadingArgs::default(),
                skip: skip_loading,
            },
            crate::import::ImportPlugin { keep: false },
        ))
        // Bevy takes at most 15 plugins in one tuple.
        .add_plugins(crate::results::ResultsPlugin(crate::results::ResultsArgs::default()))
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
    app.world().non_send::<Game>().sim.world().tick
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

fn map_id(app: &App) -> String {
    app.world().non_send::<Game>().sim.map_id().to_owned()
}

/// Starting a map by id plays that map (#880); no id is the first map.
#[test]
fn start_map_plays_the_picked_map() {
    let mut app = app();
    frames(&mut app, 3);
    let pkg = demo("tests/fixtures/maps.pkg");
    start_map(app.world_mut(), &pkg, Some("hard")).unwrap();
    frames(&mut app, 3);
    assert_eq!(map_id(&app), "hard");
    leave(app.world_mut());
    frames(&mut app, 3);
    start(app.world_mut(), &pkg).unwrap();
    frames(&mut app, 3);
    assert_eq!(map_id(&app), "medium", "no id: the first map");
}

/// A map the package does not have is an error and nothing changes.
#[test]
fn start_map_refuses_an_unknown_map() {
    let mut app = app();
    frames(&mut app, 3);
    let err = start_map(app.world_mut(), &demo("tests/fixtures/maps.pkg"), Some("nope")).unwrap_err();
    assert!(err.to_string().contains("nope"), "{err}");
    frames(&mut app, 2);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select
    );
}

fn screen(app: &App) -> RunScreen {
    *app.world().resource::<State<RunScreen>>().get()
}

/// Whether the game shows: its camera is on and the HUD is not suspended.
fn game_shows(app: &mut App) -> bool {
    let world = app.world_mut();
    let on = world
        .query_filtered::<&Camera, With<GameCamera>>()
        .iter(world)
        .all(|c| c.is_active);
    on && !world.resource::<crate::hud::Suspended>().0
}

fn press(app: &mut App, key: KeyCode) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.press(key);
    app.update();
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.release(key);
    input.clear();
}

/// A run walks its screens (#839): the loading card (no ticks, the game hidden), the game
/// (ticking), the pause menu (no ticks, the game shown), the game again, the results (no ticks,
/// the game hidden); each change takes one frame.
#[test]
fn a_run_walks_loading_playing_paused_playing_results() {
    let mut app = app_with(false);
    frames(&mut app, 3);
    start(app.world_mut(), &demo("tests/fixtures/clear.pkg")).unwrap();
    frames(&mut app, 10);
    assert_eq!(screen(&app), RunScreen::Loading);
    assert_eq!(tick(&app), 0, "no tick on the loading card");
    assert!(!game_shows(&mut app), "the card covers the game");

    // The card stays its minimum time (1.5 s), then the game plays.
    frames(&mut app, 70);
    assert_eq!(screen(&app), RunScreen::Playing);
    assert!(game_shows(&mut app));
    let at = tick(&app);
    frames(&mut app, 10);
    assert!(tick(&app) > at, "the game ticks");

    press(&mut app, crate::pause::PAUSE_KEY);
    frames(&mut app, 1);
    assert_eq!(screen(&app), RunScreen::Paused);
    assert!(app.world().resource::<crate::hud::Paused>().open);
    assert!(game_shows(&mut app), "the menu shows over the game");
    let at = tick(&app);
    frames(&mut app, 10);
    assert_eq!(tick(&app), at, "no tick in the pause menu");

    press(&mut app, crate::pause::PAUSE_KEY);
    frames(&mut app, 1);
    assert_eq!(screen(&app), RunScreen::Playing);
    assert!(!app.world().resource::<crate::hud::Paused>().open);
    frames(&mut app, 10);
    assert!(tick(&app) > at, "the game ticks again");

    crate::results::preview_next(app.world_mut(), crate::results::Scenario::Pb);
    frames(&mut app, 2);
    assert_eq!(screen(&app), RunScreen::Results);
    assert!(!game_shows(&mut app), "the results cover the game");
    let at = tick(&app);
    frames(&mut app, 10);
    assert_eq!(tick(&app), at, "no tick on the results");
    press(&mut app, crate::pause::PAUSE_KEY);
    frames(&mut app, 1);
    assert_eq!(screen(&app), RunScreen::Playing, "Escape goes back to the game");
    assert!(game_shows(&mut app));

    // A run that replaces this one opens on the card again.
    start(app.world_mut(), &demo("tests/fixtures/clear.pkg")).unwrap();
    frames(&mut app, 2);
    assert_eq!(screen(&app), RunScreen::Loading);
    leave(app.world_mut());
    frames(&mut app, 2);
    assert!(app.world().get_resource::<State<RunScreen>>().is_none(), "no run, no run screen");
}

/// With the card skipped (a `--screenshot` check of another screen) a run opens on the game.
#[test]
fn a_skipped_card_opens_on_the_game() {
    let mut app = app();
    frames(&mut app, 3);
    start(app.world_mut(), &demo("tests/fixtures/clear.pkg")).unwrap();
    frames(&mut app, 3);
    assert_eq!(screen(&app), RunScreen::Playing);
    assert!(tick(&app) > 0);
}

/// The mod select's picks play (#903): the app was built with `--mods HB`, the sheet then picked
/// Sudden Death and Slow motion at 50%. The run plays at half speed without hitboxes, its replay
/// header and stats board are `SD+SL`, and the first death ends it.
#[test]
fn a_run_plays_with_the_mods_picked_on_the_select_screen() {
    use crate::mods::RunMods;
    use crate::practice::SlowSpeed;
    use crate::view::ClockSpeed;

    let mut app = app();
    frames(&mut app, 3);
    let picked = ModSet::parse("SD+SL").unwrap();
    // What the sheet's cards and its 50% option write.
    app.world_mut().insert_resource(RunMods(picked));
    app.world_mut().insert_resource(SlowSpeed(50));
    start(app.world_mut(), &demo("tests/fixtures/input.pkg")).unwrap();
    frames(&mut app, 20);
    let world = app.world();
    assert_eq!(*world.resource::<ClockSpeed>(), ClockSpeed(0.5));
    assert_eq!(
        world.resource::<Time<Fixed>>().timestep(),
        Duration::from_millis(40),
        "50 Hz at 50%"
    );
    let ticked = tick(&app);
    assert!(
        (8..=12).contains(&ticked),
        "20 frames of 20 ms at 25 Hz ran {ticked} ticks"
    );
    assert!(
        !app.world().contains_resource::<crate::visual::Hitboxes>(),
        "the launch mods' Hitboxes are not picked"
    );
    assert_eq!(app.world().resource::<crate::replays::Recorder>().mods(), picked);
    assert_eq!(
        app.world().resource::<crate::stats::Stats>().board(),
        Some("original/SD+SL")
    );
    // Hold Suicide (Q) until the death counter moves: Sudden Death stops the sim there.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyQ);
    let mut n = 0;
    while app.world().non_send::<Game>().sim.world().engine.run.deaths == 0 {
        assert!(n < 400, "no death in 400 frames");
        frames(&mut app, 1);
        n += 1;
    }
    let over = tick(&app);
    frames(&mut app, 20);
    assert_eq!(tick(&app), over, "the run is over at the death");
    leave(app.world_mut());
    frames(&mut app, 3);
    // The next run picks No background instead: a fresh run with its own mods, at full speed.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::KeyQ);
    app.world_mut()
        .insert_resource(RunMods(ModSet::parse("NB").unwrap()));
    start(app.world_mut(), &demo("tests/fixtures/input.pkg")).unwrap();
    frames(&mut app, 20);
    let world = app.world();
    assert_eq!(*world.resource::<ClockSpeed>(), ClockSpeed(1.0));
    assert!(world.contains_resource::<crate::visual::PlainBackdrop>());
    assert!(tick(&app) >= 18, "full speed ({})", tick(&app));
    leave(app.world_mut());
    frames(&mut app, 3);
    assert!(!app.world().contains_resource::<crate::visual::PlainBackdrop>());
}
