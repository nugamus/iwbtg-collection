//! The shell without a window: runs open and close and leave nothing behind.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use bevy::audio::AudioSource;
use bevy::ecs::resource::IsResource;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::window::FileDragAndDrop;
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::stripped::PlayMode;

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
    app_on(skip_loading, None)
}

/// [`app_with`]; `first: Some(pkg)` starts the client on that package, as a package on the
/// command line does (`view::run`: `load` before the shell plugin, no game select to go back to).
fn app_on(skip_loading: bool, first: Option<&str>) -> App {
    let mods = ModSet::parse("HB").unwrap();
    let mut app = App::new();
    app.add_plugins(crate::view::ViewPlugin {
        ranked: false,
        fresh_seed: false,
        interpolate: true,
    });
    if let Some(pkg) = first {
        load(app.world_mut(), &demo(pkg), false).unwrap();
    }
    let library = first.is_none().then(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
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
            ShellPlugin {
                library,
                trace_mode: false,
                start: AppScreen::Select,
            },
            crate::bindings::BindingsPlugin(crate::bindings::Bindings::default()),
            crate::art::ArtPlugin,
            crate::audio::AudioPlugin::audible(),
            crate::mods::ModsPlugin(mods),
            crate::visual::VisualPlugin,
            crate::practice::PracticePlugin { slow_speed: Some(75) },
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
        .add_plugins((
            crate::results::ResultsPlugin(crate::results::ResultsArgs::default()),
            crate::script_error::ScriptErrorPlugin(crate::script_error::ScriptErrorArgs::default()),
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

/// Waits until the select screen's preview worker has delivered everything it asked for, so an
/// asset count is not compared across a preview arriving (#1593).
fn settle_previews(app: &mut App) {
    app.update();
    for _ in 0..2000 {
        app.update();
        let Some(lib) = app.world().get_resource::<select::library::Library>() else {
            return;
        };
        if !lib.busy() {
            app.update();
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the select screen's previews never settled");
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update();
    }
}

/// Updates until the staged start's worker has delivered and `poll_start` took it.
fn wait_for_stage(app: &mut App) {
    for _ in 0..2000 {
        if !app.world().contains_resource::<PendingStart>() {
            return;
        }
        app.update();
    }
    panic!("the staged start never finished");
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
    settle_previews(&mut app);
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
    settle_previews(&mut app);
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

/// Results' Back/Escape closes a finished run rather than resuming it (#1553): game select
/// with a list, the client's exit without one.
#[test]
fn results_back_leaves_the_finished_run() {
    let mut app = app();
    frames(&mut app, 3);
    start(app.world_mut(), &demo("tests/fixtures/clear.pkg")).unwrap();
    frames(&mut app, 3);
    crate::results::preview_next(app.world_mut(), crate::results::Scenario::Pb);
    frames(&mut app, 2);
    assert_eq!(screen(&app), RunScreen::Results);
    press(&mut app, KeyCode::Escape);
    frames(&mut app, 2);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select,
        "the finished run goes back to game select"
    );

    // Without --select the client plays one package: the finished run closes the client.
    let mut app = app_on(true, Some("tests/fixtures/clear.pkg"));
    frames(&mut app, 3);
    crate::results::preview_next(app.world_mut(), crate::results::Scenario::Pb);
    frames(&mut app, 2);
    assert_eq!(screen(&app), RunScreen::Results);
    press(&mut app, KeyCode::Escape);
    assert_eq!(app.should_exit(), Some(AppExit::Success), "it does not resume");
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

/// Starting a map by id plays that map (#880); no id is the first map. The select screen stages
/// the start, so the run opens a few frames later.
#[test]
fn start_map_plays_the_picked_map() {
    let mut app = app();
    frames(&mut app, 3);
    let pkg = demo("tests/fixtures/maps.pkg");
    start_map(app.world_mut(), &pkg, Some("hard"), PlayMode::Original, None);
    wait_for_stage(&mut app);
    frames(&mut app, 2);
    assert_eq!(map_id(&app), "hard");
    leave(app.world_mut());
    frames(&mut app, 3);
    start(app.world_mut(), &pkg).unwrap();
    frames(&mut app, 3);
    assert_eq!(map_id(&app), "medium", "no id: the first map");
}

/// A save state is the world: `Sim::snapshot` and `Sim::restore` round-trip the state hash and
/// the tick, and `F5`/`F6` in a practice run save and load it (#39).
#[test]
fn a_save_state_restores_the_world_and_f5_f6_use_it() {
    let mut app = app();
    frames(&mut app, 3);
    app.world_mut()
        .insert_resource(crate::practice::Practice(true));
    start(app.world_mut(), &demo("tests/fixtures/maps.pkg")).unwrap();
    frames(&mut app, 20);
    // The press's frame ticks before the key system saves, so read the state after it.
    press(&mut app, crate::practice::SAVE_STATE_KEY);
    let saved_tick = tick(&app);
    let saved_hash = {
        let game = app.world().non_send::<Game>();
        iwbtg_sim::hash::state_hash(game.sim.world())
    };
    frames(&mut app, 20);
    assert!(tick(&app) > saved_tick, "the run went on");

    press(&mut app, crate::practice::LOAD_STATE_KEY);
    assert_eq!(tick(&app), saved_tick, "F6 restored the tick");
    let game = app.world().non_send::<Game>();
    assert_eq!(
        iwbtg_sim::hash::state_hash(game.sim.world()),
        saved_hash,
        "and the whole state"
    );
    }

/// A finished run's clear does not follow the player into the next run: starting again from
/// game select begins in `Playing` with no results page and no remembered clear (#1551).
#[test]
fn a_new_run_from_select_does_not_open_the_old_results() {
    use crate::stats::{
        Board, Cleared, DeathLog, MapStats, Recorded, Run, Stats, board_key, map_key,
    };
    let mut app = app();
    frames(&mut app, 3);
    start(app.world_mut(), &demo("tests/fixtures/clear.pkg")).unwrap();
    frames(&mut app, 5);
    let map = map_key("demo_clear", "main");
    let board = board_key(false, "NM");
    let run = Run {
        ticks: 3000,
        deaths: 0,
        splits: vec![1500, 3000],
        segment_deaths: vec![0, 0],
        finished_at: 0,
        replay: None,
        death_log: DeathLog::default(),
    };
    {
        let mut stats = app.world_mut().resource_mut::<Stats>();
        let mut map_stats = MapStats {
            game: "demo_clear".to_owned(),
            tick_rate: 50,
            clears: 1,
            ..Default::default()
        };
        map_stats.boards.insert(
            board.clone(),
            Board {
                clears: 1,
                runs: vec![run.clone()],
                ..Default::default()
            },
        );
        stats.book.maps.insert(map.clone(), map_stats);
        stats.last = Some(Recorded {
            pb_before: None,
            pb_splits_before: Vec::new(),
            is_pb: false,
            gold: vec![false, false],
            place: Some(1),
            sum_of_best_before: None,
            sum_of_best: None,
        });
        stats.cleared = Some(Cleared {
            map,
            board,
            run,
        });
    }
    frames(&mut app, 2);
    assert_eq!(
        screen(&app),
        RunScreen::Results,
        "the recorded clear opens the page"
    );

    // Leave for game select, as the results' quit does, and start the same package again.
    leave(app.world_mut());
    frames(&mut app, 3);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select
    );
    assert!(
        app.world().resource::<Stats>().cleared.is_none(),
        "leaving the run forgets its clear"
    );
    start_map(
        app.world_mut(),
        &demo("tests/fixtures/clear.pkg"),
        None,
        PlayMode::Original,
        None,
    );
    wait_for_stage(&mut app);
    frames(&mut app, 5);
    assert_eq!(
        screen(&app),
        RunScreen::Playing,
        "the new run does not open the old results"
    );

    // And a different package after leaving again.
    leave(app.world_mut());
    frames(&mut app, 3);
    start_map(
        app.world_mut(),
        &demo("tests/fixtures/audio.pkg"),
        None,
        PlayMode::Original,
        None,
    );
    wait_for_stage(&mut app);
    frames(&mut app, 5);
    assert_eq!(screen(&app), RunScreen::Playing);
}

/// The select screen's two buttons choose the play mode (#43): Ranked is stripped and starts at
/// the ranked start, Play original is the game as ported.
#[test]
fn start_map_plays_the_chosen_mode() {
    let mut app = app();
    frames(&mut app, 3);
    let pkg = demo("tests/fixtures/maps.pkg");
    start_map(app.world_mut(), &pkg, Some("hard"), PlayMode::Stripped, None);
    wait_for_stage(&mut app);
    frames(&mut app, 2);
    {
        let sim = &app.world().non_send::<Game>().sim;
        assert_eq!(sim.options().mode, PlayMode::Stripped);
        assert_eq!(
            sim.def().room(sim.world().room).map(|r| r.name.as_str()),
            Some("goal"),
            "the ranked start room"
        );
    }
    leave(app.world_mut());
    frames(&mut app, 3);
    start_map(app.world_mut(), &pkg, Some("hard"), PlayMode::Original, None);
    wait_for_stage(&mut app);
    frames(&mut app, 2);
    let sim = &app.world().non_send::<Game>().sim;
    assert_eq!(sim.options().mode, PlayMode::Original);
    assert_eq!(
        sim.def().room(sim.world().room).map(|r| r.name.as_str()),
        Some("stage"),
        "Play original's new game room"
    );
}

/// A map the package does not have is reported by the staged start and nothing changes.
#[test]
fn start_map_refuses_an_unknown_map() {
    let mut app = app();
    frames(&mut app, 3);
    start_map(
        app.world_mut(),
        &demo("tests/fixtures/maps.pkg"),
        Some("nope"),
        PlayMode::Original,
        None,
    );
    wait_for_stage(&mut app);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select
    );
    assert!(
        !app.world().resource::<Messages<StartFailed>>().is_empty(),
        "the failure is reported"
    );
}

/// A staged start keeps the run on screen playing until its package is ready, then swaps in the
/// same run `shell::start` would build, atomically (one camera, the new sim from its first tick).
#[test]
fn a_staged_start_keeps_the_run_until_it_is_ready() {
    let mut app = app();
    frames(&mut app, 3);
    start(app.world_mut(), &demo("tests/fixtures/audio.pkg")).unwrap();
    frames(&mut app, 20);
    assert_eq!(package_id(&app), "demo_audio");
    let ticked = tick(&app);

    // The work `start_map`'s worker does, delivered on a channel the test controls.
    let path = demo("tests/fixtures/clear.pkg");
    let staged = stage(&path, false).unwrap();
    let (tx, rx) = mpsc::channel();
    app.world_mut().insert_resource(PendingStart {
        generation: 1,
        path: path.clone(),
        map: None,
        mode: PlayMode::Original,
        start: None,
        rx: Mutex::new(rx),
    });
    frames(&mut app, 10);
    assert_eq!(package_id(&app), "demo_audio", "the old run is untouched");
    assert!(tick(&app) > ticked, "the old run keeps playing while staged");

    tx.send((1, Ok(staged))).unwrap();
    frames(&mut app, 2);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Run
    );
    assert_eq!(package_id(&app), "demo_clear");
    assert!(tick(&app) <= 8, "the new run starts from its first tick");
    assert!(app.world().get_resource::<PendingStart>().is_none());
    let world = app.world_mut();
    let cameras = world.query_filtered::<(), With<GameCamera>>().iter(world).count();
    assert_eq!(cameras, 1, "the old run's camera is gone, the new one is there");
    assert!(world.contains_resource::<Textures>());
}

/// A staged start whose start room halts in its first tick is not a failure: it opens the run on
/// the script error screen, as the synchronous start does (#946).
#[test]
fn a_staged_start_that_halts_opens_the_error_screen() {
    let mut app = app();
    frames(&mut app, 3);
    start_map(
        app.world_mut(),
        &demo("tests/fixtures/start_error.pkg"),
        None,
        PlayMode::Original,
        None,
    );
    wait_for_stage(&mut app);
    frames(&mut app, 3);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Run
    );
    assert_eq!(run_screen(app.world()), Some(RunScreen::Error));
    assert!(app.world().resource::<Messages<StartFailed>>().is_empty());
}

/// A staged start that does not start reports why and leaves the run on screen playing.
#[test]
fn a_failed_staged_start_keeps_the_run_playing() {
    let mut app = app();
    frames(&mut app, 3);
    start(app.world_mut(), &demo("tests/fixtures/audio.pkg")).unwrap();
    frames(&mut app, 20);
    let ticked = tick(&app);

    let path = demo("tests/fixtures/maps.pkg");
    let (tx, rx) = mpsc::channel();
    app.world_mut().insert_resource(PendingStart {
        generation: 1,
        path: path.clone(),
        map: Some("nope".to_owned()),
        mode: PlayMode::Original,
        start: None,
        rx: Mutex::new(rx),
    });
    tx.send((1, Ok(stage(&path, false).unwrap()))).unwrap();
    frames(&mut app, 1);
    assert_eq!(package_id(&app), "demo_audio", "the run is untouched");
    assert!(tick(&app) > ticked, "it keeps playing");
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Run
    );
    assert!(!app.world().resource::<Messages<StartFailed>>().is_empty());
}

/// Copies a package folder tree.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Writes a solid `size`×`size` RGBA PNG.
fn write_png(path: &Path, size: u32) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&vec![0x80; (size * size * 4) as usize])
        .unwrap();
}

