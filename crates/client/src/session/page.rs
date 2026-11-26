//! The session summary's page (`Session.dc.html`): the header, the hero number with the facts
//! under it, the player card top right, the most deaths by room, the session's timeline (deaths
//! and saves) and the footer buttons. Laid out at 1920 x 1080.

use bevy::prelude::*;

use super::Action;
use crate::card;
use crate::locale;
use crate::stats::{GameMode, SessionLog};
use crate::ui::fonts::Type;
use crate::ui::shape::Shape;
use crate::ui::tokens;
use crate::ui::widgets::{ButtonKind, button_with, classic_pill, text, text_leading};

/// The artboard.
pub const DESIGN: Vec2 = Vec2::new(1920.0, 1080.0);
/// `#b9adff`: Classic's light violet.
const CLASSIC_LIGHT: Color = Color::srgb_u8(0xb9, 0xad, 0xff);
/// The death ticks: `rgba(255,107,107,.75)`.
const DEATH_TICK: Color = Color::srgba(1.0, 107.0 / 255.0, 107.0 / 255.0, 0.75);
/// The section labels: 12 px, 700, 0.14 em, capitals.
const CAPS: Type = Type {
    tracking: 0.14,
    caps: true,
    ..Type::barlow(12.0, 700)
};
/// How many save marks the timeline labels at most (the rest are diamonds only).
const SAVE_LABELS: usize = 8;

/// What the page shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub title: String,
    /// `26 Nov 2026, 21:14 – 22:26`.
    pub when: String,
    pub finished: bool,
    pub hero_key: String,
    pub hero: String,
    /// Label, value, note, whether the note is the violet "new" ink.
    pub facts: Vec<(String, String, String, bool)>,
    /// Most deaths by room, at most three.
    pub rooms: Vec<(String, u32)>,
    /// Each death's place along the session, 0 to 1.
    pub death_marks: Vec<f32>,
    /// Each save: its place along the session and the room.
    pub saves: Vec<(f32, String)>,
    pub start: String,
    pub end: String,
    pub card: card::Card,
}

