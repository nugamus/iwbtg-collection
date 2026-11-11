//! The two HUD panels as the artboards draw them: the live leaderboard (`Leaderboard.dc.html`)
//! on the left and run info (`RunInfo.dc.html`) on the right, at each `Density`.
//!
//! Only the frames and placeholders: the leaderboard's rows and the run's numbers come with
//! their own issues (#37 and the live leaderboard). Sizes are UI px, before `Layout::content`.

use bevy::prelude::*;

use super::layout::Density;
use crate::ui::fonts::Type;
use crate::ui::icons::{Icon, icon};
use crate::ui::shape::{FadeMask, Shape};
use crate::ui::tokens;
use crate::ui::widgets::{sheared, text};

/// Rows the leaderboard shows; yours is the middle one.
const ROWS: usize = 7;
const ROW_HEIGHT: f32 = 44.0;

/// The leaderboard `width` UI px wide.
pub fn leaderboard(width: f32, density: Density) -> impl Bundle {
    let mini = density == Density::Small;
    let header = Type {
        tracking: 0.14,
        caps: true,
        ..Type::barlow(12.0, 700)
    };
    let rows: Vec<_> = (0..ROWS).map(|i| row(width, density, i == ROWS / 2)).collect();
    (
        Node {
            width: Val::Px(width),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            ..default()
        },
        Children::spawn((
            Spawn((
                Node {
                    height: Val::Px(18.0),
                    padding: UiRect::left(Val::Px(10.0)),
                    align_items: AlignItems::Center,
                    display: if mini { Display::None } else { Display::Flex },
                    ..default()
                },
                children![text("Global", header, tokens::SOFT)],
            )),
            Spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    ..default()
                },
                FadeMask {
                    top: ROW_HEIGHT,
                    bottom: ROW_HEIGHT,
                },
                Children::spawn(SpawnIter(rows.into_iter())),
            )),
        )),
    )
}

/// One row: placeholders for the others, `You` at #1 and the run's time for yours.
fn row(width: f32, density: Density, you: bool) -> impl Bundle + use<> {
    let mini = density == Density::Small;
    let w = match (mini, you) {
        (true, true) => width,
        (true, false) => 84.0,
        (false, true) => width - 10.0,
        (false, false) => width - 30.0,
    };
    let shape = if you {
        Shape::fill(tokens::ROW_YOU).with_shadow(6.0, 18.0, Color::srgba(0.0, 0.0, 0.0, 0.35))
    } else {
        Shape::fill(tokens::ROW)
    };
    let (mut node, shape) = sheared(ROW_HEIGHT, 0.0, shape);
    node.width = Val::Px(w);
    node.margin = UiRect::left(Val::Px(10.0));
    node.justify_content = JustifyContent::FlexStart;
    node.padding = if mini {
        UiRect::horizontal(Val::Px(8.0))
    } else {
        UiRect::new(Val::Px(10.0), Val::Px(12.0), Val::ZERO, Val::ZERO)
    };
    node.column_gap = Val::Px(if mini { 6.0 } else { 10.0 });
    let rank_color = if you { tokens::INK } else { tokens::MUTED };
    (
        node,
        shape,
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn((
                Node {
                    width: Val::Px(if mini { 28.0 } else { 34.0 }),
                    flex_shrink: 0.0,
                    ..default()
                },
                children![text(
                    if you { "#1" } else { "–" },
                    Type::condensed(16.0, 700),
                    rank_color
                )],
            ));
            c.spawn(avatar());
            if mini {
                if you {
                    c.spawn(text("0:00.00", Type::barlow(13.0, 600), tokens::INK));
                }
                return;
            }
            let mut col = c.spawn(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                min_width: Val::Px(0.0),
                row_gap: Val::Px(if you { 1.0 } else { 7.0 }),
                ..default()
            });
            if you {
                col.with_children(|c| {
                    c.spawn(text("You", Type::barlow(15.0, 700), tokens::INK));
                    c.spawn(text("0:00.00", Type::barlow(14.0, 700), tokens::INK));
                });
            } else {
                col.with_children(|c| {
                    c.spawn(skeleton(Val::Percent(70.0), 9.0));
                    c.spawn(skeleton(Val::Percent(45.0), 8.0));
                });
            }
        })),
    )
}

