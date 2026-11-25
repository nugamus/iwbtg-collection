//! Game select's personal lists (#105): favorites and named collections. The left side's
//! Favorite toggle and collection chips write them; the Collections filter reads them
//! (`filters.rs`, `controls.rs`). Kept in `collections.toml` in the data area of `storage.rs`,
//! one entry per game by package id, so a moved or renamed file keeps its lists.
//!
//! The file is readable TOML:
//!
//! ```toml
//! version = 1
//! favorites = ["rely_the_needle"]
//!
//! [[collections]]
//! name = "Bosses"
//! games = ["rely_the_needle", "kaenbyou"]
//! ```
//!
//! Reading is forgiving like the settings: a file this build cannot read is kept aside as
//! `collections.broken.toml` and the session starts with no lists. Nothing here touches the sim.

use bevy::input_focus::InputFocus;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui_widgets::{Activate, TextInput};

use super::library::Library;
use super::list::Selection;
use crate::storage::{self, Store};
use crate::ui::fonts::Type;
use crate::ui::shape::Shape;
use crate::ui::tokens;
use crate::ui::widgets::{ButtonKind, Chip, button, button_with, chip, text};

/// The file in the data area.
pub(crate) const FILE: &str = "collections.toml";
/// Where an unreadable file is moved, so a new list never overwrites it.
const BROKEN_FILE: &str = "collections.broken.toml";
/// The layout version this build writes; a newer version is read as far as it fits.
pub const VERSION: i64 = 1;
/// The longest collection name, in characters.
pub const MAX_NAME_CHARS: usize = 32;
/// The built-in list; a named collection may not use this name.
pub const FAVORITES: &str = "Favorites";

/// One named collection (`[[collections]]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    pub name: String,
    /// Package ids, in the order they were added.
    pub games: Vec<String>,
}

/// What the Collections filter picks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Favorites,
    Named(String),
}

impl Pick {
    pub fn label(&self) -> &str {
        match self {
            Pick::Favorites => FAVORITES,
            Pick::Named(name) => name,
        }
    }
}

/// The player's lists, kept between sessions. The select screen reads `favorites` and `lists`;
/// its controls change them through [`toggle`](Self::toggle) and [`create`](Self::create),
/// which save the file at once. `store` is `None` in tests and when the machine has no data
/// folder, and then the lists live only for the session.
#[derive(Resource, Default)]
pub struct Collections {
    pub favorites: Vec<String>,
    pub lists: Vec<Named>,
    store: Option<Store>,
}

impl Collections {
    /// The lists kept in `store` (`None`: this session keeps nothing).
    pub fn open(store: Option<Store>) -> Collections {
        let Some(store) = store else {
            return Collections::default();
        };
        let mut this = load(&store);
        this.store = Some(store);
        this
    }

    pub fn is_favorite(&self, game: &str) -> bool {
        self.favorites.iter().any(|g| g == game)
    }

    /// Whether `game` is in `pick` (the favorites list or a named collection).
    pub fn contains(&self, pick: &Pick, game: &str) -> bool {
        match pick {
            Pick::Favorites => self.is_favorite(game),
            Pick::Named(name) => self
                .lists
                .iter()
                .find(|l| l.name == *name)
                .is_some_and(|l| l.games.iter().any(|g| g == game)),
        }
    }

    /// The named collections, in file order.
    pub fn names(&self) -> Vec<String> {
        self.lists.iter().map(|l| l.name.clone()).collect()
    }

    /// Adds `game` to `pick`, or removes it when it is already there.
    pub fn toggle(&mut self, pick: &Pick, game: &str) {
        match pick {
            Pick::Favorites => self.toggle_favorite(game),
            Pick::Named(name) => {
                let Some(list) = self.lists.iter_mut().find(|l| l.name == *name) else {
                    return;
                };
                match list.games.iter().position(|g| g == game) {
                    Some(i) => {
                        list.games.remove(i);
                    }
                    None => list.games.push(game.to_owned()),
                }
                self.commit();
            }
        }
    }