/// Startup phases on reproducible synthetic content (#1023): a copy of `audio.pkg` with 400 extra
/// 256×256 sprites. Not a check; run it by hand and compare runs on the same machine:
///
/// `cargo test -p iwbtg-client --locked shell::tests::startup_phases -- --ignored --nocapture`
#[test]
#[ignore = "a measurement, not a check (#1023)"]
fn startup_phases() {
    use std::time::Instant;
    const SPRITES: u32 = 400;
    const SIZE: u32 = 256;
    let dir = std::env::temp_dir().join(format!("iwbtg-startup-{}-{SPRITES}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_tree(&demo("tests/fixtures/audio.pkg"), &dir);
    for i in 0..SPRITES {
        std::fs::write(
            dir.join(format!("sprites/synthetic{i}.toml")),
            format!(
                "frames = 1\nframe_size = [{SIZE}, {SIZE}]\norigin = [0, 0]\n\
                 collision = \"rect\"\nbbox = [0, 0, {}, {}]\n",
                SIZE - 1,
                SIZE - 1
            ),
        )
        .unwrap();
        write_png(&dir.join(format!("sprites/synthetic{i}.png")), SIZE);
    }
    let started = Instant::now();
    let pkg = crate::package::open(&dir, false).unwrap();
    let open = started.elapsed();
    let started = Instant::now();
    let sprites = crate::package::sprite_images(&pkg.files);
    let decode = started.elapsed();
    let bytes: usize = sprites.iter().flatten().map(|s| s.rgba.len()).sum();
    let started = Instant::now();
    let prepared = iwbtg_sim::sim::Sim::prepare(&pkg.def).unwrap();
    let scripts = started.elapsed();
    // The first room: build the sim and run its start-room Create, as `view::new_run` does.
    let options = iwbtg_sim::sim::SimOptions {
        start: crate::savefile::start_point(&pkg.def),
        seed: 0,
        dll_mode: iwbtg_sim::sound::DllMode::Model,
        mode: iwbtg_sim::stripped::PlayMode::Original,
        map: None,
    };
    let started = Instant::now();
    let sim = iwbtg_sim::sim::Sim::new_or_halted_prepared(
        std::sync::Arc::clone(&pkg.def),
        options,
        &prepared,
    )
    .unwrap();
    let first_room = started.elapsed();
    assert_eq!(sim.world().tick, 0);
    eprintln!(
        "startup phases ({SPRITES} x {SIZE}px sprites): open {open:?}, sprite decode {decode:?} \
         ({} MiB RGBA), script prepare {scripts:?}, first room {first_room:?}",
        bytes / (1 << 20)
    );
    std::fs::remove_dir_all(&dir).ok();

    // The same phases on the real showcase package, for a non-synthetic point of comparison.
    let showcase = Path::new(env!("CARGO_MANIFEST_DIR")).join("showcase.pkg");
    let started = Instant::now();
    let pkg = crate::package::open(&showcase, false).unwrap();
    let open = started.elapsed();
    let started = Instant::now();
    let sprites = crate::package::sprite_images(&pkg.files);
    let decode = started.elapsed();
    let started = Instant::now();
    let _prepared = iwbtg_sim::sim::Sim::prepare(&pkg.def).unwrap();
    let scripts = started.elapsed();
    eprintln!(
        "startup phases (showcase.pkg): open {open:?}, sprite decode {decode:?} ({} sprites), \
         script prepare {scripts:?}",
        sprites.iter().flatten().count()
    );
}

/// A newer staged start replaces an older one; a stale result never commits.
#[test]
fn a_newer_staged_start_wins() {
    let mut app = app();
    frames(&mut app, 3);
    let old = demo("tests/fixtures/clear.pkg");
    let new = demo("tests/fixtures/maps.pkg");
    let old_staged = stage(&old, false).unwrap();
    let new_staged = stage(&new, false).unwrap();
    let (tx, rx) = mpsc::channel();
    app.world_mut().insert_resource(PendingStart {
        generation: 2,
        path: new.clone(),
        map: None,
        mode: PlayMode::Original,
        start: None,
        rx: Mutex::new(rx),
    });
    // The result of generation 1 is stale and must be ignored.
    tx.send((1, Ok(old_staged))).unwrap();
    frames(&mut app, 2);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select,
        "a stale result commits nothing"
    );
    tx.send((2, Ok(new_staged))).unwrap();
    frames(&mut app, 2);
    assert_eq!(package_id(&app), "demo_maps");
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
    {
        // The page shows over the game; the HUD steps aside for it (m5 D13).
        let world = app.world_mut();
        assert!(
            world
                .query_filtered::<&Camera, With<GameCamera>>()
                .iter(world)
                .all(|c| c.is_active),
            "the menu shows over the game"
        );
        assert!(world.resource::<crate::hud::Suspended>().0, "the HUD steps aside");
    }
    let at = tick(&app);
    frames(&mut app, 10);
    assert_eq!(tick(&app), at, "no tick in the pause menu");

    press(&mut app, crate::pause::PAUSE_KEY);
    frames(&mut app, 1);
    assert_eq!(screen(&app), RunScreen::Playing);
    assert!(!app.world().resource::<crate::hud::Paused>().open);
    assert!(game_shows(&mut app), "the HUD is back");
    frames(&mut app, 10);
    assert!(tick(&app) > at, "the game ticks again");

    crate::results::preview_next(app.world_mut(), crate::results::Scenario::Pb);
    frames(&mut app, 2);
    assert_eq!(screen(&app), RunScreen::Results);
    assert!(!game_shows(&mut app), "the results cover the game");
    let at = tick(&app);
    frames(&mut app, 10);
    assert_eq!(tick(&app), at, "no tick on the results");
    press(&mut app, KeyCode::Escape);
    frames(&mut app, 2);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select,
        "Escape leaves the finished run for game select"
    );
    assert!(
        app.world().get_resource::<State<RunScreen>>().is_none(),
        "no run, no run screen"
    );
    // Back on the list, a new run opens on the card again.
    start(app.world_mut(), &demo("tests/fixtures/clear.pkg")).unwrap();
    frames(&mut app, 2);
    assert_eq!(screen(&app), RunScreen::Loading);
    leave(app.world_mut());
    frames(&mut app, 2);
    assert!(
        app.world().get_resource::<State<RunScreen>>().is_none(),
        "no run, no run screen"
    );
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

/// A script error stops the game, not the client (#101): the `script_error.pkg` fixture's boss
/// reads past its array half a second in. The sim halts, the run goes to the script error screen
/// with the file, line and message, the sim ticks no more, and Escape goes back to game select;
/// another package then plays.
#[test]
fn a_script_error_stops_the_game_and_goes_back_to_select() {
    let mut app = app();
    frames(&mut app, 3);
    start(app.world_mut(), &demo("tests/fixtures/script_error.pkg")).unwrap();
    frames(&mut app, 60);
    assert_eq!(run_screen(app.world()), Some(RunScreen::Error));
    let report = app
        .world()
        .resource::<crate::script_error::Stopped>()
        .report()
        .cloned()
        .expect("a report");
    assert_eq!(report.heading, "Script error");
    assert_eq!(report.file.as_deref(), Some("scripts/boss.rhai"));
    assert_eq!(report.line, Some(9));
    assert!(report.message.contains("out of bounds"), "{report:?}");
    assert!(
        report
            .context
            .as_deref()
            .is_some_and(|c| c.contains("kind @boss")),
        "{report:?}"
    );
    let stopped_at = tick(&app);
    frames(&mut app, 20);
    assert_eq!(tick(&app), stopped_at, "no tick on the error screen");
    assert!(
        app.world().resource::<crate::hud::Suspended>().0,
        "the HUD is hidden"
    );

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    frames(&mut app, 1);
    // No input plugin here clears the key's press edge: the next run would see it.
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.release(KeyCode::Escape);
    keys.clear();
    frames(&mut app, 2);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select
    );
    play_and_leave(&mut app, "tests/fixtures/clear.pkg");
}

