//! Game select's search, group, sort and filters (#150, `Main.dc.html`, top right): which games
//! the list shows, in which order, under which headers. Pure: [`rows`] turns the library and
//! the [`Filters`] into the list's rows; `controls.rs` sets the filters, `list.rs` animates the
//! rows into place.
//!
//! The fields come from what the library has now: title, creator, your clears (stats book) and,
//! once #111 rates maps, stars. Tags, year and status wait for the game metadata model (#805),
//! collections for #105: their options are shown but cannot be picked ([`Field::ready`]).

use std::cmp::Ordering;

use bevy::prelude::*;

use super::library::{GameInfo, Records};

/// A field the list can be grouped or sorted by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// Grouping only: every game on its own, no headers (the artboard's `Group  Game`).
    Game,
    Title,
    Creator,
    /// The highest star rating of the game's maps (#111).
    Stars,
    /// Cleared or not, from your stats.
    Cleared,
    /// Release year: no data until #805.
    Year,
    /// Core tag: no data until #805.
    Tag,
    /// Port status (Ranked, Loved, Beta): no data until #805.
    Status,
}

impl Field {
    /// The Group menu, in order.
    pub const GROUPS: [Field; 7] = [
        Field::Game,
        Field::Creator,
        Field::Stars,
        Field::Cleared,
        Field::Year,
        Field::Tag,
        Field::Status,
    ];
    /// The Sort menu, in order.
    pub const SORTS: [Field; 7] = [
        Field::Title,
        Field::Creator,
        Field::Stars,
        Field::Cleared,
        Field::Year,
        Field::Tag,
        Field::Status,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Field::Game => "Game",
            Field::Title => "Title",
            Field::Creator => "Creator",
            Field::Stars => "Stars",
            Field::Cleared => "Cleared",
            Field::Year => "Year",
            Field::Tag => "Tag",
            Field::Status => "Status",
        }
    }

    /// Whether the library has the data for this field: stars once any map is rated (#111);
    /// year, tag and status not until the game metadata model (#805).
    pub fn ready(self, games: &[GameInfo]) -> bool {
        match self {
            Field::Game | Field::Title | Field::Creator | Field::Cleared => true,
            Field::Stars => games.iter().any(|g| stars(g).is_some()),
            Field::Year | Field::Tag | Field::Status => false,
        }
    }
}

/// The Cleared and Uncleared chips: at most one is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Clears {
    #[default]
    Any,
    Cleared,
    Uncleared,
}

impl Clears {
    /// The chip `pick` clicked: on, or back to Any when it was on already.
    pub fn toggle(self, pick: Clears) -> Clears {
        if self == pick { Clears::Any } else { pick }
    }
}

/// What the list shows. Kept between runs, so coming back from a game keeps the search.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Filters {
    /// The search field's text: every word must be in the title or the creator.
    pub search: String,
    pub group: Field,
    pub sort: Field,
    pub clears: Clears,
    /// The Creator dropdown: `None` is Any.
    pub creator: Option<String>,
}

impl Default for Filters {
    /// No search, no headers, by title (the artboard sorts by stars; until #111 rates maps,
    /// stars order nothing).
    fn default() -> Filters {
        Filters {
            search: String::new(),
            group: Field::Game,
            sort: Field::Title,
            clears: Clears::Any,
            creator: None,
        }
    }
}

/// One row of the list.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    /// A group's header: its name and how many games it holds.
    Header(String, usize),
    /// A game, by index in the library.
    Game(usize),
}

/// The highest star rating of a game's maps; `None` when none is rated (#111).
pub fn stars(game: &GameInfo) -> Option<f32> {
    game.maps.iter().filter_map(|m| m.stars).reduce(f32::max)
}

/// The creator a game is grouped and filtered by; a game without one reads "Unknown".
pub fn creator(game: &GameInfo) -> &str {
    if game.creator.is_empty() {
        "Unknown"
    } else {
        &game.creator
    }
}

/// The creators of `games`, each once, by name (the Creator dropdown).
pub fn creators(games: &[GameInfo]) -> Vec<String> {
    let mut out: Vec<String> = games.iter().map(|g| creator(g).to_owned()).collect();
    out.sort_by_key(|c| c.to_lowercase());
    out.dedup();
    out
}

/// Whether `game` passes the search and the filters.
pub fn matches(game: &GameInfo, cleared: bool, f: &Filters) -> bool {
    let words = f.search.to_lowercase();
    let title = game.title.to_lowercase();
    let by = game.creator.to_lowercase();
    let found = words
        .split_whitespace()
        .all(|w| title.contains(w) || by.contains(w));
    let clears = match f.clears {
        Clears::Any => true,
        Clears::Cleared => cleared,
        Clears::Uncleared => !cleared,
    };
    let made = f.creator.as_deref().is_none_or(|c| creator(game) == c);
    found && clears && made
}