    pub fn toggle_favorite(&mut self, game: &str) {
        match self.favorites.iter().position(|g| g == game) {
            Some(i) => {
                self.favorites.remove(i);
            }
            None => self.favorites.push(game.to_owned()),
        }
        self.commit();
    }

    /// A new collection named `name`, with `game` in it when given. The name is trimmed, at
    /// most [`MAX_NAME_CHARS`], not [`FAVORITES`] and not an existing name (any case).
    pub fn create(&mut self, name: &str, game: Option<&str>) -> Result<Pick, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("a collection needs a name".into());
        }
        if name.chars().count() > MAX_NAME_CHARS {
            return Err(format!("a collection name is at most {MAX_NAME_CHARS} characters"));
        }
        if name.eq_ignore_ascii_case(FAVORITES)
            || self.lists.iter().any(|l| l.name.eq_ignore_ascii_case(name))
        {
            return Err(format!("a collection named {name} already exists"));
        }
        let games = game.map(|g| vec![g.to_owned()]).unwrap_or_default();
        self.lists.push(Named {
            name: name.to_owned(),
            games,
        });
        self.commit();
        Ok(Pick::Named(name.to_owned()))
    }

    /// Writes the file; a failed write is a warning and the next change tries again.
    fn commit(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        if let Err(e) = store.write_text(FILE, &self.to_text()) {
            storage::warn(&format!("collections {e}"));
        }
    }

    /// The file as readable TOML.
    pub fn to_text(&self) -> String {
        let array = |games: &[String]| {
            toml::Value::Array(games.iter().cloned().map(toml::Value::String).collect())
        };
        let mut root = toml::Table::new();
        root.insert("version".to_owned(), toml::Value::Integer(VERSION));
        root.insert("favorites".to_owned(), array(&self.favorites));
        root.insert(
            "collections".to_owned(),
            toml::Value::Array(
                self.lists
                    .iter()
                    .map(|l| {
                        let mut table = toml::Table::new();
                        table.insert("name".to_owned(), toml::Value::String(l.name.clone()));
                        table.insert("games".to_owned(), array(&l.games));
                        toml::Value::Table(table)
                    })
                    .collect(),
            ),
        );
        toml::to_string(&root).unwrap_or_default()
    }

    /// Reads [`to_text`](Self::to_text)'s shape. Unknown keys are ignored, so a newer file still
    /// reads as far as this build knows it.
    pub fn parse(text: &str) -> Result<Collections, String> {
        let table = text.parse::<toml::Table>().map_err(|e| e.to_string())?;
        let favorites = strings(table.get("favorites"), "favorites")?;
        let mut lists: Vec<Named> = Vec::new();
        match table.get("collections") {
            None => {}
            Some(toml::Value::Array(items)) => {
                for item in items {
                    let table = item.as_table().ok_or("a collection is not a table")?;
                    let name = table
                        .get("name")
                        .and_then(toml::Value::as_str)
                        .ok_or("a collection has no name")?
                        .trim();
                    if name.is_empty() {
                        return Err("a collection has an empty name".into());
                    }
                    if lists.iter().any(|l| l.name.eq_ignore_ascii_case(name)) {
                        continue;
                    }
                    lists.push(Named {
                        name: name.to_owned(),
                        games: strings(table.get("games"), "games")?,
                    });
                }
            }
            Some(_) => return Err("collections is not an array of tables".into()),
        }
        Ok(Collections {
            favorites,
            lists,
            store: None,
        })
    }
}

/// A TOML string array; a missing key is empty.
fn strings(value: Option<&toml::Value>, what: &str) -> Result<Vec<String>, String> {
    match value {
        None => Ok(Vec::new()),
        Some(toml::Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{what} holds something that is not a string"))
            })
            .collect(),
        Some(_) => Err(format!("{what} is not an array of strings")),
    }
}

