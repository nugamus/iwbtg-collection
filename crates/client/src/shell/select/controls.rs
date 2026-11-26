//! The controls above the game select list (#150, `Main.dc.html`, top right): the search field
//! and Collections on the first row; Group, Sort, the Cleared and Uncleared chips, and the
//! Status, Creator and Year dropdowns on the second. They write [`Filters`]; the list follows.
//!
//! Wired now: search (title, creator), Group and Sort (Game/Title, Creator, Cleared; Stars once
//! #111 rates maps), the chips, Creator and Collections (Any, Favorites and each named
//! collection, #105). Shown but faded until their data exists: the Year, Tag and Status
//! options and the Status and Year dropdowns (#805).

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::picking::events::PointerPress;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui_widgets::Activate;

use super::collections::{self, Collections};
use super::filters::{self, Clears, Field, Filters};
use super::library::Library;
use crate::ui::fonts::Type;
use crate::ui::tokens;
use crate::ui::widgets::{
    ButtonKind, Chip, ControlValue, SEARCH_HEIGHT, SearchField, button_with, chip, chip_off,
    control, control_off, menu_panel, search_field, text,
};

/// The controls' top and right edge, and their width (`Main.dc.html`: x 1040 to 1880 at 1920
/// wide, so 40 px from the right; m5 A3 moves them down under the toolbar).
const TOP: f32 = 24.0 + super::UNDER_TOOLBAR;
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
    Collections,
    Cleared,
    Uncleared,
}

/// The holder of a dropdown control and, while open, its menu.
#[derive(Component)]
pub(super) struct Dropdown;

/// An open dropdown menu.
#[derive(Component)]
pub(super) struct Menu;

/// The full-screen click-catcher under an open menu: a press on it closes the menu and, as
/// the press lands on it and not on what is beneath, that thing gets no click (#899).
#[derive(Component)]
pub(super) struct Catcher;

/// Closes every open menu (and its catcher) on a press on the catcher.
fn close_on_press(_: On<PointerPress>, menus: Query<Entity, With<Menu>>, mut commands: Commands) {
    for menu in &menus {
        commands.entity(menu).despawn();
    }
}

/// What a menu option sets.
#[derive(Component, Clone, Debug, PartialEq)]
pub(super) enum Pick {
    Group(Field),
    Sort(Field),
    Creator(Option<String>),
    Collection(Option<collections::Pick>),
    /// Rename a named collection: the name goes into the left side's field (#1495).
    Rename(String),
    /// Delete a named collection; its games stay in the library.
    Delete(String),
}