/// Retry on the script error screen starts the run again from its first tick, on the game.
#[test]
fn retry_on_the_script_error_screen_plays_again() {
    let mut app = app();
    frames(&mut app, 3);
    start(app.world_mut(), &demo("tests/fixtures/script_error.pkg")).unwrap();
    frames(&mut app, 60);
    assert_eq!(run_screen(app.world()), Some(RunScreen::Error));
    crate::retry::request(app.world_mut());
    frames(&mut app, 3);
    assert_eq!(run_screen(app.world()), Some(RunScreen::Playing));
    assert!(tick(&app) < 10, "a new run: tick {}", tick(&app));
    assert!(
        app.world()
            .resource::<crate::script_error::Stopped>()
            .report()
            .is_none()
    );
}

/// The `start_error.pkg` fixture's report: its boss's Create fails in the start room (#946).
fn assert_start_error(app: &App) {
    assert_eq!(run_screen(app.world()), Some(RunScreen::Error));
    let report = app
        .world()
        .resource::<crate::script_error::Stopped>()
        .report()
        .cloned()
        .expect("a report");
    assert_eq!(report.heading, "Script error");
    assert_eq!(report.file.as_deref(), Some("scripts/boss.rhai"));
    assert_eq!(report.line, Some(5));
    assert!(report.message.contains("out of bounds"), "{report:?}");
    assert!(
        report
            .context
            .as_deref()
            .is_some_and(|c| c.contains("kind @boss") && c.contains("Create")),
        "{report:?}"
    );
    assert_eq!(tick(app), 0, "the sim halted in its first tick");
}

