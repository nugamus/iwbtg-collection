//! The select screen (`--select <folder>`): the packages in a folder as game select's list
//! (#149, `select/list.rs`): a panel per game with its key art, its maps under the selected one.
//! Up and Down pick a map, Left and Right a game, a click picks either; Enter (or a click on the
//! selected map) plays, Escape quits. Above the list, search, group, sort and filters (#150,
//! `select/controls.rs`, `select/filters.rs`) pick the rows it shows. The left side is a
//! placeholder until #148: the picked game and map, and the keys.

use std::path::{Path, PathBuf};

use bevy::camera::visibility::RenderLayers;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::picking::events::PointerClick;
use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;
use bevy::ui_widgets::Activate;
use bevy::window::PrimaryWindow;
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::stripped::PlayMode;

use super::AppScreen;
use crate::stats::Book;
use crate::ui::backdrop::{Backdrop, dim};
use crate::ui::fonts::Type;
use crate::ui::notice;
use crate::ui::tokens;
use crate::ui::widgets::text;

pub mod collections;
pub mod controls;
pub mod filters;
pub mod footer;
pub mod library;
pub mod list;
pub mod mod_sheet;
pub mod practice;

use collections::Collections;
use filters::Filters;
use library::{GameInfo, Library, file_name};
use list::{GamePanel, ListDim, MapPanel, Selection, Shown};

/// The window's title on the select screen.
pub const TITLE: &str = "IWBTG Collection";
/// The layout is drawn for a window this tall (the artboard's 1080), scaled to the real one.
const DESIGN_HEIGHT: f32 = 1080.0;

/// The left side's picked game title (placeholder until #148).
#[derive(Component)]
struct PickedTitle;

/// The left side's picked map and your PB on it (placeholder until #148).
#[derive(Component)]
struct PickedMap;

/// A button that plays the selected map in this mode (#43): Ranked (stripped) or Play original.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct ModeButton(PlayMode);

/// The keys hint under the pick; while a staged start prepares it says `Opening <name>…`.
#[derive(Component)]
struct HintText;

/// The keys hint, and the hint when the folder holds no packages.
const HINT: &str = "Type to search · ↑ ↓ ← → pick · Enter plays Ranked · Esc clears, quits";
const NO_PACKAGES: &str = "No packages here (folders with a game.toml, .iwbz files).";

/// The select screen: added by main.rs with the window (it needs the UI kit). The flag is
/// `--mod-select-preview`: the mod select sheet opens with the screen.
pub struct SelectPlugin(pub Option<PathBuf>, pub bool, pub bool);

