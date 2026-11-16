//! The placeholder select screen (`--select <folder>`): the packages in a folder, one per line.
//! Up and Down pick, Enter opens, Escape quits. The designed game select is #148 and later.

use std::path::{Path, PathBuf};

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;
use bevy::window::PrimaryWindow;

use super::AppScreen;
use crate::ui::backdrop::{Backdrop, dim};
use crate::ui::fonts::Type;
use crate::ui::notice;
use crate::ui::tokens;
use crate::ui::widgets::text;

/// The window's title on the select screen.
pub const TITLE: &str = "IWBTG Collection";
/// The layout is drawn for a window this tall, scaled to the real one.
const DESIGN_HEIGHT: f32 = 720.0;

/// The folder's packages and the one picked.
#[derive(Resource, Default)]
struct List {
    entries: Vec<PathBuf>,
    selected: usize,
    /// The rows' generation: rebuilt when it changes.
    drawn: Option<usize>,
}

#[derive(Component)]
struct Row;

/// The select screen: added by main.rs with the window (it needs the UI kit).
pub struct SelectPlugin(pub Option<PathBuf>);

impl Plugin for SelectPlugin {
    fn build(&self, app: &mut App) {
        let folder = self.0.clone();
        app.add_systems(
            OnEnter(AppScreen::Select),
            move |mut commands: Commands,
                  mut backdrop: ResMut<Backdrop>,
                  mut window: Query<&mut Window, With<PrimaryWindow>>| {
                let entries = folder.as_deref().map(packages).unwrap_or_default();
                spawn(&mut commands, folder.as_deref(), &entries);
                commands.insert_resource(List { entries, ..default() });
                backdrop.art = None;
                backdrop.dim = dim::SELECT;
                for mut w in &mut window {
                    w.title = TITLE.to_owned();
                }
            },
        )
        .add_systems(
            Update,
            (keys, rows, scale).chain().run_if(in_state(AppScreen::Select)),
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

fn name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

#[derive(Component)]
struct Rows;

fn spawn(commands: &mut Commands, folder: Option<&Path>, entries: &[PathBuf]) {
    let camera = commands
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
        .id();
    let where_ = folder.map_or_else(String::new, |f| f.display().to_string());
    let empty = entries.is_empty();
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(48.0)),
            row_gap: Val::Px(8.0),
            ..default()
        },
        UiTargetCamera(camera),
        DespawnOnExit(AppScreen::Select),
        children![
            text("Select a game", Type::condensed(40.0, 700), tokens::INK),
            text(&format!("Placeholder list: {where_}"), Type::LABEL, tokens::MUTED),
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    margin: UiRect::vertical(Val::Px(16.0)),
                    row_gap: Val::Px(4.0),
                    ..default()
                },
                Rows,
            ),
            text(
                if empty {
                    "No packages here (folders with a game.toml, .iwbz files)."
                } else {
                    "Up / Down pick · Enter plays · Escape quits"
                },
                Type::BODY,
                tokens::SOFT
            ),
        ],
    ));
}

/// Up and Down move, Enter opens the picked package, Escape quits.
fn keys(keys: Res<ButtonInput<KeyCode>>, mut list: ResMut<List>, mut commands: Commands) {
    let n = list.entries.len();
    if n > 0 {
        if keys.just_pressed(KeyCode::ArrowDown) {
            list.selected = (list.selected + 1) % n;
        }
        if keys.just_pressed(KeyCode::ArrowUp) {
            list.selected = (list.selected + n - 1) % n;
        }
        if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
            let path = list.entries[list.selected].clone();
            commands.queue(move |world: &mut World| {
                if let Err(e) = super::start(world, &path) {
                    eprintln!("{e}");
                    notice::post(notice::Kind::Drop, &e.to_string());
                }
            });
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        commands.write_message(AppExit::Success);
    }
}

/// Draws the rows again when the pick changes.
fn rows(
    mut commands: Commands,
    mut list: ResMut<List>,
    holder: Query<Entity, With<Rows>>,
    old: Query<Entity, With<Row>>,
) {
    if list.drawn == Some(list.selected) {
        return;
    }
    let Ok(holder) = holder.single() else { return };
    list.drawn = Some(list.selected);
    for e in &old {
        commands.entity(e).despawn();
    }
    for (i, path) in list.entries.iter().enumerate() {
        let picked = i == list.selected;
        let row = commands
            .spawn((
                Node {
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(if picked { tokens::ROW_YOU } else { tokens::ROW }),
                Row,
                children![text(
                    &name(path),
                    Type::BUTTON,
                    if picked { tokens::INK } else { tokens::MUTED }
                )],
            ))
            .id();
        commands.entity(holder).add_child(row);
    }
}

/// Scales the page with the window.
fn scale(window: Single<&Window, With<PrimaryWindow>>, mut ui_scale: ResMut<UiScale>) {
    let h = window.size().y;
    if h < 1.0 {
        return;
    }
    let s = (h / DESIGN_HEIGHT).clamp(0.5, 3.0);
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
        let names: Vec<String> = packages(&d).iter().map(|p| name(p)).collect();
        assert_eq!(names, ["a.iwbz", "b-game"]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