/// Reads the file. A file this build cannot read is moved aside with a warning, as the stats
/// book does, and the session starts with no lists.
fn load(store: &Store) -> Collections {
    match store.read_text(FILE) {
        Ok(None) => Collections::default(),
        Ok(Some(text)) => Collections::parse(&text).unwrap_or_else(|e| {
            storage::warn(&format!(
                "collections {}: {e}; kept as {}, starting empty",
                store.describe(FILE),
                store.describe(BROKEN_FILE)
            ));
            if let Err(e) = store.write_text(BROKEN_FILE, &text) {
                storage::warn(&format!("collections {e}"));
            }
            Collections::default()
        }),
        Err(e) => {
            storage::warn(&format!("collections {e}"));
            Collections::default()
        }
    }
}

/// The select screen's favorite button.
#[derive(Component)]
pub(super) struct Favorite;

/// The favorite button's label.
#[derive(Component)]
pub(super) struct FavoriteLabel;

/// The row of a chip per named collection.
#[derive(Component)]
pub(super) struct Chips;

/// A chip that adds or removes the selected game from the collection `0`.
#[derive(Component)]
pub(super) struct Member(pub String);

/// The new collection's name field.
#[derive(Component)]
pub(super) struct NameField;

/// The button that creates the collection named in the field.
#[derive(Component)]
pub(super) struct CreateButton;

/// The left side's lists: the Favorite toggle, a chip per named collection for the selected
/// game, and the field that names a new one. `selected` is the selected game's package id.
pub(super) fn controls(lists: &Collections, selected: Option<&str>) -> impl Bundle + use<> {
    let chips: Vec<(String, bool)> = lists
        .lists
        .iter()
        .map(|l| {
            let on = selected.is_some_and(|id| lists.contains(&Pick::Named(l.name.clone()), id));
            (l.name.clone(), on)
        })
        .collect();
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            ..default()
        },
        children![
            (
                Node {
                    column_gap: Val::Px(10.0),
                    align_items: AlignItems::Center,
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                },
                children![
                    (
                        button_with(
                            ButtonKind::Control,
                            34.0,
                            14.0,
                            (
                                text("☆ Favorite", Type::barlow(14.0, 600), tokens::INK),
                                FavoriteLabel
                            )
                        ),
                        Favorite
                    ),
                    (
                        Node {
                            column_gap: Val::Px(6.0),
                            align_items: AlignItems::Center,
                            flex_wrap: FlexWrap::Wrap,
                            ..default()
                        },
                        Chips,
                        Children::spawn(SpawnIter(chips.into_iter().map(|(name, on)| {
                            (chip(&name, on), Member(name))
                        })))
                    ),
                ],
            ),
            (
                Node {
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                children![
                    text("New collection:", Type::barlow(14.0, 400), tokens::SOFT),
                    name_field(),
                    (button("Add", ButtonKind::Control), CreateButton),
                ],
            ),
        ],
    )
}

