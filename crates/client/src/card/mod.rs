//! The player card (#496; docs/architecture/client/card.md): compact in the game select footer,
//! expanded on results with what the run changed. Artboard `ProfileCard.dc.html`, variants
//! `footer`, `results` and `results-compact` (results at 4:3).
//!
//! The card reads the local stats book (`stats/`) and the local profile name; game select
//! (#148) and results (#41) embed it with [`card`]. Parts that need things not built yet stay
//! hidden: the overall rank and the big faint rank behind (#84), the flag (accounts, #60), the
//! rank sparkline (online) and the badges (#489).

pub mod preview;

use bevy::prelude::*;
use bevy::ui::InterpolationColorSpace;

use crate::stats::{Book, Run};
use crate::ui::fonts::Type;
use crate::ui::shape::{Band, Shape, Side, Veil};
use crate::ui::tokens;
use crate::ui::widgets::{rich, text};

/// Games in the collection until game select has a library (#148): the 20-game shortlist
/// (docs/roadmap.md, M5).
pub const COLLECTION_GAMES: usize = 20;

/// The three sizes of the artboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Variant {
    /// 420x108, bottom right of game select.
    Footer,
    /// 620x280, on results at 16:9 and wider.
    Results,
    /// 330x172, on results at 4:3.
    ResultsCompact,
}

impl Variant {
    /// The results variant for a window `aspect` (width / height) wide: compact below 16:10.
    #[allow(dead_code, reason = "the results screen picks the variant (#41)")]
    pub fn for_results(aspect: f32) -> Variant {
        if aspect < 1.6 {
            Variant::ResultsCompact
        } else {
            Variant::Results
        }
    }

    fn dims(self) -> Dims {
        match self {
            Variant::Footer => Dims {
                size: Vec2::new(420.0, 108.0),
                pad: [12.0, 24.0, 12.0, 18.0],
                gap: 8.0,
                avatar: 52.0,
                avatar_font: 26.0,
                name: 19.0,
                line: 13.0,
                bar: 5.0,
                cols: 0,
                cell: 0.0,
            },
            Variant::Results => Dims {
                size: Vec2::new(620.0, 280.0),
                pad: [18.0, 26.0, 18.0, 22.0],
                gap: 14.0,
                avatar: 72.0,
                avatar_font: 36.0,
                name: 24.0,
                line: 15.0,
                bar: 6.0,
                cols: 4,
                cell: 20.0,
            },
            Variant::ResultsCompact => Dims {
                size: Vec2::new(330.0, 172.0),
                pad: [14.0, 20.0, 14.0, 18.0],
                gap: 10.0,
                avatar: 44.0,
                avatar_font: 22.0,
                name: 17.0,
                line: 13.0,
                bar: 6.0,
                cols: 2,
                cell: 16.0,
            },
        }
    }

    /// The card's size in logical px.
    pub fn size(self) -> Vec2 {
        self.dims().size
    }
}

/// The artboard's numbers for one variant.
struct Dims {
    size: Vec2,
    /// Top, right, bottom, left.
    pad: [f32; 4],
    gap: f32,
    avatar: f32,
    avatar_font: f32,
    name: f32,
    line: f32,
    bar: f32,
    /// Columns of this run's changes; 0: none (the footer).
    cols: usize,
    cell: f32,
}

/// What the card shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub name: String,
    pub games_cleared: usize,
    pub games_total: usize,
    pub deaths: u64,
    pub play_seconds: f64,
    /// What the run on results changed; `None` in the footer.
    pub run: Option<RunChange>,
}

/// What one run changed (the results variants).
#[derive(Clone, Debug, PartialEq)]
pub struct RunChange {
    pub deaths: u64,
    pub play_seconds: f64,
    /// The game's title, the label over its entries.
    pub game: String,
    /// The game's maps (entries) with a clear, out of all of them.
    pub entries_cleared: usize,
    pub entries_total: usize,
}