impl Summary {
    /// The open session's summary (an empty one without a session).
    pub fn read(world: &World, finished: bool) -> Summary {
        let game = world.non_send::<crate::view::Game>();
        let def = game.sim.def();
        let title = world
            .get_resource::<crate::shell::LoadedPackage>()
            .and_then(|p| p.pkg.files.get("game.toml"))
            .and_then(|b| std::str::from_utf8(b).ok())
            .and_then(|t| iwbtg_format::manifest::Manifest::parse(t).ok())
            .map(|m| m.title.into_inner())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| def.id.clone());
        let stats = world.get_resource::<crate::stats::Stats>();
        let empty = SessionLog {
            game: def.id.clone(),
            tick_rate: def.tick_rate,
            started_at: crate::stats::now_seconds(),
            ..SessionLog::default()
        };
        let log = stats.and_then(|s| s.open_session()).unwrap_or(&empty);
        let mode = stats
            .map(|s| s.book.game_mode(&def.id, false))
            .unwrap_or_default();
        let now = world
            .get_resource::<super::Session>()
            .map_or_else(crate::stats::now_seconds, |s| s.opened_at);
        let mut s = Summary::from_log(log, &mode, title, finished, now);
        let empty_book = crate::stats::Book::default();
        let book = stats.map_or(&empty_book, |st| &st.book);
        s.card = card::Card::from_book(book, &card::profile_name(), card::COLLECTION_GAMES);
        s
    }

    /// The summary of `log` with the game's Classic numbers `mode`, ended at `now`.
    pub fn from_log(log: &SessionLog, mode: &GameMode, title: String, finished: bool, now: u64) -> Summary {
        let clock = crate::shell::shelf::clock;
        let span = log.ticks.max(1) as f32;
        let at = |t: u64| (t as f32 / span).clamp(0.0, 1.0);
        let new_rooms = log.new_rooms();
        let distinct = {
            let mut r: Vec<&str> = log.rooms.iter().map(|v| v.room.as_str()).collect();
            r.sort_unstable();
            r.dedup();
            r.len()
        };
        let deaths = (
            locale::text("session_deaths").to_owned(),
            card::grouped(log.deaths),
            locale::format("pause_in_this_game", &[("n", &card::grouped(mode.deaths))]),
            false,
        );
        let rooms = (
            locale::text("session_rooms").to_owned(),
            distinct.to_string(),
            locale::format("session_rooms_new", &[("n", &new_rooms)]),
            false,
        );
        let facts = if finished {
            vec![
                deaths,
                (
                    locale::text("session_time_played").to_owned(),
                    clock(mode.play_seconds),
                    locale::text("session_all_sessions").to_owned(),
                    false,
                ),
                rooms,
                (
                    locale::text("session_finished").to_owned(),
                    crate::shell::shelf::times(mode.finishes.max(1)),
                    locale::text("mode_classic").to_owned(),
                    false,
                ),
            ]
        } else {
            let furthest = log.rooms.last().map(|r| r.room.clone()).unwrap_or_default();
            vec![
                deaths,
                (
                    locale::text("session_where").to_owned(),
                    furthest,
                    if new_rooms > 0 {
                        locale::format("session_rooms_new", &[("n", &new_rooms)])
                    } else {
                        String::new()
                    },
                    new_rooms > 0,
                ),
                (
                    locale::text("session_save_points").to_owned(),
                    log.saves.len().to_string(),
                    log.saves.last().map_or_else(String::new, |s| {
                        locale::format("session_last_save", &[("room", &s.room)])
                    }),
                    false,
                ),
                rooms,
            ]
        };
        let hhmm = |secs: u64| {
            let local = secs as i64 + i64::from(crate::hud::leaderboard::local_offset(secs));
            let m = local.rem_euclid(86_400) / 60;
            format!("{}:{:02}", m / 60, m % 60)
        };
        Summary {
            title,
            when: format!(
                "{}, {} – {}",
                crate::hud::leaderboard::date(log.started_at),
                hhmm(log.started_at),
                hhmm(now)
            ),
            finished,
            hero_key: locale::text(if finished {
                "session_hero_finished"
            } else {
                "session_hero"
            })
            .to_owned(),
            hero: clock(log.seconds),
            facts,
            rooms: log
                .deaths_by_room()
                .into_iter()
                .take(3)
                .map(|(r, n)| (r.to_owned(), n))
                .collect(),
            death_marks: log.death_log.iter().map(|d| at(d.tick)).collect(),
            saves: log.saves.iter().map(|s| (at(s.tick), s.room.clone())).collect(),
            start: hhmm(log.started_at),
            end: hhmm(now),
            card: card::Card::from_book(
                &crate::stats::Book::default(),
                "",
                card::COLLECTION_GAMES,
            ),
        }
    }
}

/// The page, 1920 x 1080.
pub fn page(s: &Summary, fruit: Option<Handle<Image>>) -> impl Bundle + use<> {
    let s = s.clone();
    (
        Node {
            width: Val::Px(DESIGN.x),
            height: Val::Px(DESIGN.y),
            ..default()
        },
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            header(c, &s);
            hero(c, &s, fruit);
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(80.0),
                    top: Val::Px(64.0),
                    ..default()
                },
                children![card::card(&s.card, card::Variant::Results)],
            ));
            details(c, &s);
            footer(c, &s);
        })),
    )
}

fn header(c: &mut ChildSpawner, s: &Summary) {
    c.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(80.0),
        top: Val::Px(64.0),
        align_items: AlignItems::Center,
        column_gap: Val::Px(16.0),
        ..default()
    })
    .with_children(|c| {
        c.spawn((text(&s.title, Type::barlow(26.0, 700), tokens::INK), TextLayout::no_wrap()));
        c.spawn(classic_pill());
        c.spawn((text(&s.when, Type::barlow(15.0, 400), tokens::MUTED), TextLayout::no_wrap()));
    });
}