/// The control's value as it reads (`Stars ▾`). Group reads without the arrow, as on the
/// artboard.
fn value(control: Control, f: &Filters) -> String {
    match control {
        Control::Group => f.group.label().to_owned(),
        Control::Sort => format!("{} ▾", f.sort.label()),
        Control::Creator => format!("{} ▾", f.creator.as_deref().unwrap_or("Any")),
        Control::Collections => f
            .collection
            .as_ref()
            .map_or_else(|| "Collections ▾".to_owned(), |p| format!("{} ▾", p.label())),
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
                    (
                        Node {
                            width: Val::Px(150.0),
                            flex_direction: FlexDirection::Column,
                            flex_shrink: 0.0,
                            ..default()
                        },
                        Dropdown,
                        children![(
                            button_with(
                                ButtonKind::Control,
                                SEARCH_HEIGHT,
                                18.0,
                                (
                                    text(
                                        &value(Control::Collections, f),
                                        Type::barlow(15.0, 600),
                                        tokens::INK
                                    ),
                                    ControlValue
                                )
                            ),
                            Control::Collections
                        )]
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
fn options(
    control: Control,
    lib: &Library,
    lists: &Collections,
    f: &Filters,
) -> Vec<(String, Pick, bool, bool)> {
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
        Control::Collections => {
            let mut out = vec![(
                "Any".to_owned(),
                Pick::Collection(None),
                f.collection.is_none(),
                true,
            )];
            let favorites = collections::Pick::Favorites;
            out.push((
                format!("★ {}", favorites.label()),
                Pick::Collection(Some(favorites.clone())),
                f.collection.as_ref() == Some(&favorites),
                true,
            ));
            for name in lists.names() {
                let pick = collections::Pick::Named(name.clone());
                let on = f.collection.as_ref() == Some(&pick);
                out.push((name, Pick::Collection(Some(pick)), on, true));
            }
            // The picked named collection can be managed from its own menu (#1495).
            if let Some(collections::Pick::Named(name)) = &f.collection {
                out.push((format!("Rename \"{name}\""), Pick::Rename(name.clone()), false, true));
                out.push((format!("Delete \"{name}\""), Pick::Delete(name.clone()), false, true));
            }
            out
        }
        Control::Cleared | Control::Uncleared => Vec::new(),
    }
}

/// A click (or Enter, Space) on a control: a chip flips its filter, a dropdown opens or closes
/// its menu, an option sets its filter and closes the menu. Rename hands the collection's name
/// to the left side's field and focuses it; Delete drops the collection, and a filter on it goes
/// back to Any. Otherwise the keyboard focus goes back to the search field, so typing keeps
/// searching.
#[allow(clippy::too_many_arguments, reason = "one observer for every control")]
pub(super) fn on_activate(
    activate: On<Activate>,
    controls: Query<(&Control, &ChildOf)>,
    picks: Query<&Pick>,
    menus: Query<Entity, With<Menu>>,
    cameras: Query<&UiTargetCamera>,
    dropdowns: Query<Option<&Children>, With<Dropdown>>,
    search: Query<Entity, With<SearchField>>,
    mut fields: Query<&mut EditableText, With<collections::NameField>>,
    names: Query<Entity, With<collections::NameField>>,
    lib: Option<Res<Library>>,
    lists: Option<ResMut<Collections>>,
    mut renaming: Option<ResMut<collections::Renaming>>,
    filters: Option<ResMut<Filters>>,
    mut focus: ResMut<InputFocus>,
    mut commands: Commands,
) {
    let (Some(lib), Some(mut lists), Some(mut f)) = (lib, lists, filters) else {
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
                    // The Collections button is the search row's height, so its menu starts below it.
                    let top = if control == Control::Collections {
                        SEARCH_HEIGHT + 6.0
                    } else {
                        MENU_TOP
                    };
                    let menu = commands
                        .spawn((menu_panel(), Menu))
                        .insert(Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(top),
                            left: Val::Px(0.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::FlexStart,
                            row_gap: Val::Px(6.0),
                            padding: UiRect::all(Val::Px(12.0)),
                            ..default()
                        })
                        .with_children(|m| {
                            for (label, pick, on, ready) in options(control, &lib, &lists, &f) {
                                if ready {
                                    m.spawn((chip(&label, on), pick));
                                } else {
                                    m.spawn(chip_off(&label));
                                }
                            }
                        })
                        .id();
                    commands.entity(parent.parent()).add_child(menu);
                    if let Some(camera) = cameras.iter().next() {
                        commands
                            .spawn((
                                Node {
                                    position_type: PositionType::Absolute,
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                // Over the controls (5), under the menu (20).
                                GlobalZIndex(10),
                                UiTargetCamera(camera.0),
                                Menu,
                                Catcher,
                            ))
                            .observe(close_on_press);
                    }
                }
            }
        }
    } else if let Ok(pick) = picks.get(activate.entity) {
        let mut refocus = true;
        match pick.clone() {
            Pick::Group(field) => f.group = field,
            Pick::Sort(field) => f.sort = field,
            Pick::Creator(c) => f.creator = c,
            Pick::Collection(c) => f.collection = c,
            Pick::Rename(name) => {
                if let Some(renaming) = renaming.as_mut() {
                    renaming.0 = Some(name.clone());
                }
                if let Ok(mut field) = fields.single_mut() {
                    *field = EditableText::new(&name);
                }
                if let Ok(entity) = names.single() {
                    focus.set(entity, FocusCause::Navigated);
                    refocus = false;
                }
            }
            Pick::Delete(name) => {
                if let Some(renaming) = renaming.as_mut()
                    && renaming.0.as_deref() == Some(name.as_str())
                {
                    renaming.0 = None;
                }
                lists.delete(&name);
                if f.collection.as_ref() == Some(&collections::Pick::Named(name)) {
                    f.collection = None;
                }
            }
        }
        close(&mut commands);
        if refocus && let Ok(search) = search.single() {
            focus.set(search, FocusCause::Navigated);
        }
        return;
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
        assert_eq!(value(Control::Collections, &f), "Collections ▾");
        let f = Filters {
            collection: Some(collections::Pick::Favorites),
            ..default()
        };
        assert_eq!(value(Control::Collections, &f), "Favorites ▾");
        let f = Filters {
            collection: Some(collections::Pick::Named("Warmups".into())),
            ..default()
        };
        assert_eq!(value(Control::Collections, &f), "Warmups ▾");
    }

    /// The Collections menu is Any, the favorites list and each named collection (#105).
    #[test]
    fn the_collections_menu_lists_favorites_and_each_collection() {
        let mut images = Assets::<Image>::default();
        let lib = Library::load(None, &[], &mut images);
        let mut lists = Collections::default();
        lists.create("Warmups", None).unwrap();
        let menu = options(Control::Collections, &lib, &lists, &Filters::default());
        let labels: Vec<&str> = menu.iter().map(|(label, ..)| label.as_str()).collect();
        assert_eq!(labels, ["Any", "★ Favorites", "Warmups"]);
        assert!(menu[0].2, "Any is picked by default");
        assert_eq!(
            menu[1].1,
            Pick::Collection(Some(collections::Pick::Favorites))
        );
        assert_eq!(menu[2].1, Pick::Collection(Some(
            collections::Pick::Named("Warmups".into())
        )));

        // While a named collection is the pick, its menu can rename or delete it (#1495).
        let picked = Filters {
            collection: Some(collections::Pick::Named("Warmups".into())),
            ..default()
        };
        let menu = options(Control::Collections, &lib, &lists, &picked);
        let labels: Vec<&str> = menu.iter().map(|(label, ..)| label.as_str()).collect();
        assert_eq!(
            labels,
            ["Any", "★ Favorites", "Warmups", "Rename \"Warmups\"", "Delete \"Warmups\""]
        );
        assert_eq!(menu[3].1, Pick::Rename("Warmups".into()));
        assert_eq!(menu[4].1, Pick::Delete("Warmups".into()));

        // The favorites list itself is not a named collection and has no such actions.
        let favorites = Filters {
            collection: Some(collections::Pick::Favorites),
            ..default()
        };
        let menu = options(Control::Collections, &lib, &lists, &favorites);
        assert_eq!(menu.len(), 3, "Any, Favorites and the named collection");
    }
}