impl Card {
    /// The card from the stats book: account totals, the collection out of `games_total`.
    pub fn from_book(book: &Book, name: &str, games_total: usize) -> Card {
        let t = book.totals();
        Card {
            name: name.to_owned(),
            games_cleared: t.games_cleared,
            games_total: games_total.max(t.games_cleared),
            deaths: t.deaths,
            play_seconds: t.play_seconds,
            run: None,
        }
    }
}

impl RunChange {
    /// `run` on a map of `game` (its id): its deaths and time, and the game's entries cleared
    /// out of `entries_total` (the package's maps).
    pub fn new(
        book: &Book,
        game: &str,
        title: &str,
        entries_total: usize,
        run: &Run,
        tick_rate: u32,
    ) -> RunChange {
        let entries_cleared = book
            .maps
            .values()
            .filter(|m| m.game == game && m.clears > 0)
            .count();
        RunChange {
            deaths: u64::from(run.deaths),
            play_seconds: run.ticks as f64 / f64::from(tick_rate.max(1)),
            game: title.to_owned(),
            entries_cleared,
            entries_total: entries_total.max(entries_cleared),
        }
    }
}

/// The local profile name: the settings file's `name` when set, else the system account's name
/// until accounts exist (#60), else `Player`.
pub fn profile_name() -> String {
    let set = crate::settings::get().name;
    if !set.is_empty() {
        return set;
    }
    #[cfg(not(target_arch = "wasm32"))]
    for var in ["USERNAME", "USER"] {
        if let Ok(name) = std::env::var(var) {
            let name = name.trim();
            if !name.is_empty() {
                return name.to_owned();
            }
        }
    }
    "Player".to_owned()
}

/// `48213` as `48,213`.
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Play time in its largest unit: `212h`, `38m`, `12s`.
pub fn hours(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m", s / 60),
        _ => format!("{}h", s / 3600),
    }
}

/// Play time in its two largest units: `212h 14m`, `38m 12s`, `12s`.
pub fn hours_minutes(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m {:02}s", s / 60, s % 60),
        _ => format!("{}h {:02}m", s / 3600, s / 60 % 60),
    }
}

/// One cell of this run's changes: label, value, change.
fn cells(card: &Card, cols: usize) -> Vec<[String; 3]> {
    let run = card.run.as_ref();
    let mut out = vec![
        [
            "Total deaths".to_owned(),
            grouped(card.deaths),
            run.map_or(String::new(), |r| format!("+{} this run", grouped(r.deaths))),
        ],
        [
            "Play time".to_owned(),
            hours_minutes(card.play_seconds),
            run.map_or(String::new(), |r| format!("+{}", hours_minutes(r.play_seconds))),
        ],
    ];
    if let Some(r) = run {
        let unit = if r.entries_total == 1 { "entry" } else { "entries" };
        out.push([
            r.game.clone(),
            format!("{} / {} {unit}", r.entries_cleared, r.entries_total),
            "cleared".to_owned(),
        ]);
    }
    out.truncate(cols);
    out
}

/// The player card in `variant`, at the artboard's size in logical px.
pub fn card(card: &Card, variant: Variant) -> impl Bundle + use<> {
    let d = variant.dims();
    let card = card.clone();
    (
        Node {
            width: Val::Px(d.size.x),
            height: Val::Px(d.size.y),
            padding: UiRect {
                top: Val::Px(d.pad[0]),
                right: Val::Px(d.pad[1]),
                bottom: Val::Px(d.pad[2]),
                left: Val::Px(d.pad[3]),
            },
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(d.gap),
            flex_shrink: 0.0,
            ..default()
        },
        Shape::fill(Color::NONE)
            .with_veil(Veil::across(
                tokens::CARD,
                tokens::CARD_MID,
                tokens::CARD_WARM,
                0.6,
            ))
            .with_band(Band {
                side: Side::Bottom,
                width: 3.0,
                color: tokens::ACCENT,
                fade_to: 0.75,
            })
            .with_shadow(12.0, 36.0, Color::srgba(0.0, 0.0, 0.0, 0.4)),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn(identity(&card, variant, &d));
            c.spawn(collection(&card, &d));
            if d.cols > 0 {
                let inner = d.size.x - d.pad[1] - d.pad[3];
                c.spawn(changes(&card, &d, inner));
            }
        })),
    )
}