fn hero(c: &mut ChildSpawner, s: &Summary, fruit: Option<Handle<Image>>) {
    c.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(80.0),
        top: Val::Px(130.0),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(10.0),
        ..default()
    })
    .with_children(|c| {
        c.spawn(text(&s.hero_key, CAPS, tokens::MUTED));
        c.spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(24.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(text_leading(&s.hero, Type::condensed(150.0, 700), tokens::INK, 0.9));
            if s.finished {
                c.spawn((
                    crate::ui::widgets::sheared(
                        40.0,
                        16.0,
                        Shape::fill(tokens::CLASSIC.with_alpha(0.3)).with_ring(2.0, CLASSIC_LIGHT),
                    ),
                    Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
                        if let Some(f) = fruit {
                            c.spawn((
                                Node {
                                    width: Val::Px(18.0),
                                    height: Val::Px(21.0),
                                    ..default()
                                },
                                ImageNode {
                                    image: f,
                                    rect: Some(Rect::new(
                                        0.0,
                                        0.0,
                                        crate::pause::FRUIT.x as f32,
                                        crate::pause::FRUIT.y as f32,
                                    )),
                                    ..default()
                                },
                            ));
                        }
                        c.spawn(text(
                            locale::text("session_finished_pill"),
                            Type::barlow(18.0, 700),
                            tokens::INK,
                        ));
                    })),
                ));
            }
        });
        c.spawn(Node {
            column_gap: Val::Px(56.0),
            margin: UiRect::top(Val::Px(18.0)),
            ..default()
        })
        .with_children(|c| {
            for (k, v, d, new) in &s.facts {
                c.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(text(k, Type::LABEL, tokens::MUTED));
                    c.spawn((text(v, Type::barlow(30.0, 600), tokens::INK), TextLayout::no_wrap()));
                    c.spawn((
                        text(
                            d,
                            Type::barlow(14.0, 400),
                            if *new { CLASSIC_LIGHT } else { tokens::MUTED },
                        ),
                        TextLayout::no_wrap(),
                    ));
                });
            }
        });
    });
}