/// A 190 x 34 px glass field holding the new collection's name.
fn name_field() -> impl Bundle + use<> {
    use bevy::text::TextCursorStyle;
    (
        Node {
            width: Val::Px(190.0),
            height: Val::Px(34.0),
            padding: UiRect::horizontal(Val::Px(10.0)),
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        Shape::fill(tokens::GLASS),
        children![(
            Node {
                width: Val::Percent(100.0),
                ..default()
            },
            TextInput,
            EditableText::new(""),
            Type::barlow(14.0, 400).font(),
            TextColor(tokens::INK),
            TextCursorStyle {
                color: tokens::INK,
                selection_color: tokens::white(0.25),
                ..default()
            },
            NameField,
            TabIndex(0),
        )],
    )
}

/// The Favorite button flips the selected game in the favorites list.
pub(super) fn favorite(
    activate: On<Activate>,
    pressed: Query<(), With<Favorite>>,
    sel: Res<Selection>,
    lib: Res<Library>,
    mut lists: ResMut<Collections>,
) {
    if pressed.get(activate.entity).is_err() {
        return;
    }
    let Some(game) = lib.games.get(sel.game) else {
        return;
    };
    lists.toggle_favorite(&game.id);
}

/// A collection chip adds or removes the selected game.
pub(super) fn member(
    activate: On<Activate>,
    pressed: Query<&Member>,
    sel: Res<Selection>,
    lib: Res<Library>,
    mut lists: ResMut<Collections>,
) {
    let Ok(member) = pressed.get(activate.entity) else {
        return;
    };
    let Some(game) = lib.games.get(sel.game) else {
        return;
    };
    lists.toggle(&Pick::Named(member.0.clone()), &game.id);
}

/// The Add button creates the collection named in the field and puts the selected game in it.
pub(super) fn create(
    activate: On<Activate>,
    pressed: Query<(), With<CreateButton>>,
    mut field: Query<&mut EditableText, With<NameField>>,
    sel: Res<Selection>,
    lib: Res<Library>,
    mut lists: ResMut<Collections>,
) {
    if pressed.get(activate.entity).is_err() {
        return;
    }
    let Ok(mut field) = field.single_mut() else {
        return;
    };
    let Some(game) = lib.games.get(sel.game) else {
        return;
    };
    match lists.create(&field.value().to_string(), Some(&game.id)) {
        Ok(_) => *field = EditableText::new(""),
        Err(e) => storage::console_warn(&format!("collections: {e}")),
    }
}

/// Enter in the name field creates the collection, as the Add button does; Escape goes back to
/// the search field (the screen's other keys stay off while the field has the focus,
/// `super::keys`).
pub(super) fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: ResMut<InputFocus>,
    mut field: Query<&mut EditableText, With<NameField>>,
    search: Query<Entity, With<crate::ui::widgets::SearchField>>,
    sel: Res<Selection>,
    lib: Res<Library>,
    mut lists: ResMut<Collections>,
) {
    let Some(focused) = focus.get() else {
        return;
    };
    let Ok(mut field) = field.get_mut(focused) else {
        return;
    };
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter])
        && let Some(game) = lib.games.get(sel.game)
    {
        match lists.create(&field.value().to_string(), Some(&game.id)) {
            Ok(_) => *field = EditableText::new(""),
            Err(e) => storage::console_warn(&format!("collections: {e}")),
        }
    }
    if keys.just_pressed(KeyCode::Escape)
        && let Ok(search) = search.single()
    {
        focus.set(search, bevy::input_focus::FocusCause::Navigated);
    }
}

/// The Favorite button reads the selected game's state.
pub(super) fn favorite_label(
    sel: Res<Selection>,
    lib: Res<Library>,
    lists: Res<Collections>,
    mut labels: Query<&mut Text, With<FavoriteLabel>>,
) {
    if !sel.is_changed() && !lib.is_changed() && !lists.is_changed() {
        return;
    }
    let on = lib
        .games
        .get(sel.game)
        .is_some_and(|g| lists.is_favorite(&g.id));
    let want = if on { "★ Favorited" } else { "☆ Favorite" };
    for mut label in &mut labels {
        if label.0 != want {
            label.0 = want.to_owned();
        }
    }
}