impl Plugin for SelectPlugin {
    fn build(&self, app: &mut App) {
        let folder = self.0.clone();
        let preview = self.1;
        let practice_preview = self.2;
        // The player's favorites and collections (a test inserts its own empty set first).
        if app.world().get_resource::<Collections>().is_none() {
            app.insert_resource(Collections::open(crate::storage::Store::data()));
        }
        app.init_resource::<ListDim>()
            .init_resource::<Filters>()
            .init_resource::<collections::Renaming>()
            .init_resource::<Shown>()
            .init_resource::<mod_sheet::Sheet>()
            .init_resource::<practice::Sheet>()
            .init_resource::<practice::Pick>()
            .insert_resource(practice::Preview(practice_preview))
            // `failed` reads it; the screen's tests add this plugin without `ShellPlugin`.
            .add_message::<super::StartFailed>()
            .add_systems(
                OnEnter(AppScreen::Select),
                move |mut commands: Commands,
                      mut images: ResMut<Assets<Image>>,
                      lib: Option<ResMut<Library>>,
                      sel: Option<Res<Selection>>,
                      stats: Option<Res<crate::stats::Stats>>,
                      mods: Option<Res<crate::mods::RunMods>>,
                      speed: Option<Res<crate::practice::SlowSpeed>>,
                      mut sheet: ResMut<mod_sheet::Sheet>,
                      mut practice_sheet: ResMut<practice::Sheet>,
                      mut practice_pick: ResMut<practice::Pick>,
                      filters: Res<Filters>,
                      lists: Res<Collections>,
                      mut shown: ResMut<Shown>,
                      mut backdrop: ResMut<Backdrop>,
                      mut window: Query<&mut Window, With<PrimaryWindow>>| {
                    let paths = folder.as_deref().map(packages).unwrap_or_default();
                    let empty = Book::default();
                    let book = stats.as_ref().map_or(&empty, |s| &s.book);
                    // The sheet's picks live in `RunMods` and `SlowSpeed`; a headless test has
                    // neither until here.
                    if mods.is_none() {
                        commands.insert_resource(crate::mods::RunMods(ModSet::NONE));
                    }
                    if speed.is_none() {
                        commands.insert_resource(crate::practice::SlowSpeed(crate::practice::SLOW_SPEEDS[0]));
                    }
                    let mods = mods.map_or(ModSet::NONE, |m| m.0);
                    let speed = speed.map_or(crate::practice::SLOW_SPEEDS[0], |s| s.0);
                    sheet.open = false;
                    practice_sheet.open = false;
                    practice_pick.start = None;
                    if preview {
                        sheet.open_with(mods);
                    }
                    let camera = camera(&mut commands);
                    let sel = sel.map(|s| *s).unwrap_or_default();
                    // A package replaced or changed at the same path has a new key (#1593).
                    let keys: Vec<library::PathKey> =
                        paths.iter().map(|p| library::path_key(p)).collect();
                    let same =
                        |l: &Library| l.matches(folder.as_deref(), &paths, &keys);
                    let sel = match lib {
                        // Back from a run: the art is kept, the PBs and clears are read again.
                        Some(lib) if same(&lib) => {
                            let lib = lib.into_inner();
                            lib.refresh(book, mods);
                            enter(
                                &mut commands,
                                camera,
                                folder.as_deref(),
                                lib,
                                sel,
                                &filters,
                                &lists,
                                &mut shown,
                            )
                        }
                        _ => {
                            let mut lib = Library::load(folder.as_deref(), &paths, &mut images);
                            lib.refresh(book, mods);
                            let sel = enter(
                                &mut commands,
                                camera,
                                folder.as_deref(),
                                &lib,
                                sel,
                                &filters,
                                &lists,
                                &mut shown,
                            );
                            commands.insert_resource(lib);
                            sel
                        }
                    };
                    commands.insert_resource(sel);
                    footer::spawn(&mut commands, camera, mods);
                    mod_sheet::spawn(
                        &mut commands,
                        camera,
                        mods,
                        speed,
                        crate::settings::get().auto_retry,
                    );
                    backdrop.art = None;
                    backdrop.dim = dim::SELECT;
                    for mut w in &mut window {
                        w.title = TITLE.to_owned();
                    }
                },
            )
            .add_observer(on_click)
            .add_observer(on_mode)
            .add_observer(practice::open)
            .add_observer(practice::on_activate)
            .add_observer(controls::on_activate)
            .add_observer(collections::favorite)
            .add_observer(collections::member)
            .add_observer(collections::create)
            .add_observer(mod_sheet::on_activate)
            .add_observer(mod_sheet::on_option)
            .add_systems(
                Update,
                (
                    (
                        controls::search,
                        list::refilter,
                        controls::labels,
                        keys.run_if(mod_sheet::closed).run_if(practice::closed),
                        collections::keys,
                        mod_sheet::keys,
                        mod_sheet::slide,
                        mod_sheet::follow,
                        mod_sheet::auto_retry,
                        records,
                        list::looks,
                        list::arrange,
                        list::settle,
                        list::dim,
                        list::want_previews,
                        list::previews,
                        picked,
                        collections::favorite_label,
                        collections::chips,
                    )
                        .chain(),
                    (
                        opening,
                        failed,
                        collections::editor_labels,
                    practice::chips,
                    practice::keys,
                    practice::preview,
                        scale,
                    )
                        .chain(),
                )
                    .chain()
                    .run_if(in_state(AppScreen::Select)),
            );
    }
}

/// A new set of mods reads the list's PBs again (they are per board, and every set has its own).
fn records(
    mods: Res<crate::mods::RunMods>,
    stats: Option<Res<crate::stats::Stats>>,
    mut lib: ResMut<Library>,
) {
    if !mods.is_changed() || mods.is_added() {
        return;
    }
    let empty = Book::default();
    lib.refresh(stats.as_ref().map_or(&empty, |s| &s.book), mods.0);
}

/// The packages in `folder`: folders with a `game.toml` and `.iwbz` files, by name.
pub fn packages(folder: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| crate::package::is_iwbz(p) || (p.is_dir() && p.join("game.toml").is_file()))
        .collect();
    out.sort();
    out
}

fn camera(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            Camera2d,
            Camera {
                order: 10,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            RenderLayers::none(),
            IsDefaultUiCamera,
            DespawnOnExit(AppScreen::Select),
        ))
        .id()
}