/// Avatar, name and the lines under it.
fn identity(card: &Card, variant: Variant, d: &Dims) -> impl Bundle + use<> {
    let initial = card
        .name
        .chars()
        .next()
        .map_or("?".to_owned(), |c| c.to_uppercase().collect());
    let avatar = (
        Node {
            width: Val::Px(d.avatar),
            height: Val::Px(d.avatar),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(tokens::RADIUS)),
            ..default()
        },
        BackgroundGradient::from(LinearGradient {
            color_space: InterpolationColorSpace::Srgba,
            ..LinearGradient::to_bottom_right(vec![
                ColorStop::auto(tokens::AVATAR_FROM),
                ColorStop::auto(tokens::AVATAR_TO),
            ])
        }),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.35),
            Val::Px(0.0),
            Val::Px(6.0),
            Val::Px(0.0),
            Val::Px(16.0),
        ),
        children![text(&initial, Type::condensed(d.avatar_font, 700), tokens::INK)],
    );
    let footer = variant == Variant::Footer;
    let cleared = if footer {
        format!("{} cleared", card.games_cleared)
    } else {
        let unit = if card.games_cleared == 1 { "game" } else { "games" };
        format!("{} {unit} cleared", card.games_cleared)
    };
    let name = card.name.clone();
    let line = d.line;
    let name_size = d.name;
    let (deaths, play) = (grouped(card.deaths), hours(card.play_seconds));
    (
        Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(14.0),
            ..default()
        },
        Children::spawn((
            Spawn(avatar),
            Spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    flex_grow: 1.0,
                    min_width: Val::Px(0.0),
                    overflow: Overflow::clip_x(),
                    ..default()
                },
                Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
                    c.spawn((
                        text(&name, Type::barlow(name_size, 700), tokens::INK),
                        TextLayout::no_wrap(),
                    ));
                    c.spawn((
                        text(&cleared, Type::barlow(line, 400), tokens::SOFT),
                        TextLayout::no_wrap(),
                    ));
                    if footer {
                        let soft = Type::barlow(13.0, 400);
                        let bold = Type::barlow(13.0, 600);
                        c.spawn(rich(vec![
                            ("Deaths ".to_owned(), soft, tokens::SOFT),
                            (deaths, bold, tokens::INK),
                            (" · Play time ".to_owned(), soft, tokens::SOFT),
                            (play, bold, tokens::INK),
                        ]));
                    }
                })),
            )),
        )),
    )
}

/// `Collection   13 / 20 games cleared` over the bar, a notch between games.
fn collection(card: &Card, d: &Dims) -> impl Bundle + use<> {
    let total = card.games_total.max(1);
    let share = card.games_cleared.min(total) as f32 / total as f32;
    let small = Type::barlow(12.0, 600);
    let header = (
        Node {
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        },
        children![
            text("Collection", small, tokens::SOFT),
            rich(vec![
                (card.games_cleared.to_string(), small, tokens::INK),
                (
                    format!(" / {} games cleared", card.games_total),
                    small,
                    tokens::SOFT
                ),
            ]),
        ],
    );
    let fill = (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            width: Val::Percent(share * 100.0),
            ..default()
        },
        BackgroundGradient::from(LinearGradient {
            color_space: InterpolationColorSpace::Srgba,
            ..LinearGradient::to_right(vec![
                ColorStop::auto(tokens::ACCENT),
                ColorStop::auto(tokens::ACCENT_LIGHT),
            ])
        }),
    );
    let notches = (1..total).map(move |k| {
        (
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(k as f32 * 100.0 / total as f32),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(tokens::NOTCH),
        )
    });
    let bar = (
        Node {
            height: Val::Px(d.bar),
            border_radius: BorderRadius::all(Val::Px(3.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(tokens::TRACK),
        Children::spawn((Spawn(fill), SpawnIter(notches))),
    );
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        },
        Children::spawn((Spawn(header), Spawn(bar))),
    )
}

