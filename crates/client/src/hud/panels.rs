//! The two HUD panels as the artboards draw them: the live leaderboard (`Leaderboard.dc.html`)
//! on the left and run info (`RunInfo.dc.html`) on the right, at each `Density`.
//!
//! The leaderboard is a frame with placeholders until the live leaderboard lands; your row shows
//! the run's time. Run info is `run_info.rs`. Sizes are UI px, before `Layout::content`.

use bevy::prelude::*;

use super::layout::Density;
use super::run_info::Field;
use crate::ui::fonts::Type;
use crate::ui::icons::{Icon, icon};
use crate::ui::shape::{FadeMask, Shape};
use crate::ui::tokens;
use crate::ui::widgets::{sheared, text};

/// Rows the leaderboard shows; yours is the middle one.
const ROWS: usize = 7;
const ROW_HEIGHT: f32 = 44.0;
/// How far a sheared row reaches past its box on each side.
const SHEAR_OVERHANG: f32 = tokens::SHEAR * ROW_HEIGHT / 2.0;
/// The small leaderboard's rows start this far in (`HudSmall` places the list at -4 with a 10 px
/// indent: 6 px), and its rank, gaps and padding are tighter than the artboard's so your row
/// (rank, avatar, time) fits inside the 124 px column.
const MINI_INDENT: f32 = 6.0;

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
        // `HudSmall` draws it the column's full width after the indent, which overhangs the game
        // by 6 px plus the shear; here it stops at the column's edge, shear included.
        (true, true) => width - MINI_INDENT - SHEAR_OVERHANG,
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
    node.margin = UiRect::left(Val::Px(if mini { MINI_INDENT } else { 10.0 }));
    node.justify_content = JustifyContent::FlexStart;
    node.padding = if mini {
        UiRect::new(Val::Px(8.0), Val::Px(4.0), Val::ZERO, Val::ZERO)
    } else {
        UiRect::new(Val::Px(10.0), Val::Px(12.0), Val::ZERO, Val::ZERO)
    };
    node.column_gap = Val::Px(if mini { 4.0 } else { 10.0 });
    let rank_color = if you { tokens::INK } else { tokens::MUTED };
    (
        node,
        shape,
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn((
                Node {
                    width: Val::Px(if mini { 22.0 } else { 34.0 }),
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
                    c.spawn((
                        text("0:00.00", Type::barlow(13.0, 600), tokens::INK),
                        Field::YourTime,
                    ));
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
                    c.spawn((
                        text("0:00.00", Type::barlow(14.0, 700), tokens::INK),
                        Field::YourTime,
                    ));
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
