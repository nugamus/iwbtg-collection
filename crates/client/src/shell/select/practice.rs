//! Game select's practice pickers (#39, docs/product.md, Practice): start a run at a stage, at
//! the game's save, or in any play room. The panel is opened by the left side's Practice button
//! and lists the picked map's stages (a stage whose trigger enters a room), the game's own save
//! when there is one, and every play room. Start stages the same package and map with an explicit
//! `StartPoint` (`shell::start_map`), and marks the run practice: it is never ranked and its
//! replay is not recorded.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use iwbtg_sim::def::{GameDef, RoomId, RoomRole};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::timing::TimingTrigger;

use super::AppScreen;
use super::library::Library;
use super::list::Selection;
use crate::ui::fonts::Type;
use crate::ui::shape::Shape;
use crate::ui::tokens;
use crate::ui::widgets::{ButtonKind, Chip, button, chip, chip_off, text};

/// The picker panel is open (the select screen's other keys are off).
#[derive(Resource, Default)]
pub(super) struct Sheet {
    pub open: bool,
}

/// The option picked in the open panel.
#[derive(Resource, Default)]
pub(super) struct Pick {
    pub start: Option<StartPoint>,
}

/// The left side's Practice button.
#[derive(Component)]
pub(super) struct OpenButton;

/// `--practice-preview`: the panel opens with the screen (a screenshot check).
#[derive(Resource, Default)]
pub(super) struct Preview(pub bool);

/// The panel is closed, so the select screen's own keys run.
pub(super) fn closed(sheet: Option<Res<Sheet>>) -> bool {
    !sheet.is_some_and(|s| s.open)
}

/// The open panel (the dim and everything in it).
#[derive(Component)]
pub(super) struct Panel;

/// The package and map a Start uses, on the panel's Start button.
#[derive(Component, Clone)]
pub(super) struct PanelGame {
    path: PathBuf,
    map: Option<String>,
}

/// One pickable start.
#[derive(Component, Clone)]
pub(super) struct OptionStart(pub(super) StartPoint);

/// The panel's Start button.
#[derive(Component)]
pub(super) struct StartButton;

/// The panel's Back button.
#[derive(Component)]
pub(super) struct BackButton;

/// The panel's options follow the pick: the picked one is on.
pub(super) fn chips(pick: Res<Pick>, mut chips: Query<(&OptionStart, &mut Chip)>) {
    if !pick.is_changed() {
        return;
    }
    for (option, mut chip) in &mut chips {
        let on = pick.start.as_ref() == Some(&option.0);
        if chip.on != on {
            chip.on = on;
        }
    }
}

/// The Practice button opens the panel for the selected game and map.
pub(super) fn open(
    activate: On<Activate>,
    pressed: Query<(), With<OpenButton>>,
    lib: Option<Res<Library>>,
    sel: Option<Res<Selection>>,
    sheet: ResMut<Sheet>,
    pick: ResMut<Pick>,
    cameras: Query<&UiTargetCamera>,
    commands: Commands,
) {
    if pressed.get(activate.entity).is_err() {
        return;
    }
    open_for(commands, &lib, &sel, sheet, pick, cameras.iter().next().map(|c| c.0));
}

/// `--practice-preview`: opens the panel once, on the screen's first update.
pub(super) fn preview(
    mut done: Local<bool>,
    preview: Option<Res<Preview>>,
    lib: Option<Res<Library>>,
    sel: Option<Res<Selection>>,
    sheet: ResMut<Sheet>,
    pick: ResMut<Pick>,
    cameras: Query<&UiTargetCamera>,
    commands: Commands,
) {
    if *done || !preview.is_some_and(|p| p.0) || sheet.open {
        return;
    }
    *done = true;
    open_for(commands, &lib, &sel, sheet, pick, cameras.iter().next().map(|c| c.0));
}

/// Opens the panel for the selected game and map (the button and the preview both do).
fn open_for(
    mut commands: Commands,
    lib: &Option<Res<Library>>,
    sel: &Option<Res<Selection>>,
    mut sheet: ResMut<Sheet>,
    mut pick: ResMut<Pick>,
    camera: Option<Entity>,
) {
    let (Some(lib), Some(sel)) = (lib, sel) else {
        return;
    };
    let Some(game) = lib.games.get(sel.game) else {
        return;
    };
    let map = game.maps.get(sel.map).map(|m| m.id.clone());
    // The picked package's rooms, stages and save: the list only keeps the strip it draws.
    let pkg = match crate::package::open(&game.path, false) {
        Ok(pkg) => pkg,
        Err(e) => {
            crate::ui::notice::post(crate::ui::notice::Kind::Drop, &e.to_string());
            return;
        }
    };
    pick.start = None;
    sheet.open = true;
    spawn(&mut commands, camera, &pkg.def, map, game.path.clone());
}

/// Closes the panel (Back, Escape, or after Start).
pub(super) fn close(commands: &mut Commands, panels: &Query<Entity, With<Panel>>) {
    for panel in panels {
        commands.entity(panel).despawn();
    }
}

/// Escape closes the panel before the screen's own keys.
pub(super) fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut sheet: ResMut<Sheet>,
    panels: Query<Entity, With<Panel>>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Escape) && sheet.open {
        sheet.open = false;
        close(&mut commands, &panels);
    }
}

