//! The select screen (`--select <folder>`): the packages in a folder as game select's list
//! (#149, `select/list.rs`): a panel per game with its key art, its maps under the selected one.
//! Up and Down pick a map, Left and Right a game, a click picks either; Enter (or a click on the
//! selected map) plays, Escape quits. The left side is a placeholder until #148: the picked game
//! and map, and the keys.

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

pub mod library;
pub mod list;

use library::{Library, file_name};
use list::{GamePanel, ListDim, MapPanel, Selection};

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

/// The select screen: added by main.rs with the window (it needs the UI kit).
pub struct SelectPlugin(pub Option<PathBuf>);

impl Plugin for SelectPlugin {
    fn build(&self, app: &mut App) {
        let folder = self.0.clone();
        app.init_resource::<ListDim>()
            .add_systems(
                OnEnter(AppScreen::Select),
                move |mut commands: Commands,
                      mut images: ResMut<Assets<Image>>,
                      lib: Option<ResMut<Library>>,
                      sel: Option<Res<Selection>>,
                      stats: Option<Res<crate::stats::Stats>>,
                      mods: Option<Res<crate::mods::RunMods>>,
                      mut backdrop: ResMut<Backdrop>,
                      mut window: Query<&mut Window, With<PrimaryWindow>>| {
                    let paths = folder.as_deref().map(packages).unwrap_or_default();
                    let empty = Book::default();
                    let book = stats.as_ref().map_or(&empty, |s| &s.book);
                    let mods = mods.map_or(ModSet::NONE, |m| m.0);
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
                            enter(&mut commands, camera, folder.as_deref(), lib, sel)
                        }
                        _ => {
                            let mut lib = Library::load(folder.as_deref(), &paths, &mut images);
                            lib.refresh(book, mods);
                            let sel = enter(&mut commands, camera, folder.as_deref(), &lib, sel);
                            commands.insert_resource(lib);
                            sel
                        }
                    };
                    commands.insert_resource(sel);
                    backdrop.art = None;
                    backdrop.dim = dim::SELECT;
                    for mut w in &mut window {
                        w.title = TITLE.to_owned();
                    }
                },
            )
            .add_observer(on_click)
            .add_systems(
                Update,
                (keys, list::looks, list::dim, picked, scale)
                    .chain()
                    .run_if(in_state(AppScreen::Select)),
            );
    }
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

/// Spawns the page for `lib` and returns the selection, kept inside the list.
fn enter(
    commands: &mut Commands,
    camera: Entity,
    folder: Option<&Path>,
    lib: &Library,
    sel: Selection,
) -> Selection {
    let game = sel.game.min(lib.games.len().saturating_sub(1));
    let maps = lib.games.get(game).map_or(0, |g| g.maps.len());
    let sel = Selection {
        game,
        map: sel.map.min(maps.saturating_sub(1)),
    };
    let where_ = folder.map_or_else(String::new, |f| f.display().to_string());
    let hint = if lib.games.is_empty() {
        "No packages here (folders with a game.toml, .iwbz files)."
    } else {
        "↑ ↓ map · ← → game · Enter plays · Esc quits"
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
    list::spawn(commands, camera, lib, sel);
    sel
}

/// Starts the package at `path`; one that does not open shows the NOT OPENED notice.
fn play(commands: &mut Commands, path: PathBuf) {
    commands.queue(move |world: &mut World| {
        if let Err(e) = super::start(world, &path) {
            eprintln!("{e}");
            notice::post(notice::Kind::Drop, &e.to_string());
        }
    });
}

/// The arrows move the selection ([`list::step`]), Enter plays the selected game, Escape quits.
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    lib: Res<Library>,
    mut sel: ResMut<Selection>,
    mut commands: Commands,
) {
    let maps: Vec<usize> = lib.games.iter().map(|g| g.maps.len()).collect();
    for key in [
        KeyCode::ArrowDown,
        KeyCode::ArrowUp,
        KeyCode::ArrowRight,
        KeyCode::ArrowLeft,
    ] {
        if keys.just_pressed(key) {
            let next = list::step(&maps, *sel, key);
            if next != *sel {
                *sel = next;
            }
        }
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter])
        && let Some(game) = lib.games.get(sel.game)
    {
        play(&mut commands, game.path.clone());
    }
    if keys.just_pressed(KeyCode::Escape) {
        commands.write_message(AppExit::Success);
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
                play(&mut commands, game.path.clone());
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
}