/// A script error in the start room's Create (the first tick, run while the sim is built) opens
/// the run on the script error screen from game select (#946), not the NOT OPENED notice. Retry
/// shows it again; Escape goes back to game select, and another package then plays.
#[test]
fn a_first_tick_script_error_opens_the_error_screen_from_select() {
    let mut app = app();
    frames(&mut app, 3);
    start(app.world_mut(), &demo("tests/fixtures/start_error.pkg")).expect("the run opens");
    frames(&mut app, 3);
    assert_start_error(&app);
    frames(&mut app, 10);
    assert_eq!(tick(&app), 0, "no tick on the error screen");

    crate::retry::request(app.world_mut());
    frames(&mut app, 3);
    assert_start_error(&app);

    press(&mut app, KeyCode::Escape);
    frames(&mut app, 2);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select
    );
    play_and_leave(&mut app, "tests/fixtures/clear.pkg");
}

/// The same package on the command line (#946): the client opens, shows the loading card and then
/// the script error screen instead of exiting with the message; its Back is Quit, and quits.
#[test]
fn a_first_tick_script_error_opens_the_error_screen_from_the_command_line() {
    let mut app = app_on(false, Some("tests/fixtures/start_error.pkg"));
    frames(&mut app, 3);
    assert_eq!(screen(&app), RunScreen::Loading);
    frames(&mut app, 80);
    assert_start_error(&app);
    assert!(!app.world().resource::<Shell>().goes_back(), "Back is Quit");
    assert!(app.should_exit().is_none());

    press(&mut app, KeyCode::Escape);
    frames(&mut app, 1);
    assert_eq!(app.should_exit(), Some(AppExit::Success));
}