/// Spawns the page for `lib` with the rows `filters` leave (into `shown`) and returns the
/// selection, kept inside the rows.
#[allow(clippy::too_many_arguments, reason = "the page's parts")]
fn enter(
    commands: &mut Commands,
    camera: Entity,
    folder: Option<&Path>,
    lib: &Library,
    sel: Selection,
    filters: &Filters,
    lists: &Collections,
    shown: &mut Shown,
) -> Selection {
    shown.rows = filters::rows(&lib.games, &lib.records, filters, lists);
    let order = shown.order();
    let game = if order.contains(&sel.game) {
        sel.game
    } else {
        order.first().copied().unwrap_or(0)
    };
    let maps = lib.games.get(game).map_or(0, |g| g.maps.len());
    let sel = Selection {
        game,
        map: sel.map.min(maps.saturating_sub(1)),
    };
    let where_ = folder.map_or_else(String::new, |f| f.display().to_string());
    let hint = if lib.games.is_empty() {
        NO_PACKAGES
    } else {
        HINT
    };
    // The left side: a placeholder until #148's title wedge, stats and tabs.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(64.0),
            top: Val::Px(150.0),
            width: Val::Px(860.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(12.0),
            ..default()
        },
        UiTargetCamera(camera),
        DespawnOnExit(AppScreen::Select),
        children![
            text("Select a game", Type::LABEL, tokens::MUTED),
            (
                text("", Type::condensed(56.0, 700).italic(), tokens::INK),
                PickedTitle
            ),
            (text("", Type::barlow(18.0, 600), tokens::SOFT), PickedMap),
            text(&where_, Type::barlow(14.0, 400), tokens::MUTED),
            (
                Node {
                    column_gap: Val::Px(10.0),
                    align_items: AlignItems::Center,
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                },
                children![
                    (
                        crate::ui::widgets::button(
                            "Ranked",
                            crate::ui::widgets::ButtonKind::Primary
                        ),
                        ModeButton(PlayMode::Stripped)
                    ),
                    (
                        crate::ui::widgets::button(
                            "Play original",
                            crate::ui::widgets::ButtonKind::Secondary
                        ),
                        ModeButton(PlayMode::Original)
                    ),
                    text(
                        "Ranked: the ranked start, timer on first input · Play original: the game as ported",
                        Type::barlow(14.0, 400),
                        tokens::MUTED
                    ),
                ],
            ),
            (text(hint, Type::BODY, tokens::SOFT), HintText),
            (
                Node {
                    column_gap: Val::Px(10.0),
                    align_items: AlignItems::Center,
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                },
                children![
                    (
                        crate::ui::widgets::button(
                            "Practice",
                            crate::ui::widgets::ButtonKind::Control
                        ),
                        practice::OpenButton
                    ),
                    text(
                        "Start at a stage, your save or any room · practice runs stay off the leaderboard",
                        Type::barlow(14.0, 400),
                        tokens::MUTED
                    ),
                ],
            ),
            collections::controls(lists, lib.games.get(sel.game).map(|g| g.id.as_str())),
        ],
    ));
    controls::spawn(commands, camera, lib, filters);
    list::spawn(commands, camera, lib, sel, shown);
    sel
}

/// The id of the game's `index`th map; `None` for a game without maps (one that did not read).
fn map_id(game: &GameInfo, index: usize) -> Option<String> {
    game.maps.get(index).map(|m| m.id.clone())
}

/// Stages the package at `path` on map `map` (`None`: its first); the list stays up with
/// `Opening <name>…` until it is ready, and one that does not open shows the NOT OPENED notice.
fn play(
    commands: &mut Commands,
    path: PathBuf,
    map: Option<String>,
    mode: PlayMode,
    start: Option<iwbtg_sim::room::StartPoint>,
) {
    commands.queue(move |world: &mut World| {
        if let Some(mut practice) = world.get_resource_mut::<crate::practice::Practice>() {
            practice.0 = false;
        }
        super::start_map(world, &path, map.as_deref(), mode, start);
    });
}

/// The Ranked and Play original buttons play the selected map in their mode.
fn on_mode(
    activate: On<Activate>,
    buttons: Query<&ModeButton>,
    lib: Option<Res<Library>>,
    sel: Option<Res<Selection>>,
    mut commands: Commands,
) {
    let (Ok(button), Some(lib), Some(sel)) = (buttons.get(activate.entity), lib, sel) else {
        return;
    };
    let Some(game) = lib.games.get(sel.game) else {
        return;
    };
    play(
        &mut commands,
        game.path.clone(),
        map_id(game, sel.map),
        button.0,
        None,
    );
}

/// A staged start that did not commit: the run on screen (if any) is unchanged, and the notice
/// says why.
fn failed(mut failed: MessageReader<super::StartFailed>) {
    for super::StartFailed(e) in failed.read() {
        eprintln!("{e}");
        notice::post(notice::Kind::Drop, &e.to_string());
    }
}

/// Shows `Opening <name>…` while a start is preparing, and the keys hint otherwise.
fn opening(
    pending: Option<Res<super::PendingStart>>,
    lib: Option<Res<Library>>,
    mut last: Local<String>,
    mut hint: Query<&mut Text, With<HintText>>,
) {
    let empty = lib.as_deref().is_none_or(|l| l.games.is_empty());
    // Nothing to write unless the pending start appeared or went away; the base hint does not
    // allocate the common path.
    let opening = pending.as_deref().map(super::PendingStart::opening);
    let text = opening.as_deref().unwrap_or(if empty { NO_PACKAGES } else { HINT });
    if last.as_str() == text {
        return;
    }
    last.clear();
    last.push_str(text);
    let styled = Type::BODY.set(text);
    for mut t in &mut hint {
        t.0 = styled.clone();
    }
}