/// The 30 px avatar square; until profiles exist, the no-avatar mark.
fn avatar() -> impl Bundle {
    (
        Node {
            width: Val::Px(30.0),
            height: Val::Px(30.0),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(7.0)),
            ..default()
        },
        BackgroundColor(tokens::AVATAR_EMPTY),
        children![icon(Icon::Person, 16.0, tokens::AVATAR_MARK)],
    )
}

/// A rounded bar standing in for a line of text.
fn skeleton(width: Val, height: f32) -> impl Bundle {
    (
        Node {
            width,
            height: Val::Px(height),
            border_radius: BorderRadius::all(Val::Px(height / 2.0)),
            ..default()
        },
        BackgroundColor(tokens::SKELETON),
    )
}

/// Run info `width` UI px wide: timer, stats and (unless small) the input display.
pub fn run_info(width: f32, density: Density) -> impl Bundle {
    let (timer, cs, value, gap) = match density {
        Density::Full => (58.0, 36.0, 24.0, 18.0),
        Density::Compact => (44.0, 28.0, 20.0, 18.0),
        Density::Small => (30.0, 19.0, 16.0, 10.0),
    };
    let stack = density == Density::Small;
    let label = Type {
        tracking: 0.14,
        caps: true,
        ..Type::barlow(11.0, 700)
    };
    let stat = move |name: &str, shown: &str| {
        (
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            children![
                text(name, label, tokens::MUTED),
                text(shown, Type::barlow(value, 600), tokens::INK),
            ],
        )
    };
    (
        Node {
            width: Val::Px(width),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(gap),
            ..default()
        },
        children![
            (
                Node {
                    align_items: AlignItems::Baseline,
                    ..default()
                },
                children![
                    text("0:00", Type::condensed(timer, 600), tokens::INK),
                    text(".00", Type::condensed(cs, 600), tokens::SOFT),
                ],
            ),
            (
                Node {
                    display: Display::Grid,
                    grid_template_columns: if stack {
                        vec![RepeatedGridTrack::flex(1, 1.0)]
                    } else {
                        vec![RepeatedGridTrack::flex(2, 1.0)]
                    },
                    row_gap: Val::Px(if stack { 6.0 } else { 14.0 }),
                    column_gap: Val::Px(16.0),
                    ..default()
                },
                children![
                    stat("Deaths", "–"),
                    stat("Stage", "– / –"),
                    stat("This stage", "–"),
                ],
            ),
            (
                Node {
                    display: if stack { Display::None } else { Display::Flex },
                    ..default()
                },
                children![input_display()],
            ),
        ],
    )
}

/// The input display, nothing held yet: four arrows, then Jump, Shoot and Restart.
fn input_display() -> impl Bundle {
    let arrows = [-90.0f32, 0.0, 180.0, 90.0].map(|deg| {
        (
            Node {
                width: Val::Px(26.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(6.0),
                ..default()
            },
            children![
                (
                    icon(Icon::Arrow, 26.0, tokens::KEY_IDLE),
                    UiTransform {
                        rotation: Rot2::degrees(deg),
                        ..default()
                    },
                ),
                bar(),
            ],
        )
    });
    let style = Type {
        tracking: 0.08,
        caps: true,
        ..Type::barlow(13.0, 700)
    };
    let buttons = ["Jump", "Shoot", "Restart"].map(|label| {
        (
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            },
            children![text(label, style, tokens::BUTTON_IDLE), bar()],
        )
    });
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            padding: UiRect::top(Val::Px(10.0)),
            ..default()
        },
        children![
            (
                Node {
                    column_gap: Val::Px(18.0),
                    ..default()
                },
                Children::spawn(SpawnIter(arrows.into_iter())),
            ),
            (
                Node {
                    flex_wrap: FlexWrap::Wrap,
                    row_gap: Val::Px(10.0),
                    column_gap: Val::Px(14.0),
                    ..default()
                },
                Children::spawn(SpawnIter(buttons.into_iter())),
            ),
        ],
    )
}

/// The 18x2 bar under a key; the accent while it is held (transparent until #37 wires input).
fn bar() -> impl Bundle {
    (
        Node {
            width: Val::Px(18.0),
            height: Val::Px(2.0),
            border_radius: BorderRadius::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::NONE),
    )
}