fn details(c: &mut ChildSpawner, s: &Summary) {
    c.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(80.0),
        top: Val::Px(520.0),
        width: Val::Px(1760.0),
        display: Display::Grid,
        grid_template_columns: vec![
            RepeatedGridTrack::px(1, 860.0),
            RepeatedGridTrack::minmax(
                1,
                MinTrackSizingFunction::Px(0.0),
                MaxTrackSizingFunction::Fraction(1.0),
            ),
        ],
        column_gap: Val::Px(80.0),
        ..default()
    })
    .with_children(|c| {
        // Most deaths this session.
        c.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(text(
                locale::text("session_most_deaths"),
                CAPS,
                tokens::MUTED,
            ));
            if s.rooms.is_empty() {
                c.spawn(text(locale::text("pause_no_deaths"), Type::barlow(16.0, 400), tokens::MUTED));
            }
            let max = s.rooms.first().map_or(1, |r| r.1.max(1));
            for (i, (name, n)) in s.rooms.iter().enumerate() {
                let (fill, ink) = if i == 0 {
                    (tokens::CLASSIC, Color::WHITE)
                } else {
                    (tokens::white(0.35), tokens::SOFT)
                };
                c.spawn(Node {
                    display: Display::Grid,
                    grid_template_columns: vec![
                        RepeatedGridTrack::px(1, 240.0),
                        RepeatedGridTrack::minmax(
                            1,
                            MinTrackSizingFunction::Px(0.0),
                            MaxTrackSizingFunction::Fraction(1.0),
                        ),
                        RepeatedGridTrack::px(1, 60.0),
                    ],
                    column_gap: Val::Px(14.0),
                    align_items: AlignItems::Center,
                    height: Val::Px(30.0),
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(crate::ui::widgets::ellipsis_text(name, Type::barlow(16.0, 400), ink));
                    c.spawn((
                        Node {
                            height: Val::Px(10.0),
                            border_radius: BorderRadius::all(Val::Px(5.0)),
                            ..default()
                        },
                        BackgroundColor(tokens::white(0.08)),
                    ))
                    .with_child((
                        Node {
                            width: Val::Percent(100.0 * *n as f32 / max as f32),
                            height: Val::Percent(100.0),
                            border_radius: BorderRadius::all(Val::Px(5.0)),
                            ..default()
                        },
                        BackgroundColor(fill),
                    ));
                    c.spawn((
                        Node {
                            justify_content: JustifyContent::FlexEnd,
                            ..default()
                        },
                        children![text(&card::grouped(u64::from(*n)), Type::barlow(16.0, 600), tokens::INK)],
                    ));
                });
            }
        });
        // The session, start to finish.
        c.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(text(
                locale::text("session_timeline"),
                CAPS,
                tokens::MUTED,
            ));
            c.spawn(Node {
                height: Val::Px(90.0),
                ..default()
            })
            .with_children(|c| {
                c.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        right: Val::Px(0.0),
                        top: Val::Px(44.0),
                        height: Val::Px(2.0),
                        ..default()
                    },
                    BackgroundColor(tokens::white(0.18)),
                ));
                for x in &s.death_marks {
                    c.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(100.0 * x),
                            top: Val::Px(30.0),
                            width: Val::Px(2.0),
                            height: Val::Px(16.0),
                            ..default()
                        },
                        BackgroundColor(DEATH_TICK),
                    ));
                }
                let labelled = s.saves.len().min(SAVE_LABELS);
                let skip = s.saves.len().saturating_sub(labelled);
                for (i, (x, room)) in s.saves.iter().enumerate() {
                    c.spawn(Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(100.0 * x),
                        top: Val::Px(50.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(2.0),
                        // `translateX(-50%)` of a box about 60 px wide.
                        margin: UiRect::left(Val::Px(-30.0)),
                        width: Val::Px(60.0),
                        ..default()
                    })
                    .with_children(|c| {
                        c.spawn((
                            Node {
                                width: Val::Px(12.0),
                                height: Val::Px(12.0),
                                border_radius: BorderRadius::all(Val::Px(3.0)),
                                ..default()
                            },
                            BackgroundColor(tokens::CLASSIC),
                            UiTransform::from_rotation(Rot2::degrees(45.0)),
                        ));
                        if i >= skip {
                            c.spawn((text(room, Type::barlow(12.0, 400), tokens::SOFT), TextLayout::no_wrap()));
                        }
                    });
                }
            });
            c.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|c| {
                let small = Type::barlow(13.0, 400);
                c.spawn(text(&s.start, small, tokens::MUTED));
                c.spawn(text(locale::text("session_timeline_key"), small, tokens::MUTED));
                c.spawn(text(&s.end, small, tokens::MUTED));
            });
            c.spawn((
                Node {
                    margin: UiRect::top(Val::Px(16.0)),
                    max_width: Val::Px(760.0),
                    ..default()
                },
                children![text_leading(
                    locale::text(if s.finished {
                        "session_note_finished"
                    } else {
                        "session_note"
                    }),
                    Type::barlow(16.0, 400),
                    Color::srgb_u8(0xe3, 0xe5, 0xee),
                    1.5
                )],
            ));
        });
    });
}

fn footer(c: &mut ChildSpawner, s: &Summary) {
    let primary = locale::text(if s.finished {
        "session_play_again"
    } else {
        "session_continue"
    });
    c.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(80.0),
        right: Val::Px(80.0),
        bottom: Val::Px(36.0),
        align_items: AlignItems::Center,
        column_gap: Val::Px(12.0),
        ..default()
    })
    .with_children(|c| {
        c.spawn((
            Action::Back,
            button_with(
                ButtonKind::Secondary,
                52.0,
                26.0,
                (
                    Node {
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(10.0),
                        ..default()
                    },
                    children![
                        text("‹", Type::barlow(20.0, 600), tokens::INK),
                        text(locale::text("pause_back_to_shelf"), Type::barlow(17.0, 600), tokens::INK),
                    ],
                ),
            ),
        ));
        c.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        c.spawn((
            Action::Primary,
            button_with(
                ButtonKind::Primary,
                52.0,
                28.0,
                text(primary, Type::barlow(19.0, 700), tokens::INK),
            ),
        ));
    });
}
