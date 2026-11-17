//! The select screen (`--select <folder>`): the packages in a folder as game select's list
//! (#149, `select/list.rs`): a panel per game with its key art, its maps under the selected one.
//! Up and Down pick a map, Left and Right a game, a click picks either; Enter (or a click on the
//! selected map) plays, Escape quits. Above the list, search, group, sort and filters (#150,
//! `select/controls.rs`, `select/filters.rs`) pick the rows it shows. The left side is a
//! placeholder until #148: the picked game and map, and the keys.

use std::path::{Path, PathBuf};

use bevy::camera::visibility::RenderLayers;
use bevy::picking::events::PointerClick;
use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;
use bevy::window::PrimaryWindow;
use iwbtg_sim::mods::ModSet;

use super::AppScreen;
use crate::stats::Book;
use crate::ui::backdrop::{Backdrop, dim};
use crate::ui::fonts::Type;
use crate::ui::notice;
use crate::ui::tokens;
use crate::ui::widgets::text;

pub mod controls;
pub mod filters;
pub mod footer;
pub mod library;
pub mod list;
pub mod mod_sheet;

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

/// The select screen: added by main.rs with the window (it needs the UI kit). The flag is
/// `--mod-select-preview`: the mod select sheet opens with the screen.
pub struct SelectPlugin(pub Option<PathBuf>, pub bool);

impl Plugin for SelectPlugin {
    fn build(&self, app: &mut App) {
        let folder = self.0.clone();
        let preview = self.1;
        app.init_resource::<ListDim>()
            .init_resource::<Filters>()
            .init_resource::<Shown>()
            .init_resource::<mod_sheet::Sheet>()
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
                      filters: Res<Filters>,
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
                    if preview {
                        sheet.open_with(mods);
                    }
                    let camera = camera(&mut commands);
                    let sel = sel.map(|s| *s).unwrap_or_default();
                    let same = |l: &Library| {
                        l.folder.as_deref() == folder.as_deref()
                            && l.games.iter().map(|g| &g.path).eq(paths.iter())
                    };
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
            .add_observer(controls::on_activate)
            .add_observer(mod_sheet::on_activate)
            .add_observer(mod_sheet::on_option)
            .add_systems(
                Update,
                (
                    controls::search,
                    list::refilter,
                    controls::labels,
                    keys.run_if(mod_sheet::closed),
                    mod_sheet::keys,
                    mod_sheet::slide,
                    mod_sheet::follow,
                    mod_sheet::auto_retry,
                    records,
                    list::looks,
                    list::arrange,
                    list::settle,
                    list::dim,
                    picked,
                    scale,
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
fn enter(
    commands: &mut Commands,
    camera: Entity,
    folder: Option<&Path>,
    lib: &Library,
    sel: Selection,
    filters: &Filters,
    shown: &mut Shown,
) -> Selection {
    shown.rows = filters::rows(&lib.games, &lib.records, filters);
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
        "No packages here (folders with a game.toml, .iwbz files)."
    } else {
        "Type to search · ↑ ↓ ← → pick · Enter plays · Esc clears, quits"
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
            text(hint, Type::BODY, tokens::SOFT),
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

/// Starts the package at `path` on map `map` (`None`: its first); one that does not open shows the NOT OPENED notice.
fn play(commands: &mut Commands, path: PathBuf, map: Option<String>) {
    commands.queue(move |world: &mut World| {
        if let Err(e) = super::start_map(world, &path, map.as_deref()) {
            eprintln!("{e}");
            notice::post(notice::Kind::Drop, &e.to_string());
        }
    });
}

/// The arrows move the selection among the games shown ([`list::step`]; Left and Right move
/// the search field's cursor instead while it holds text), Enter plays the selected map.
/// Escape closes an open dropdown, else clears the search, else quits.
#[allow(clippy::too_many_arguments, reason = "the screen's keys")]
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    lib: Res<Library>,
    shown: Res<Shown>,
    focus: Res<bevy::input_focus::InputFocus>,
    buttons: Query<(), With<bevy::ui_widgets::Button>>,
    menus: Query<Entity, With<controls::Menu>>,
    mut search: Query<&mut bevy::text::EditableText, With<crate::ui::widgets::SearchField>>,
    mut sel: ResMut<Selection>,
    mut commands: Commands,
) {
    let maps: Vec<usize> = lib.games.iter().map(|g| g.maps.len()).collect();
    let order = shown.order();
    let typing = search.iter().any(|s| !s.value().to_string().is_empty());
    for key in [
        KeyCode::ArrowDown,
        KeyCode::ArrowUp,
        KeyCode::ArrowRight,
        KeyCode::ArrowLeft,
    ] {
        let sideways = matches!(key, KeyCode::ArrowLeft | KeyCode::ArrowRight);
        if keys.just_pressed(key) && !(sideways && typing) {
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
        && order.contains(&sel.game)
        && let Some(game) = lib.games.get(sel.game)
    {
        play(&mut commands, game.path.clone(), map_id(game, sel.map));
    }
    if keys.just_pressed(KeyCode::Escape) && !controls::close_menus(&mut commands, &menus) {
        if typing {
            for mut s in &mut search {
                s.clear();
            }
        } else {
            commands.write_message(AppExit::Success);
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
                play(&mut commands, game.path.clone(), map_id(game, map.1));
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
            .init_state::<AppScreen>()
            .add_plugins(SelectPlugin(Some(fixtures), false));
        app.update();
        app.update();
        app
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
        assert_eq!(shown.len(), 6, "every fixture is listed");
        let before = targets(&mut app);
        assert!(before.iter().all(|t| (t.2 - 1.0).abs() < 1e-6));

        type_in(&mut app, "FIXTURE");
        app.update();
        // clear.pkg is "Clear fixture" (1), maps.pkg "Maps fixture" (3).
        assert_eq!(app.world().resource::<Shown>().order(), [1, 3]);
        let after = targets(&mut app);
        for (g, y, opacity) in &after {
            let shown = [1, 3].contains(g);
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
        assert_eq!(rows[0], filters::Row::Header("Not cleared".into(), 6));
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
            .map(|(e, (t, span))| (e, t.map_or_else(|| span.map_or_else(String::new, |s| s.0.clone()), |t| t.0.clone())))
            .collect();
        texts.sort_by_key(|t| t.0);
        texts.into_iter().map(|t| t.1).collect::<Vec<_>>().join("|")
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
        assert_eq!(app.world().resource::<RunMods>().0, ModSet::NONE, "Back puts the set back");
        assert!(!app.world().resource::<Sheet>().open);

        activate::<SheetButton>(&mut app, |b| *b == SheetButton::Mods);
        activate::<mod_sheet::Card>(&mut app, |c| c.0 == iwbtg_sim::mods::Mod::SuddenDeath);
        activate::<SheetButton>(&mut app, |b| *b == SheetButton::Done);
        app.update();
        assert_eq!(app.world().resource::<RunMods>().0.codes(), "SD");
        assert!(header_words(&mut app).contains("|SD|"), "the Mods button shows SD");
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
}