/// A click or an Enter on a button of game select reaches every global `Activate` and
/// `PointerClick` observer, the run screens' too. Their resources exist only in a run, so they
/// must skip on the select screen instead of failing validation and panicking (#1467).
#[test]
fn activating_a_button_on_the_select_screen_does_not_panic() {
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::backend::HitData;
    use bevy::picking::events::{Pointer, PointerClick};
    use bevy::picking::pointer::{Location, PointerButton, PointerId};
    use bevy::window::WindowRef;

    let mut app = app();
    frames(&mut app, 3);
    let entity = app.world_mut().spawn_empty().id();
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity });
    app.world_mut().trigger(PointerClick {
        entity,
        pointer: Pointer::new(
            PointerId::Mouse,
            Location {
                target: NormalizedRenderTarget::Window(
                    WindowRef::Entity(Entity::PLACEHOLDER)
                        .normalize(None)
                        .unwrap_or_else(|| unreachable!()),
                ),
                position: Vec2::ZERO,
            },
        ),
        button: PointerButton::Primary,
        hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
        duration: Duration::ZERO,
        count: 1,
    });
    frames(&mut app, 2);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select
    );
}

/// The toolbar's mode switch on a mode's screen opens the other mode's screen: Classic goes to
/// the shelf, Time Attack back to game select (m5 A1, B8; #1618). Leaving a run goes to the
/// mode's screen.
#[test]
fn the_mode_switch_moves_between_game_select_and_the_shelf() {
    use crate::ui::mode::Mode;
    let mut app = app();
    app.world_mut().insert_resource(Mode::TimeAttack);
    frames(&mut app, 3);
    *app.world_mut().resource_mut::<Mode>() = Mode::Classic;
    frames(&mut app, 2);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Shelf
    );
    *app.world_mut().resource_mut::<Mode>() = Mode::TimeAttack;
    frames(&mut app, 2);
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Select
    );
    assert_eq!(mode_home(Some(Mode::Classic)), AppScreen::Shelf);
    assert_eq!(mode_home(None), AppScreen::Select);
}