/// Keeps one chip per named collection: a new collection adds its chip, and each chip reads
/// whether the selected game is in its collection.
pub(super) fn chips(
    mut commands: Commands,
    row: Query<Entity, With<Chips>>,
    members: Query<(Entity, &Member)>,
    mut visuals: Query<&mut Chip>,
    sel: Res<Selection>,
    lib: Res<Library>,
    lists: Res<Collections>,
) {
    if !sel.is_changed() && !lib.is_changed() && !lists.is_changed() {
        return;
    }
    let id = lib.games.get(sel.game).map(|g| g.id.as_str());
    let same = members.iter().count() == lists.lists.len()
        && members
            .iter()
            .all(|(_, member)| lists.lists.iter().any(|l| l.name == member.0));
    if same {
        for (entity, member) in &members {
            if let Ok(mut chip) = visuals.get_mut(entity) {
                chip.on =
                    id.is_some_and(|id| lists.contains(&Pick::Named(member.0.clone()), id));
            }
        }
        return;
    }
    for (entity, _) in &members {
        commands.entity(entity).despawn();
    }
    let Ok(row) = row.single() else {
        return;
    };
    commands.entity(row).with_children(|row| {
        for list in &lists.lists {
            let on = id.is_some_and(|id| lists.contains(&Pick::Named(list.name.clone()), id));
            row.spawn((chip(&list.name, on), Member(list.name.clone())));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> Store {
        let d = std::env::temp_dir().join(format!(
            "iwbtg-collections-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        Store::at(d)
    }

    #[test]
    fn favorites_and_collections_round_trip_through_the_store() {
        let store = temp("round");
        let mut lists = Collections::open(Some(store.clone()));
        assert!(!lists.is_favorite("demo_maps"));
        lists.toggle_favorite("demo_maps");
        lists.create("Warmups", Some("demo_clear")).unwrap();
        lists.toggle(&Pick::Named("Warmups".into()), "demo_maps");

        let again = Collections::open(Some(store));
        assert!(again.is_favorite("demo_maps"));
        assert_eq!(again.names(), ["Warmups"]);
        assert!(
            again.contains(&Pick::Named("Warmups".into()), "demo_clear"),
            "the game the collection was created with"
        );
        assert!(again.contains(&Pick::Named("Warmups".into()), "demo_maps"));
        assert!(!again.contains(&Pick::Favorites, "demo_clear"));
    }

    #[test]
    fn the_file_is_readable_toml() {
        let mut lists = Collections::default();
        lists.favorites.push("a".into());
        lists.lists.push(Named {
            name: "Bosses".into(),
            games: vec!["b".into()],
        });
        let text = lists.to_text();
        assert!(text.contains("version = 1"), "{text}");
        assert!(text.contains("[[collections]]"), "{text}");
        let back = Collections::parse(&text).unwrap();
        assert_eq!(back.favorites, ["a"]);
        assert_eq!(back.lists, lists.lists);
        assert_eq!(Collections::open(None).favorites, Vec::<String>::new());
    }

    #[test]
    fn a_broken_file_is_kept_aside_and_the_lists_start_empty() {
        let store = temp("broken");
        store.write_text(FILE, "favorites = 3\n").unwrap();
        let lists = Collections::open(Some(store.clone()));
        assert!(lists.favorites.is_empty());
        assert_eq!(store.read_text(BROKEN_FILE).unwrap().as_deref(), Some("favorites = 3\n"));
        assert!(Collections::parse("[[collections]]\ngames = []").is_err());
        assert!(Collections::parse("collections = 3").is_err());
        assert!(Collections::parse("favorites = [1]").is_err());
    }

    #[test]
    fn a_collection_needs_a_new_name_and_takes_the_game() {
        let mut lists = Collections::default();
        let pick = lists.create("  Bosses  ", Some("g")).unwrap();
        assert_eq!(pick, Pick::Named("Bosses".into()));
        assert!(lists.contains(&pick, "g"));
        assert!(lists.create("", None).is_err(), "empty");
        assert!(lists.create("bosses", None).is_err(), "a duplicate in any case");
        assert!(lists.create("Favorites", None).is_err(), "the reserved name");
        assert!(lists.create(&"x".repeat(MAX_NAME_CHARS + 1), None).is_err());
        let longest = "y".repeat(MAX_NAME_CHARS);
        assert!(lists.create(&longest, None).is_ok());
    }

    #[test]
    fn toggling_a_pick_adds_and_removes_the_game() {
        let mut lists = Collections::default();
        lists.toggle_favorite("g");
        assert!(lists.is_favorite("g"));
        lists.toggle_favorite("g");
        assert!(!lists.is_favorite("g"));
        lists.create("A", None).unwrap();
        lists.toggle(&Pick::Named("A".into()), "g");
        assert!(lists.contains(&Pick::Named("A".into()), "g"));
        lists.toggle(&Pick::Named("A".into()), "g");
        assert!(!lists.contains(&Pick::Named("A".into()), "g"));
        lists.toggle(&Pick::Named("missing".into()), "g");
        assert!(lists.names() == ["A"], "an unknown collection changes nothing");
    }
}
