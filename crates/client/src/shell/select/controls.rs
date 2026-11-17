//! The controls above the game select list (#150, `Main.dc.html`, top right): the search field
//! and Collections on the first row; Group, Sort, the Cleared and Uncleared chips, and the
//! Status, Creator and Year dropdowns on the second. They write [`Filters`]; the list follows.
//!
//! Wired now: search (title, creator), Group and Sort (Game/Title, Creator, Cleared; Stars once
//! #111 rates maps), the chips and Creator. Shown but faded until their data exists: the
//! Year, Tag and Status options, the Status and Year dropdowns (#805) and Collections (#105).

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui_widgets::Activate;

use super::filters::{self, Clears, Field, Filters};
use super::library::Library;
use crate::ui::fonts::Type;
use crate::ui::tokens;
use crate::ui::widgets::{
    ButtonKind, Chip, ControlValue, SEARCH_HEIGHT, SearchField, button_off, chip, chip_off,
    control, control_off, menu_panel, search_field, text,
};

/// The controls' top and right edge, and their width (`Main.dc.html`: x 1040 to 1880 at 1920
/// wide, so 40 px from the right).
const TOP: f32 = 24.0;
const RIGHT: f32 = 40.0;
const WIDTH: f32 = 840.0;
/// A dropdown opens this far below its control's top (34 px control, 6 px gap).
const MENU_TOP: f32 = 40.0;

/// Which control an entity is.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Control {
    Group,
    Sort,
    Creator,
    Cleared,
    Uncleared,
}

/// The holder of a dropdown control and, while open, its menu.
#[derive(Component)]
pub(super) struct Dropdown;

/// An open dropdown menu.
#[derive(Component)]
pub(super) struct Menu;

/// What a menu option sets.
#[derive(Component, Clone, Debug, PartialEq)]
pub(super) enum Pick {
    Group(Field),
    Sort(Field),
    Creator(Option<String>),
}

/// The control's value as it reads (`Stars ▾`). Group reads without the arrow, as on the
/// artboard.
fn value(control: Control, f: &Filters) -> String {
    match control {
        Control::Group => f.group.label().to_owned(),
        Control::Sort => format!("{} ▾", f.sort.label()),
        Control::Creator => format!("{} ▾", f.creator.as_deref().unwrap_or("Any")),
        Control::Cleared | Control::Uncleared => String::new(),
    }
}

/// Spawns the controls for `lib` with `f` set, under `camera`.
pub(super) fn spawn(commands: &mut Commands, camera: Entity, lib: &Library, f: &Filters) {
    let placeholder = format!("Search {} games: title, creator", lib.games.len());
    let dropdown = |control: Control, key: &str| {
        (
            Node::default(),
            Dropdown,
            children![(self::control(key, &value(control, f)), control)],
        )
    };
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(TOP),
            right: Val::Px(RIGHT),
            width: Val::Px(WIDTH),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(12.0),
            ..default()
        },
        GlobalZIndex(5),
        UiTargetCamera(camera),
        DespawnOnExit(super::AppScreen::Select),
        children![
            (
                Node {
                    column_gap: Val::Px(10.0),
                    ..default()
                },
                children![
                    search_field(&placeholder, &f.search),
                    // Collections: #105.
                    button_off(
                        ButtonKind::Control,
                        SEARCH_HEIGHT,
                        18.0,
                        text("Collections ▾", Type::barlow(15.0, 600), tokens::INK),
                    ),
                ],
            ),
            (
                Node {
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                },
                children![
                    dropdown(Control::Group, "Group"),
                    dropdown(Control::Sort, "Sort"),
                    (chip("Cleared", f.clears == Clears::Cleared), Control::Cleared),
                    (
                        chip("Uncleared", f.clears == Clears::Uncleared),
                        Control::Uncleared
                    ),
                    // Status and Year: #805.
                    control_off("Status", "Any ▾"),
                    dropdown(Control::Creator, "Creator"),
                    control_off("Year", "Any ▾"),
                ],
            ),
        ],
    ));
}

/// The options of `control`'s menu: (label, what it sets, picked now, can be picked).
fn options(control: Control, lib: &Library, f: &Filters) -> Vec<(String, Pick, bool, bool)> {
    let fields = |list: &[Field], now: Field, make: fn(Field) -> Pick| {
        list.iter()
            .map(|&field| {
                (
                    field.label().to_owned(),
                    make(field),
                    field == now,
                    field.ready(&lib.games),
                )
            })
            .collect()
    };
    match control {
        Control::Group => fields(&Field::GROUPS, f.group, Pick::Group),
        Control::Sort => fields(&Field::SORTS, f.sort, Pick::Sort),
        Control::Creator => std::iter::once(None)
            .chain(filters::creators(&lib.games).into_iter().map(Some))
            .map(|c| {
                let label = c.clone().unwrap_or_else(|| "Any".to_owned());
                let on = f.creator == c;
                (label, Pick::Creator(c), on, true)
            })
            .collect(),
        Control::Cleared | Control::Uncleared => Vec::new(),
    }
}