/// This run's changes, in the artboard's columns.
fn changes(card: &Card, d: &Dims, inner: f32) -> impl Bundle + use<> {
    let gap = 16.0;
    let width = (inner - gap * (d.cols - 1) as f32) / d.cols as f32;
    let value = Type::barlow(d.cell, 600);
    let change = Type::barlow(13.0, 600);
    let cells = cells(card, d.cols);
    (
        Node {
            column_gap: Val::Px(gap),
            ..default()
        },
        Children::spawn(SpawnIter(cells.into_iter().map(move |[k, v, delta]| {
            (
                Node {
                    width: Val::Px(width),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    overflow: Overflow::clip_x(),
                    ..default()
                },
                children![
                    (text(&k, Type::LABEL, tokens::MUTED), TextLayout::no_wrap()),
                    (text(&v, value, tokens::INK), TextLayout::no_wrap()),
                    (text(&delta, change, tokens::SOFT), TextLayout::no_wrap()),
                ],
            )
        }))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::MapStats;

    fn map(game: &str, clears: u32, deaths: u64, seconds: f64) -> MapStats {
        MapStats {
            game: game.to_owned(),
            tick_rate: 50,
            clears,
            deaths,
            play_seconds: seconds,
            ..MapStats::default()
        }
    }

    #[test]
    fn numbers_read_as_the_artboard_writes_them() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(48_213), "48,213");
        assert_eq!(grouped(1_234_567), "1,234,567");
        assert_eq!(hours(764_040.0), "212h");
        assert_eq!(hours(2292.0), "38m");
        assert_eq!(hours(12.9), "12s");
        assert_eq!(hours_minutes(764_040.0), "212h 14m");
        assert_eq!(hours_minutes(2292.0), "38m 12s");
        assert_eq!(hours_minutes(3725.0), "1h 02m");
        assert_eq!(hours_minutes(-3.0), "0s");
    }

    /// Totals over the book, the collection out of all games, and a run's changes: its deaths,
    /// its time and the game's cleared entries.
    #[test]
    fn the_card_reads_the_book() {
        let mut book = Book::default();
        book.maps
            .insert("kaenbyou/easy".into(), map("kaenbyou", 2, 100, 600.0));
        book.maps
            .insert("kaenbyou/hard".into(), map("kaenbyou", 0, 50, 300.0));
        book.maps.insert("needle/main".into(), map("needle", 1, 7, 60.0));
        let c = Card::from_book(&book, "kid", COLLECTION_GAMES);
        assert_eq!(
            (c.games_cleared, c.games_total, c.deaths, c.play_seconds),
            (2, 20, 157, 960.0)
        );
        assert_eq!(
            Card::from_book(&book, "kid", 1).games_total,
            2,
            "never fewer than cleared"
        );

        let run = Run {
            ticks: 50 * 125,
            deaths: 12,
            splits: vec![50 * 125],
            segment_deaths: vec![12],
            finished_at: 0,
            replay: None,
            death_log: Default::default(),
        };
        let r = RunChange::new(&book, "kaenbyou", "Kaenbyou", 5, &run, 50);
        assert_eq!((r.deaths, r.play_seconds), (12, 125.0));
        assert_eq!((r.entries_cleared, r.entries_total), (1, 5));

        let full = Card { run: Some(r), ..c };
        let shown = cells(&full, 4);
        assert_eq!(
            shown[0],
            ["Total deaths", "157", "+12 this run"].map(String::from)
        );
        assert_eq!(shown[1], ["Play time", "16m 00s", "+2m 05s"].map(String::from));
        assert_eq!(
            shown[2],
            ["Kaenbyou", "1 / 5 entries", "cleared"].map(String::from)
        );
        assert_eq!(cells(&full, 2).len(), 2, "compact: deaths and play time");
    }

    #[test]
    fn results_at_4_3_use_the_compact_card() {
        assert_eq!(Variant::for_results(4.0 / 3.0), Variant::ResultsCompact);
        assert_eq!(Variant::for_results(16.0 / 10.0), Variant::Results);
        assert_eq!(Variant::for_results(16.0 / 9.0), Variant::Results);
        assert_eq!(Variant::Footer.size(), Vec2::new(420.0, 108.0));
    }
}