/// A chip picks its start; Start stages the run from it.
#[allow(clippy::too_many_arguments, reason = "the panel's own observer")]
pub(super) fn on_activate(
    activate: On<Activate>,
    options: Query<&OptionStart>,
    starts: Query<&PanelGame, With<StartButton>>,
    backs: Query<(), With<BackButton>>,
    panels: Query<Entity, With<Panel>>,
    mut pick: ResMut<Pick>,
    mut sheet: ResMut<Sheet>,
    mut commands: Commands,
) {
    if let Ok(option) = options.get(activate.entity) {
        pick.start = Some(option.0.clone());
        return;
    }
    if backs.get(activate.entity).is_ok() {
        sheet.open = false;
        close(&mut commands, &panels);
        return;
    }
    let Ok(game) = starts.get(activate.entity) else {
        return;
    };
    let Some(start) = pick.start.clone() else {
        return;
    };
    let path = game.path.clone();
    let map = game.map.clone();
    sheet.open = false;
    close(&mut commands, &panels);
    commands.queue(move |world: &mut World| {
        if let Some(mut practice) = world.get_resource_mut::<crate::practice::Practice>() {
            practice.0 = true;
        }
        super::super::start_map(world, &path, map.as_deref(), Some(start));
    });
}

/// The panel: the title, the never-ranked line, the stage, save and room rows, and the footer.
fn spawn(
    commands: &mut Commands,
    camera: Option<Entity>,
    def: &GameDef,
    map: Option<String>,
    path: PathBuf,
) {
    let sections = sections(def, map.as_deref());
    let mut dim = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.62)),
        GlobalZIndex(20),
        Panel,
        DespawnOnExit(AppScreen::Select),
    ));
    if let Some(camera) = camera {
        dim.insert(UiTargetCamera(camera));
    }
    dim.with_children(|dim| {
        dim.spawn((
            Node {
                width: Val::Px(920.0),
                max_height: Val::Percent(86.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(14.0),
                padding: UiRect::all(Val::Px(30.0)),
                ..default()
            },
            Shape::fill(tokens::GLASS),
        ))
        .with_children(|panel| {
            panel.spawn(text("Practice", Type::condensed(40.0, 700), tokens::INK));
            panel.spawn(text(
                "Practice runs stay off the leaderboard.",
                Type::barlow(16.0, 500),
                tokens::ACCENT_LIGHT,
            ));
            for (title, options) in &sections {
                panel.spawn(text(title, Type::LABEL, tokens::MUTED));
                panel
                    .spawn(Node {
                        column_gap: Val::Px(8.0),
                        row_gap: Val::Px(8.0),
                        flex_wrap: FlexWrap::Wrap,
                        max_width: Val::Percent(100.0),
                        ..default()
                    })
                    .with_children(|row| {
                        for (label, start) in options {
                            match start {
                                Some(start) => {
                                    row.spawn((chip(label, false), OptionStart(start.clone())));
                                }
                                None => {
                                    row.spawn(chip_off(label));
                                }
                            }
                        }
                    });
            }
            panel
                .spawn(Node {
                    column_gap: Val::Px(10.0),
                    align_items: AlignItems::Center,
                    margin: UiRect::top(Val::Px(6.0)),
                    ..default()
                })
                .with_children(|footer| {
                    footer.spawn((
                        button("Start", ButtonKind::Primary),
                        StartButton,
                        PanelGame { path, map },
                    ));
                    footer.spawn((button("Back", ButtonKind::Secondary), BackButton));
                });
        });
    });
}

/// The panel's sections: the map's stages, the save and the start of the game, and every play
/// room. A stage whose trigger is not `enter_room` cannot be started at, so it is faded out.
fn sections(def: &GameDef, map: Option<&str>) -> Vec<(String, Vec<(String, Option<StartPoint>)>)> {
    let map_index = def.map_index(map).unwrap_or(0);
    let room = |id: RoomId| def.room(id).map(|r| r.name.clone()).unwrap_or_default();
    let stages: Vec<(String, Option<StartPoint>)> = def
        .map_timing(map_index)
        .stages
        .iter()
        .enumerate()
        .map(|(i, trigger)| match trigger {
            TimingTrigger::EnterRoom(id) => (
                format!("Stage {} — {}", i + 1, room(*id)),
                Some(StartPoint::Room {
                    room: *id,
                    difficulty: 0,
                }),
            ),
            _ => (format!("Stage {}", i + 1), None),
        })
        .collect();
    let save = crate::savefile::start_point(def);
    let saved = matches!(save, StartPoint::FromSave(_)).then_some(save);
    let beginning = Some(StartPoint::NewGame { difficulty: 0 });
    let rooms: Vec<(String, Option<StartPoint>)> = def
        .rooms
        .iter()
        .enumerate()
        .filter(|(_, r)| r.role == RoomRole::Play)
        .map(|(i, r)| {
            let id = u32::try_from(i).unwrap_or(0);
            (
                r.name.clone(),
                Some(StartPoint::Room {
                    room: RoomId(id),
                    difficulty: 0,
                }),
            )
        })
        .collect();
    vec![
        ("STAGE".to_owned(), stages),
        (
            "SAVE".to_owned(),
            vec![
                ("Your save".to_owned(), saved),
                ("Start of the game".to_owned(), beginning),
            ],
        ),
        ("ROOM".to_owned(), rooms),
    ]
}