/// The arrows move the selection among the games shown ([`list::step`]; Left and Right move
/// the search field's cursor instead while it holds text), Enter plays the selected map as
/// Ranked.
/// Escape closes an open dropdown, else clears the search, else quits. While the new
/// collection's name field has the focus, the arrows and Enter stay out of the way: Enter
/// creates the collection ([`collections::keys`]) and Escape goes back to the search field.
#[allow(clippy::too_many_arguments, reason = "the screen's keys")]
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    lib: Res<Library>,
    shown: Res<Shown>,
    mut focus: ResMut<InputFocus>,
    buttons: Query<(), With<bevy::ui_widgets::Button>>,
    names: Query<(), With<collections::NameField>>,
    menus: Query<Entity, With<controls::Menu>>,
    mut search: Query<
        (Entity, &mut bevy::text::EditableText),
        With<crate::ui::widgets::SearchField>,
    >,
    mut sel: ResMut<Selection>,
    mut commands: Commands,
) {
    let maps: Vec<usize> = lib.games.iter().map(|g| g.maps.len()).collect();
    let order = shown.order();
    let typing = search.iter().any(|(_, s)| !s.value().to_string().is_empty());
    let naming = focus.get().is_some_and(|e| names.contains(e));
    for key in [
        KeyCode::ArrowDown,
        KeyCode::ArrowUp,
        KeyCode::ArrowRight,
        KeyCode::ArrowLeft,
    ] {
        let sideways = matches!(key, KeyCode::ArrowLeft | KeyCode::ArrowRight);
        if keys.just_pressed(key) && !(sideways && typing) && !naming {
            let next = list::step(&order, &maps, *sel, key);
            if next != *sel {
                *sel = next;
            }
        }
    }
    // Enter on a focused button activates the button, not the game.
    let on_button = focus.get().is_some_and(|e| buttons.contains(e));
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter])
        && !on_button
        && !naming
        && order.contains(&sel.game)
        && let Some(game) = lib.games.get(sel.game)
    {
        play(
            &mut commands,
            game.path.clone(),
            map_id(game, sel.map),
            PlayMode::Stripped,
            None,
        );
    }
    if keys.just_pressed(KeyCode::Escape) {
        if naming {
            if let Some((entity, _)) = search.iter_mut().next() {
                focus.set(entity, FocusCause::Navigated);
            }
        } else if !controls::close_menus(&mut commands, &menus) {
            if typing {
                for (_, mut s) in &mut search {
                    s.clear();
                }
            } else {
                commands.write_message(AppExit::Success);
            }
        }
    }
}

/// A click on a game opens it on its first map; a click on a map selects it, and on the
/// selected map plays the game.
fn on_click(
    click: On<PointerClick>,
    games: Query<&GamePanel>,
    maps: Query<&MapPanel>,
    lib: Option<Res<Library>>,
    sel: Option<ResMut<Selection>>,
    mut commands: Commands,
) {
    let (Some(lib), Some(mut sel)) = (lib, sel) else {
        return;
    };
    if let Ok(game) = games.get(click.entity) {
        if game.0 != sel.game {
            *sel = Selection { game: game.0, map: 0 };
        }
    } else if let Ok(map) = maps.get(click.entity) {
        let picked = Selection {
            game: map.0,
            map: map.1,
        };
        if picked == *sel {
            if let Some(game) = lib.games.get(map.0) {
                play(
                    &mut commands,
                    game.path.clone(),
                    map_id(game, map.1),
                    PlayMode::Stripped,
                    None,
                );
            }
        } else {
            *sel = picked;
        }
    }
}

/// The left side's title and map line follow the selection.
fn picked(
    sel: Res<Selection>,
    lib: Res<Library>,
    mut title: Query<&mut Text, (With<PickedTitle>, Without<PickedMap>)>,
    mut map: Query<&mut Text, (With<PickedMap>, Without<PickedTitle>)>,
) {
    if !sel.is_changed() && !lib.is_changed() {
        return;
    }
    let game = lib.games.get(sel.game);
    let line = game.map_or_else(String::new, |g| {
        let pb = lib
            .records
            .get(sel.game)
            .and_then(|r| r.pbs.get(sel.map).cloned().flatten())
            .map_or_else(|| "No PB".to_owned(), |t| format!("PB {t}"));
        match g.maps.get(sel.map) {
            Some(m) => format!("{} · {pb}", m.name),
            None => format!("{}: its game.toml does not read", file_name(&g.path)),
        }
    });
    for mut t in &mut title {
        t.0 = game.map_or_else(String::new, |g| g.title.clone());
    }
    for mut t in &mut map {
        t.0 = line.clone();
    }
}