/// The group a game falls in, by `field`; `None` for [`Field::Game`] (no headers).
fn group_of(field: Field, game: &GameInfo, cleared: bool) -> Option<String> {
    match field {
        Field::Creator => Some(creator(game).to_owned()),
        Field::Cleared => Some(if cleared { "Cleared" } else { "Not cleared" }.to_owned()),
        Field::Stars => {
            Some(stars(game).map_or_else(|| "No rating".to_owned(), |s| format!("★ {}", s.floor() as i32)))
        }
        _ => None,
    }
}

/// Compares two games by `field`; ties fall through to the next key.
fn by(field: Field, a: (&GameInfo, bool), b: (&GameInfo, bool)) -> Ordering {
    match field {
        Field::Title | Field::Game => Ordering::Equal,
        Field::Creator => creator(a.0).to_lowercase().cmp(&creator(b.0).to_lowercase()),
        // Easiest first, unrated last.
        Field::Stars => match (stars(a.0), stars(b.0)) {
            (Some(x), Some(y)) => x.total_cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
        // Cleared first.
        Field::Cleared => b.1.cmp(&a.1),
        Field::Year | Field::Tag | Field::Status => Ordering::Equal,
    }
}

/// The list's rows: the games that pass, grouped (a header before each group) and sorted
/// within, ties by title and then by package order. A field that is not ready
/// ([`Field::ready`]) groups and sorts like `Game` and `Title`.
pub fn rows(games: &[GameInfo], records: &[Records], f: &Filters) -> Vec<Row> {
    let cleared = |i: usize| records.get(i).is_some_and(|r| r.cleared);
    let group = if f.group.ready(games) {
        f.group
    } else {
        Field::Game
    };
    let sort = if f.sort.ready(games) { f.sort } else { Field::Title };
    let mut shown: Vec<usize> = (0..games.len())
        .filter(|&i| matches(&games[i], cleared(i), f))
        .collect();
    let key = |i: usize| group_of(group, &games[i], cleared(i));
    shown.sort_by(|&a, &b| {
        let (ga, gb) = ((&games[a], cleared(a)), (&games[b], cleared(b)));
        // Groups come in their own order: the grouping field's sort.
        by(group, ga, gb)
            .then_with(|| key(a).cmp(&key(b)))
            .then_with(|| by(sort, ga, gb))
            .then_with(|| games[a].title.to_lowercase().cmp(&games[b].title.to_lowercase()))
            .then(a.cmp(&b))
    });
    let mut out = Vec::with_capacity(shown.len());
    let mut i = 0;
    while i < shown.len() {
        let name = key(shown[i]);
        let len = shown[i..].iter().take_while(|&&g| key(g) == name).count();
        if let Some(name) = name {
            out.push(Row::Header(name, len));
        }
        out.extend(shown[i..i + len].iter().map(|&g| Row::Game(g)));
        i += len;
    }
    out
}

/// The games among `rows`, in order.
pub fn order(rows: &[Row]) -> Vec<usize> {
    rows.iter()
        .filter_map(|r| match r {
            Row::Game(g) => Some(*g),
            Row::Header(..) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::shell::select::library::MapInfo;

    fn game(title: &str, creator: &str, stars: &[Option<f32>]) -> GameInfo {
        GameInfo {
            path: PathBuf::from(title),
            id: title.to_owned(),
            title: title.to_owned(),
            creator: creator.to_owned(),
            maps: stars
                .iter()
                .enumerate()
                .map(|(i, s)| MapInfo {
                    id: format!("m{i}"),
                    name: format!("Map {i}"),
                    stars: *s,
                })
                .collect(),
        }
    }

    fn records(cleared: &[bool]) -> Vec<Records> {
        cleared
            .iter()
            .map(|&c| Records {
                pbs: Vec::new(),
                cleared: c,
            })
            .collect()
    }

    fn library() -> Vec<GameInfo> {
        vec![
            game("rely the needle", "Kale", &[None]),
            game("Kaenbyou", "Kira", &[None, None]),
            game("I Wanna Be The Showcase", "", &[None]),
            game("needle hell", "Kira", &[None]),
        ]
    }

    #[test]
    fn search_keeps_games_whose_title_or_creator_has_every_word() {
        let games = library();
        let recs = records(&[false; 4]);
        let f = |s: &str| Filters {
            search: s.to_owned(),
            ..default()
        };
        assert_eq!(order(&rows(&games, &recs, &f("needle"))), [3, 0]);
        assert_eq!(order(&rows(&games, &recs, &f("NEEDLE kira"))), [3]);
        assert_eq!(
            order(&rows(&games, &recs, &f("kale"))),
            [0],
            "the creator matches too"
        );
        assert_eq!(order(&rows(&games, &recs, &f("  "))).len(), 4, "blank keeps all");
        assert!(rows(&games, &recs, &f("zzz")).is_empty());
    }

    #[test]
    fn the_default_order_is_by_title_without_headers() {
        let games = library();
        let rows = rows(&games, &records(&[false; 4]), &Filters::default());
        assert_eq!(rows, [Row::Game(2), Row::Game(1), Row::Game(3), Row::Game(0)]);
    }

    #[test]
    fn the_clear_chips_and_the_creator_dropdown_filter() {
        let games = library();
        let recs = records(&[true, false, false, true]);
        let only = |clears, creator: Option<&str>| {
            let f = Filters {
                clears,
                creator: creator.map(str::to_owned),
                ..default()
            };
            order(&rows(&games, &recs, &f))
        };
        assert_eq!(only(Clears::Cleared, None), [3, 0]);
        assert_eq!(only(Clears::Uncleared, None), [2, 1]);
        assert_eq!(only(Clears::Any, Some("Kira")), [1, 3]);
        assert_eq!(
            only(Clears::Any, Some("Unknown")),
            [2],
            "no creator reads Unknown"
        );
        assert_eq!(only(Clears::Cleared, Some("Kira")), [3]);
        assert_eq!(Clears::Cleared.toggle(Clears::Cleared), Clears::Any);
        assert_eq!(Clears::Cleared.toggle(Clears::Uncleared), Clears::Uncleared);
        assert_eq!(creators(&games), ["Kale", "Kira", "Unknown"]);
    }

    #[test]
    fn grouping_puts_a_header_with_its_count_before_each_group() {
        let games = library();
        let recs = records(&[true, false, false, true]);
        let f = Filters {
            group: Field::Creator,
            ..default()
        };
        assert_eq!(
            rows(&games, &recs, &f),
            [
                Row::Header("Kale".into(), 1),
                Row::Game(0),
                Row::Header("Kira".into(), 2),
                Row::Game(1),
                Row::Game(3),
                Row::Header("Unknown".into(), 1),
                Row::Game(2),
            ]
        );
        let f = Filters {
            group: Field::Cleared,
            ..default()
        };
        assert_eq!(
            rows(&games, &recs, &f),
            [
                Row::Header("Cleared".into(), 2),
                Row::Game(3),
                Row::Game(0),
                Row::Header("Not cleared".into(), 2),
                Row::Game(2),
                Row::Game(1),
            ]
        );
    }

    #[test]
    fn sorting_by_creator_or_clears_breaks_ties_by_title() {
        let games = library();
        let recs = records(&[false, true, false, false]);
        let sorted = |sort| {
            let f = Filters { sort, ..default() };
            order(&rows(&games, &recs, &f))
        };
        assert_eq!(sorted(Field::Creator), [0, 1, 3, 2], "no creator reads Unknown");
        assert_eq!(sorted(Field::Cleared), [1, 2, 3, 0]);
    }

    #[test]
    fn stars_wait_for_ratings_and_sort_easiest_first_once_there() {
        let mut games = library();
        assert!(!Field::Stars.ready(&games), "no map is rated before #111");
        let recs = records(&[false; 4]);
        let f = Filters {
            sort: Field::Stars,
            ..default()
        };
        assert_eq!(
            order(&rows(&games, &recs, &f)),
            order(&rows(&games, &recs, &Filters::default())),
            "unrated stars sort by title"
        );
        games[0].maps[0].stars = Some(6.5);
        games[1].maps[1].stars = Some(2.1);
        assert!(Field::Stars.ready(&games));
        assert_eq!(order(&rows(&games, &recs, &f)), [1, 0, 2, 3], "unrated last");
        let g = Filters {
            group: Field::Stars,
            ..default()
        };
        assert_eq!(
            rows(&games, &recs, &g)[0],
            Row::Header("★ 2".into(), 1),
            "groups follow the field's order"
        );
    }

    #[test]
    fn year_tag_and_status_wait_for_the_metadata_model() {
        let games = library();
        for field in [Field::Year, Field::Tag, Field::Status] {
            assert!(!field.ready(&games), "{field:?}");
            let f = Filters {
                group: field,
                sort: field,
                ..default()
            };
            assert_eq!(
                rows(&games, &records(&[false; 4]), &f),
                rows(&games, &records(&[false; 4]), &Filters::default())
            );
        }
    }
}