/// A click (or Enter, Space) on a control: a chip flips its filter, a dropdown opens or closes
/// its menu, an option sets its filter and closes the menu. The keyboard focus goes back to the
/// search field, so typing keeps searching.
#[allow(clippy::too_many_arguments, reason = "one observer for every control")]
pub(super) fn on_activate(
    activate: On<Activate>,
    controls: Query<(&Control, &ChildOf)>,
    picks: Query<&Pick>,
    menus: Query<Entity, With<Menu>>,
    dropdowns: Query<Option<&Children>, With<Dropdown>>,
    search: Query<Entity, With<SearchField>>,
    lib: Option<Res<Library>>,
    filters: Option<ResMut<Filters>>,
    mut focus: ResMut<InputFocus>,
    mut commands: Commands,
) {
    let (Some(lib), Some(mut f)) = (lib, filters) else {
        return;
    };
    let close = |commands: &mut Commands| {
        for menu in &menus {
            commands.entity(menu).despawn();
        }
    };
    if let Ok((&control, parent)) = controls.get(activate.entity) {
        match control {
            Control::Cleared => f.clears = f.clears.toggle(Clears::Cleared),
            Control::Uncleared => f.clears = f.clears.toggle(Clears::Uncleared),
            _ => {
                let open = dropdowns
                    .get(parent.parent())
                    .ok()
                    .flatten()
                    .is_some_and(|kids| kids.iter().any(|k| menus.contains(k)));
                close(&mut commands);
                if !open {
                    let menu = commands
                        .spawn((menu_panel(), Menu))
                        .insert(Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(MENU_TOP),
                            left: Val::Px(0.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::FlexStart,
                            row_gap: Val::Px(6.0),
                            padding: UiRect::all(Val::Px(12.0)),
                            ..default()
                        })
                        .with_children(|m| {
                            for (label, pick, on, ready) in options(control, &lib, &f) {
                                if ready {
                                    m.spawn((chip(&label, on), pick));
                                } else {
                                    m.spawn(chip_off(&label));
                                }
                            }
                        })
                        .id();
                    commands.entity(parent.parent()).add_child(menu);
                }
            }
        }
    } else if let Ok(pick) = picks.get(activate.entity) {
        match pick.clone() {
            Pick::Group(field) => f.group = field,
            Pick::Sort(field) => f.sort = field,
            Pick::Creator(c) => f.creator = c,
        }
        close(&mut commands);
    } else {
        return;
    }
    if let Ok(search) = search.single() {
        focus.set(search, FocusCause::Navigated);
    }
}

/// Closes an open dropdown; true if one was open (Escape closes it before anything else).
pub(super) fn close_menus(commands: &mut Commands, menus: &Query<Entity, With<Menu>>) -> bool {
    let mut any = false;
    for menu in menus {
        commands.entity(menu).despawn();
        any = true;
    }
    any
}

/// The search field's text into [`Filters::search`].
pub(super) fn search(fields: Query<&EditableText, With<SearchField>>, mut f: ResMut<Filters>) {
    for field in &fields {
        let text = field.value().to_string();
        if f.search != text {
            f.search = text;
        }
    }
}

/// The controls follow the filters: each dropdown reads its value, each chip is on or off.
pub(super) fn labels(
    f: Res<Filters>,
    mut controls: Query<(Entity, &Control, Option<&mut Chip>)>,
    children: Query<&Children>,
    mut values: Query<&mut Text, With<ControlValue>>,
) {
    if !f.is_changed() {
        return;
    }
    for (entity, &control, chip) in &mut controls {
        match (control, chip) {
            (Control::Cleared, Some(mut chip)) => chip.on = f.clears == Clears::Cleared,
            (Control::Uncleared, Some(mut chip)) => chip.on = f.clears == Clears::Uncleared,
            _ => {
                let want = value(control, &f);
                for part in children.iter_descendants(entity) {
                    if let Ok(mut t) = values.get_mut(part)
                        && t.0 != want
                    {
                        t.0.clone_from(&want);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_controls_read_the_artboards_way() {
        let f = Filters {
            sort: Field::Stars,
            ..default()
        };
        assert_eq!(value(Control::Group, &f), "Game");
        assert_eq!(value(Control::Sort, &f), "Stars ▾");
        assert_eq!(value(Control::Creator, &f), "Any ▾");
        let f = Filters {
            creator: Some("Kira".into()),
            ..default()
        };
        assert_eq!(value(Control::Creator, &f), "Kira ▾");
    }
}