/// Scales the page with the window.
fn scale(window: Single<&Window, With<PrimaryWindow>>, mut ui_scale: ResMut<UiScale>) {
    let h = window.size().y;
    if h < 1.0 {
        return;
    }
    let s = (h / DESIGN_HEIGHT).clamp(0.25, 3.0);
    if (ui_scale.0 - s).abs() > 1e-4 {
        ui_scale.0 = s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_holds_package_folders_and_iwbz_files_by_name() {
        let d = std::env::temp_dir().join(format!("iwbtg-select-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("b-game")).unwrap();
        std::fs::write(d.join("b-game/game.toml"), "").unwrap();
        std::fs::create_dir_all(d.join("not-a-game")).unwrap();
        std::fs::write(d.join("a.iwbz"), "").unwrap();
        std::fs::write(d.join("notes.txt"), "").unwrap();
        let names: Vec<String> = packages(&d).iter().map(|p| file_name(p)).collect();
        assert_eq!(names, ["a.iwbz", "b-game"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The select screen on the test fixtures, without a window.
    fn screen() -> App {
        use bevy::state::app::StatesPlugin;
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
            .init_asset::<Image>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .init_resource::<UiScale>()
            .init_resource::<Backdrop>()
            .init_resource::<Collections>()
            .init_resource::<crate::practice::Practice>()
            .init_state::<AppScreen>()
            .add_plugins(SelectPlugin(Some(fixtures), false, false));
        app.update();
        app.update();
        app
    }

    /// The hint under the pick, as drawn.
    fn hint_text(app: &mut App) -> String {
        let world = app.world_mut();
        let mut hint = world.query_filtered::<&Text, With<HintText>>();
        hint.single(world).unwrap().0.clone()
    }

    /// The select screen shows `Opening <name>…` while a staged start prepares (#1023): the
    /// player gets feedback before the expensive work, without leaving the list.
    #[test]
    fn the_hint_says_opening_while_a_start_prepares() {
        let mut app = screen();
        assert_eq!(hint_text(&mut app), HINT);

        let (tx, rx) = std::sync::mpsc::channel();
        app.world_mut()
            .insert_resource(super::super::PendingStart {
                generation: 1,
                path: PathBuf::from("big-game.pkg"),
                map: None,
                mode: PlayMode::Original,
                start: None,
                rx: std::sync::Mutex::new(rx),
            });
        app.update();
        assert_eq!(hint_text(&mut app), "Opening big-game.pkg…");

        // Once the staged start is gone (committed or failed), the keys hint is back.
        drop(tx);
        app.world_mut()
            .remove_resource::<super::super::PendingStart>();
        app.update();
        assert_eq!(hint_text(&mut app), HINT);
    }

    /// Puts `text` in the search field, as typing does.
    fn type_in(app: &mut App, text: &str) {
        let world = app.world_mut();
        let mut fields =
            world.query_filtered::<&mut bevy::text::EditableText, With<crate::ui::widgets::SearchField>>();
        for mut field in fields.iter_mut(world) {
            *field = bevy::text::EditableText::new(text);
        }
    }

    /// Each game row's target place and opacity, by game.
    fn targets(app: &mut App) -> Vec<(usize, f32, f32)> {
        use crate::ui::motion::{OffsetY, Opacity, Tween};
        let world = app.world_mut();
        let mut out: Vec<(usize, f32, f32)> = world
            .query::<(&list::GameRow, &Tween<OffsetY>, &Tween<Opacity>)>()
            .iter(world)
            .map(|(r, y, o)| (r.0, y.to, o.to))
            .collect();
        out.sort_by_key(|t| t.0);
        out
    }

    /// Typing filters the list: the games that stay slide to their new places, the rest fade
    /// out; the selection moves to a game that is shown. A new sort slides the rows (#150).
    #[test]
    fn a_search_fades_out_the_rest_and_a_sort_slides_the_rows() {
        let mut app = screen();
        let shown = app.world().resource::<Shown>().order();
        assert_eq!(shown.len(), 8, "every fixture is listed");
        let before = targets(&mut app);
        assert!(before.iter().all(|t| (t.2 - 1.0).abs() < 1e-6));

        type_in(&mut app, "FIXTURE");
        app.update();
        // clear.pkg is "Clear fixture" (1), maps.pkg "Maps fixture" (3), script_error.pkg
        // "Script error fixture" (6), start_error.pkg "Start error fixture" (7).
        assert_eq!(app.world().resource::<Shown>().order(), [1, 3, 6, 7]);
        let after = targets(&mut app);
        for (g, y, opacity) in &after {
            let shown = [1, 3, 6, 7].contains(g);
            assert_eq!(*opacity, if shown { 1.0 } else { 0.0 }, "game {g}");
            if *g == 1 {
                assert_eq!(*y, 0.0, "the first game shown moves to the top");
            }
        }
        assert_eq!(app.world().resource::<Selection>().game, 1);

        type_in(&mut app, "");
        app.world_mut().resource_mut::<Filters>().group = filters::Field::Cleared;
        app.update();
        let rows = &app.world().resource::<Shown>().rows;
        assert_eq!(rows[0], filters::Row::Header("Not cleared".into(), 8));
        let headers = {
            let world = app.world_mut();
            world.query::<&list::HeaderRow>().iter(world).count()
        };
        assert_eq!(headers, 1, "a header row is spawned for the group");
        let grouped = targets(&mut app);
        assert!(
            grouped.iter().all(|t| t.1 >= list::HEADER_HEIGHT),
            "every game slides down under the header"
        );
    }

    /// Activates the first entity that has component `C` and passes `pick`.
    fn activate<C: Component>(app: &mut App, pick: impl Fn(&C) -> bool) {
        let world = app.world_mut();
        let entity = world
            .query::<(Entity, &C)>()
            .iter(world)
            .find(|(_, c)| pick(c))
            .map(|(e, _)| e)
            .expect("the entity is spawned");
        world.trigger(bevy::ui_widgets::Activate { entity });
        app.update();
    }

    /// Every text on the page, spans included.
    fn header_words(app: &mut App) -> String {
        let world = app.world_mut();
        let mut texts: Vec<(Entity, String)> = world
            .query::<(Entity, AnyOf<(&Text, &TextSpan)>)>()
            .iter(world)
            .map(|(e, (t, span))| {
                (
                    e,
                    t.map_or_else(|| span.map_or_else(String::new, |s| s.0.clone()), |t| t.0.clone()),
                )
            })
            .collect();
        texts.sort_by_key(|t| t.0);
        texts.into_iter().map(|t| t.1).collect::<Vec<_>>().join("|")
    }

    /// The Practice panel lists the map's stage and rooms; a chip and Start stage a practice
    /// run (`Practice` set, the picked `StartPoint` in `PendingStart`, #39).
    #[test]
    fn the_practice_pickers_stage_a_practice_start() {
        use super::super::PendingStart;
        use iwbtg_sim::room::StartPoint;
        use practice::{OpenButton, OptionStart, StartButton};
        let mut app = screen();
        *app.world_mut().resource_mut::<Selection>() = Selection { game: 3, map: 1 };
        app.update();
        activate::<OpenButton>(&mut app, |_| true);
        assert!(app.world().resource::<practice::Sheet>().open);
        activate::<OptionStart>(&mut app, |o| matches!(o.0, StartPoint::Room { .. }));
        assert!(app.world().resource::<practice::Pick>().start.is_some());
        activate::<StartButton>(&mut app, |_| true);
        assert!(!app.world().resource::<practice::Sheet>().open, "Start closes it");
        assert!(app.world().resource::<crate::practice::Practice>().0);
        let pending = app.world().resource::<PendingStart>();
        assert!(
            matches!(pending.start, Some(StartPoint::Room { .. })),
            "the picked stage's room"
        );
    }

    /// The left side offers the two ways to play; each stages the selected map in its mode,
    /// and the second replaces the first (#43).
    #[test]
    fn the_mode_buttons_start_the_selected_map_in_their_mode() {
        use super::super::PendingStart;
        let mut app = screen();
        // The hard map of the maps fixture (game 3 in the title order).
        *app.world_mut().resource_mut::<Selection>() = Selection { game: 3, map: 1 };
        app.update();
        activate::<ModeButton>(&mut app, |b| b.0 == PlayMode::Stripped);
        let pending = app.world().resource::<PendingStart>();
        assert_eq!(pending.mode, PlayMode::Stripped);
        assert_eq!(pending.map.as_deref(), Some("hard"));
        assert!(pending.path.ends_with("maps.pkg"));

        activate::<ModeButton>(&mut app, |b| b.0 == PlayMode::Original);
        assert_eq!(
            app.world().resource::<PendingStart>().mode,
            PlayMode::Original,
            "the second button replaces the staged start"
        );
    }

    /// Puts `name` in the new collection field, as typing does.
    fn name_collection(app: &mut App, name: &str) {
        let world = app.world_mut();
        let mut fields =
            world.query_filtered::<&mut bevy::text::EditableText, With<collections::NameField>>();
        for mut field in fields.iter_mut(world) {
            *field = bevy::text::EditableText::new(name);
        }
    }

    /// The Favorite toggle, a named collection and the Collections filter (#105).
    #[test]
    fn favorites_and_collections_come_from_the_left_side_and_filter_the_list() {
        use collections::{Collections, CreateButton, Favorite, FavoriteLabel, Member, Pick};
        use crate::ui::widgets::Chip;
        let mut app = screen();
        *app.world_mut().resource_mut::<Selection>() = Selection { game: 3, map: 0 };
        app.update();
        assert_eq!(app.world().resource::<Library>().games[3].id, "demo_maps");

        activate::<Favorite>(&mut app, |_| true);
        assert!(
            app.world().resource::<Collections>().is_favorite("demo_maps"),
            "the toggle favorites the selected game"
        );
        let label = {
            let world = app.world_mut();
            world
                .query_filtered::<&Text, With<FavoriteLabel>>()
                .iter(world)
                .map(|t| t.0.clone())
                .collect::<Vec<_>>()
        };
        assert!(label.iter().any(|l| l.contains("Favorited")), "{label:?}");

        app.world_mut().resource_mut::<Filters>().collection = Some(Pick::Favorites);
        app.update();
        assert_eq!(
            app.world().resource::<Shown>().order(),
            [3],
            "the filter keeps only the favorite"
        );

        app.world_mut().resource_mut::<Filters>().collection = None;
        app.update();
        name_collection(&mut app, "Warmups");
        activate::<CreateButton>(&mut app, |_| true);
        app.update();
        let lists = app.world().resource::<Collections>();
        assert_eq!(lists.names(), ["Warmups"]);
        assert!(
            lists.contains(&Pick::Named("Warmups".into()), "demo_maps"),
            "the selected game joins the new collection"
        );

        app.world_mut().resource_mut::<Filters>().collection =
            Some(Pick::Named("Warmups".into()));
        app.update();
        assert_eq!(
            app.world().resource::<Shown>().order(),
            [3],
            "the filter keeps the collection's games"
        );

        let on = {
            let world = app.world_mut();
            world
                .query::<(&Member, &Chip)>()
                .iter(world)
                .find(|(m, _)| m.0 == "Warmups")
                .map(|(_, chip)| chip.on)
        };
        assert_eq!(on, Some(true), "the new collection's chip is on");
        activate::<Member>(&mut app, |m| m.0 == "Warmups");
        app.update();
        assert!(
            !app.world()
                .resource::<Collections>()
                .contains(&Pick::Named("Warmups".into()), "demo_maps"),
            "the chip takes the game out"
        );
        app.update();
        assert!(
            app.world().resource::<Shown>().order().is_empty(),
            "the filter now finds nothing"
        );
    }

    /// The name field's text.
    fn field_text(app: &mut App) -> String {
        let world = app.world_mut();
        world
            .query_filtered::<&bevy::text::EditableText, With<collections::NameField>>()
            .iter(world)
            .map(|field| field.value().to_string())
            .next()
            .unwrap_or_default()
    }

    /// A named collection is renamed and deleted from its Collections menu, keeping its games
    /// and the favorites (#1495).
    #[test]
    fn a_collection_renames_and_deletes_from_the_collections_menu() {
        use collections::{Collections, CreateButton, CreateLabel, Pick, Renaming};
        use controls::{Control, Pick as Option};
        let mut app = screen();
        *app.world_mut().resource_mut::<Selection>() = Selection { game: 3, map: 0 };
        app.update();
        activate::<collections::Favorite>(&mut app, |_| true);
        name_collection(&mut app, "Warmups");
        activate::<CreateButton>(&mut app, |_| true);
        app.update();
        app.world_mut().resource_mut::<Filters>().collection =
            Some(Pick::Named("Warmups".into()));
        app.update();

        // Collections -> Rename "Warmups": the field holds the old name, the button says Rename.
        activate::<Control>(&mut app, |c| *c == Control::Collections);
        activate::<Option>(&mut app, |p| {
            matches!(p, Option::Rename(name) if name == "Warmups")
        });
        assert_eq!(
            app.world().resource::<Renaming>().0.as_deref(),
            Some("Warmups")
        );
        assert_eq!(field_text(&mut app), "Warmups");
        let label = {
            let world = app.world_mut();
            world
                .query_filtered::<&Text, With<CreateLabel>>()
                .iter(world)
                .map(|t| t.0.clone())
                .collect::<Vec<_>>()
        };
        assert!(label.iter().any(|l| l == "Rename"), "{label:?}");

        name_collection(&mut app, "Practice");
        activate::<CreateButton>(&mut app, |_| true);
        app.update();
        let lists = app.world().resource::<Collections>();
        assert_eq!(lists.names(), ["Practice"]);
        assert!(
            lists.contains(&Pick::Named("Practice".into()), "demo_maps"),
            "renaming kept the game"
        );
        assert!(app.world().resource::<Renaming>().0.is_none());
        assert_eq!(
            app.world().resource::<Filters>().collection,
            Some(Pick::Named("Practice".into())),
            "the filter follows the new name"
        );
        assert_eq!(app.world().resource::<Shown>().order(), [3]);

        // Collections -> Delete "Practice": the list goes, the games and the favorite stay.
        activate::<Control>(&mut app, |c| *c == Control::Collections);
        activate::<Option>(&mut app, |p| {
            matches!(p, Option::Delete(name) if name == "Practice")
        });
        app.update();
        let lists = app.world().resource::<Collections>();
        assert!(lists.names().is_empty());
        assert!(lists.is_favorite("demo_maps"), "the favorite stays");
        assert_eq!(
            app.world().resource::<Filters>().collection,
            None,
            "the filter falls back to Any"
        );
        assert_eq!(
            app.world().resource::<Shown>().order().len(),
            8,
            "the games are still in the library"
        );
        app.world_mut().resource_mut::<Filters>().collection = Some(Pick::Favorites);
        app.update();
        assert_eq!(
            app.world().resource::<Shown>().order(),
            [3],
            "the Favorites filter still finds the favorite"
        );
    }

    /// The Mods button opens the sheet; Sudden Death and Slow motion turn the header Unranked
    /// with the reason; Back puts the set back; Done keeps the picks on the Mods button (#497).
    #[test]
    fn the_mod_sheet_picks_mods_and_says_where_the_run_counts() {
        use crate::mods::RunMods;
        use mod_sheet::{Sheet, SheetButton};
        let mut app = screen();
        assert!(!app.world().resource::<Sheet>().open);
        activate::<SheetButton>(&mut app, |b| *b == SheetButton::Mods);
        assert!(app.world().resource::<Sheet>().open);
        app.update();
        assert!(header_words(&mut app).contains("RANKED"), "ranked with no mods");

        activate::<mod_sheet::Card>(&mut app, |c| c.0 == iwbtg_sim::mods::Mod::SuddenDeath);
        app.update();
        assert_eq!(app.world().resource::<RunMods>().0.codes(), "SD");
        let words = header_words(&mut app);
        assert!(words.contains("+ SD"), "{words}");

        activate::<mod_sheet::Card>(&mut app, |c| c.0 == iwbtg_sim::mods::Mod::SlowMotion);
        app.update();
        let words = header_words(&mut app);
        assert!(words.contains("UNRANKED"), "{words}");
        assert!(words.contains("Slow motion 75% is a practice assist"), "{words}");

        activate::<SheetButton>(&mut app, |b| *b == SheetButton::Back);
        assert_eq!(
            app.world().resource::<RunMods>().0,
            ModSet::NONE,
            "Back puts the set back"
        );
        assert!(!app.world().resource::<Sheet>().open);

        activate::<SheetButton>(&mut app, |b| *b == SheetButton::Mods);
        activate::<mod_sheet::Card>(&mut app, |c| c.0 == iwbtg_sim::mods::Mod::SuddenDeath);
        activate::<SheetButton>(&mut app, |b| *b == SheetButton::Done);
        app.update();
        assert_eq!(app.world().resource::<RunMods>().0.codes(), "SD");
        assert!(
            header_words(&mut app).contains("|SD|"),
            "the Mods button shows SD"
        );
    }

    /// Enter and a click start the picked map's id; a game that did not read has none (#880).
    #[test]
    fn the_picked_map_is_started_by_its_id() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/maps.pkg");
        let (info, _) = library::read(&path);
        assert_eq!(map_id(&info, 0).as_deref(), Some("medium"));
        assert_eq!(map_id(&info, 1).as_deref(), Some("hard"));
        assert_eq!(map_id(&info, 9), None);
    }

    /// A press outside an open dropdown closes it and its click does nothing else; with no
    /// menu open a click works as before (#899).
    #[test]
    fn a_click_outside_a_dropdown_only_closes_it() {
        use bevy::camera::NormalizedRenderTarget;
        use bevy::picking::backend::HitData;
        use bevy::picking::events::{Pointer, PointerClick, PointerPress};
        use bevy::picking::pointer::{Location, PointerButton, PointerId};
        use bevy::window::WindowRef;

        let mut app = screen();
        let pointer = || {
            Pointer::new(
                PointerId::Mouse,
                Location {
                    target: NormalizedRenderTarget::Window(
                        WindowRef::Entity(Entity::PLACEHOLDER)
                            .normalize(None)
                            .unwrap_or_else(|| unreachable!()),
                    ),
                    position: Vec2::ZERO,
                },
            )
        };
        let hit = || HitData::new(Entity::PLACEHOLDER, 0.0, None, None);
        let press = |app: &mut App, entity: Entity| {
            app.world_mut().trigger(PointerPress {
                entity,
                pointer: pointer(),
                button: PointerButton::Primary,
                hit: hit(),
                count: 1,
            });
        };
        let click = |app: &mut App, entity: Entity| {
            app.world_mut().trigger(PointerClick {
                entity,
                pointer: pointer(),
                button: PointerButton::Primary,
                hit: hit(),
                duration: std::time::Duration::ZERO,
                count: 1,
            });
        };
        let open_menus = |app: &mut App| {
            let world = app.world_mut();
            world.query::<&controls::Menu>().iter(world).count()
        };
        let (sort, game) = {
            let world = app.world_mut();
            let sort = world
                .query::<(Entity, &controls::Control)>()
                .iter(world)
                .find(|(_, c)| **c == controls::Control::Sort)
                .map(|(e, _)| e)
                .unwrap();
            let game = world
                .query::<(Entity, &list::GamePanel)>()
                .iter(world)
                .find(|(_, g)| g.0 == 3)
                .map(|(e, _)| e)
                .unwrap();
            (sort, game)
        };
        let before = *app.world().resource::<Selection>();
        assert_ne!(before.game, 3);

        app.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity: sort });
        app.update();
        assert_eq!(
            open_menus(&mut app),
            2,
            "the control opens its menu and a catcher"
        );

        // The catcher is the pickable thing under the pointer on bare backdrop (and over the
        // list), so a press there is a press on it.
        let catcher = {
            let world = app.world_mut();
            world
                .query_filtered::<Entity, With<controls::Catcher>>()
                .single(world)
                .expect("an open menu has a catcher")
        };
        press(&mut app, catcher);
        app.update();
        assert_eq!(
            open_menus(&mut app),
            0,
            "a press on the backdrop closes the menu and its catcher"
        );
        // The press ended on the catcher, which is gone: no click reaches the game beneath.
        assert_eq!(
            *app.world().resource::<Selection>(),
            before,
            "and selects no game"
        );

        app.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity: sort });
        app.update();
        assert_eq!(open_menus(&mut app), 2, "the menu and its catcher");
        app.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity: sort });
        app.update();
        assert_eq!(open_menus(&mut app), 0, "a second click on the control closes it");

        press(&mut app, game);
        click(&mut app, game);
        assert_eq!(
            app.world().resource::<Selection>().game,
            3,
            "the next click works as before"
        );
    }
}
