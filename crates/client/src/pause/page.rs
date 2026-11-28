//! The pause menu's page (m5 `D13_PauseTimeAttack`, `D14_PauseClassic`, `D16_Pause43Pad`;
//! `Pause.dc.html`, #624): laid out on the artboard (1920 x 1080, or 1366 x 1024 on a 4:3 window)
//! and scaled to the window by `UiScale`, which the HUD gives up while the menu shows
//! (`hud::Suspended`). Left: `Paused`, the status pill and the items, the delicious fruit beside
//! the selected one. Right: the run card, what this run looks like. The hint bar at the bottom.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::Button;

use super::{FRUIT, Item, Menu};
use crate::locale;
use crate::ui::fonts::Type;
use crate::ui::shape::{Shape, Veil};
use crate::ui::tokens;
use crate::ui::widgets::{classic_pill, mod_pill, text, text_leading};

/// One artboard: its size and where the parts sit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Board {
    pub size: Vec2,
    menu_x: f32,
    menu_y: f32,
    menu_w: f32,
    title_size: f32,
    item_h: f32,
    item_size: f32,
    card_x: f32,
    card_w: f32,
    controls_w: f32,
    hint_bottom: f32,
}

/// 16:9 (`ratio=169`).
pub const WIDE: Board = Board {
    size: Vec2::new(1920.0, 1080.0),
    menu_x: 130.0,
    menu_y: 170.0,
    menu_w: 640.0,
    title_size: 104.0,
    item_h: 70.0,
    item_size: 22.0,
    card_x: 900.0,
    card_w: 880.0,
    controls_w: 260.0,
    hint_bottom: 36.0,
};

/// 4:3 (`ratio=43`).
pub const SQUARE: Board = Board {
    size: Vec2::new(1366.0, 1024.0),
    menu_x: 60.0,
    menu_y: 150.0,
    menu_w: 560.0,
    title_size: 84.0,
    item_h: 66.0,
    item_size: 21.0,
    card_x: 680.0,
    card_w: 630.0,
    controls_w: 230.0,
    hint_bottom: 40.0,
};

impl Board {
    /// The board for a window: 4:3 below a 1.5 aspect, else 16:9.
    pub fn for_window(win: Vec2) -> Board {
        if win.x / win.y.max(1.0) < 1.5 { SQUARE } else { WIDE }
    }

    /// The UI scale that fits the board in `win`.
    pub fn scale(&self, win: Vec2) -> f32 {
        (win.y / self.size.y).min(win.x / self.size.x)
    }
}

/// What the card shows, read when the menu opens (`super::card`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub classic: bool,
    pub title: String,
    /// `Classic`, or the map's name.
    pub map_line: String,
    /// The run's mod codes (Time Attack).
    pub mods: Vec<&'static str>,
    /// Three numbers: label, value, note, whether the value is the large one.
    pub big: Vec<(String, String, String)>,
    /// The note under the first number in the accent's light ink (`still running`).
    pub live_note: bool,
    pub rooms_key: String,
    /// Deaths by room, most first, at most four.
    pub rooms: Vec<(String, u32)>,
    /// Glyph, action, whether it is one of the game's extra actions.
    pub controls: Vec<(String, String, bool)>,
}

/// The parts that follow the selection and the hold.
#[derive(Component)]
pub struct Row(pub Item);
#[derive(Component)]
pub struct Selector(pub Item);
#[derive(Component)]
pub struct Plate(pub Item);
#[derive(Component)]
pub struct Label(pub Item);
#[derive(Component)]
pub struct Sub(pub Item);
#[derive(Component)]
pub struct HoldFill(pub Item);
/// The status pill's text (`Timer running · 38:09.02`) and its dot.
#[derive(Component)]
pub struct Status;
#[derive(Component)]
pub struct StatusDot;
/// The first big number (the run time), kept live.
#[derive(Component)]
pub struct LiveTime;

/// The selected row's plate and the others'.
pub fn plate_on() -> Color {
    tokens::white(0.2)
}
pub const PLATE_OFF: Color = Color::srgba(12.0 / 255.0, 14.0 / 255.0, 22.0 / 255.0, 0.55);
/// The selected row's ring.
pub const RING: Color = Color::srgba(1.0, 1.0, 1.0, 0.55);
/// The selected row moves this far right, OutQuint.
pub const SHIFT: f32 = 20.0;
/// `#e3e5ee`.
pub const IDLE_INK: Color = Color::srgb_u8(0xe3, 0xe5, 0xee);
/// The hold fill: `rgba(255,61,85,.45)`.
const HOLD: Color = Color::srgba(1.0, 61.0 / 255.0, 85.0 / 255.0, 0.45);

/// The accent of the mode: Classic's violet or Time Attack's red.
pub fn accent(classic: bool) -> Color {
    if classic { tokens::CLASSIC } else { tokens::ACCENT }
}

/// The whole page, `board.size` large.
pub fn page(
    board: Board,
    menu: &Menu,
    card: &Card,
    status: String,
    fruit: Handle<Image>,
    pad: bool,
) -> impl Bundle + use<> {
    let card = card.clone();
    let items: Vec<Item> = Item::list(menu.classic).to_vec();
    let selected = menu.selected;
    let classic = menu.classic;
    (
        Node {
            width: Val::Px(board.size.x),
            height: Val::Px(board.size.y),
            ..default()
        },
        // The game behind at half brightness (`filter: brightness(.5)`; there is no blur yet), then
        // `linear-gradient(90deg, rgba(8,9,14,.92) 0%, rgba(8,9,14,.55) 45%, rgba(8,9,14,.7) 100%)`
        // over the game.
        Shape {
            shear: 0.0,
            radius: 0.0,
            ..Shape::fill(Color::NONE)
        }
        .with_veil(Veil::across(
            tokens::dark(tokens::GROUND, 0.92),
            tokens::dark(tokens::GROUND, 0.55),
            tokens::dark(tokens::GROUND, 0.7),
            0.45,
        )),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            menu_column(c, board, &items, selected, classic, status, fruit, pad);
            run_card(c, board, &card);
            c.spawn(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(board.hint_bottom),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_child(crate::ui::hints::hints(&hints(classic, pad)));
        })),
    )
}

/// The hint bar: Choose, Continue, Retry from save and Hold to restart (Time Attack).
fn hints(classic: bool, pad: bool) -> Vec<(&'static str, &'static str)> {
    let mut h = vec![
        (if pad { "A" } else { "Enter" }, locale::text("pause_hint_choose")),
        (if pad { "B" } else { "Esc" }, locale::text("pause_continue")),
        (if pad { "Y" } else { "R" }, locale::text("pause_retry_from_save")),
    ];
    if !classic {
        h.push((if pad { "Back" } else { "`" }, locale::text("pause_hint_hold_restart")));
    }
    h
}

#[allow(clippy::too_many_arguments, reason = "the column's parts")]
fn menu_column(
    c: &mut ChildSpawner,
    board: Board,
    items: &[Item],
    selected: Item,
    classic: bool,
    status: String,
    fruit: Handle<Image>,
    pad: bool,
) {
    let accent = accent(classic);
    c.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(board.menu_x),
        top: Val::Px(board.menu_y),
        width: Val::Px(board.menu_w),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(10.0),
        ..default()
    })
    .with_children(|c| {
        c.spawn(text_leading(
            locale::text("pause_paused"),
            Type::condensed(board.title_size, 700).italic(),
            tokens::INK,
            0.9,
        ));
        // The status pill: violet and still in Classic, red and pulsing in Time Attack.
        let pill_fill = if classic {
            tokens::CLASSIC.with_alpha(0.25)
        } else {
            tokens::ACCENT.with_alpha(0.22)
        };
        let dot = if classic {
            Color::srgb_u8(0xb9, 0xad, 0xff)
        } else {
            tokens::ACCENT
        };
        c.spawn(Node {
            align_self: AlignSelf::FlexStart,
            margin: UiRect::new(Val::Px(0.0), Val::Px(0.0), Val::Px(6.0), Val::Px(18.0)),
            ..default()
        })
        .with_child((
            crate::ui::widgets::sheared(36.0, 16.0, Shape::fill(pill_fill)),
            children![
                (
                    Node {
                        width: Val::Px(9.0),
                        height: Val::Px(9.0),
                        border_radius: BorderRadius::all(Val::Px(5.0)),
                        ..default()
                    },
                    BackgroundColor(dot),
                    StatusDot,
                ),
                (text(&status, Type::barlow(16.0, 700), tokens::INK), Status),
            ],
        ));
        for &item in items {
            row(c, board, item, item == selected, classic, fruit.clone(), pad, accent);
        }
    });
}

#[allow(clippy::too_many_arguments, reason = "one row's parts")]
fn row(
    c: &mut ChildSpawner,
    board: Board,
    item: Item,
    on: bool,
    classic: bool,
    fruit: Handle<Image>,
    pad: bool,
    _accent: Color,
) {
    let shift = if on { SHIFT } else { 0.0 };
    let key = item.glyph(classic, pad);
    let sub = item.sub(classic);
    c.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: Val::Px(10.0),
        ..default()
    })
    .with_children(|c| {
        // The fruit, 28 x 32 (the 21 x 24 sprite at 4/3).
        c.spawn((
            Node {
                width: Val::Px(28.0),
                height: Val::Px(32.0),
                flex_shrink: 0.0,
                ..default()
            },
            ImageNode {
                image: fruit,
                rect: Some(Rect::new(0.0, 0.0, FRUIT.x as f32, FRUIT.y as f32)),
                color: Color::WHITE.with_alpha(if on { 1.0 } else { 0.0 }),
                ..default()
            },
            Selector(item),
        ));
        let mut plate = Shape::fill(if on { plate_on() } else { PLATE_OFF });
        if on {
            plate = plate.with_ring(2.0, RING);
        }
        c.spawn((
            Button,
            Hovered::default(),
            Row(item),
            Plate(item),
            Node {
                flex_grow: 1.0,
                height: Val::Px(board.item_h),
                align_items: AlignItems::Center,
                column_gap: Val::Px(14.0),
                padding: UiRect::new(Val::Px(26.0), Val::Px(22.0), Val::Px(0.0), Val::Px(0.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            UiTransform::from_translation(Val2::px(shift, 0.0)),
            crate::ui::motion::Tween::<crate::ui::motion::OffsetX>::at_rest(
                shift,
                crate::ui::tokens::motion::SELECT,
                crate::ui::motion::Ease::OutQuint,
            ),
            plate,
        ))
        .with_children(|c| {
            // The hold fill from the left.
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    width: Val::Percent(0.0),
                    ..default()
                },
                BackgroundColor(HOLD),
                HoldFill(item),
            ));
            c.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(1.0),
                flex_grow: 1.0,
                min_width: Val::Px(0.0),
                ..default()
            })
            .with_children(|c| {
                c.spawn((
                    text(
                        item.label(classic),
                        Type::barlow(board.item_size, if on { 700 } else { 600 }),
                        if on { Color::WHITE } else { IDLE_INK },
                    ),
                    TextLayout::no_wrap(),
                    Label(item),
                ));
                if !sub.is_empty() {
                    c.spawn((
                        text(sub, Type::barlow(13.0, 400), if on { IDLE_INK } else { tokens::MUTED }),
                        TextLayout::no_wrap(),
                        Sub(item),
                    ));
                }
            });
            if !key.is_empty() {
                c.spawn((
                    Node {
                        min_width: Val::Px(28.0),
                        height: Val::Px(28.0),
                        padding: UiRect::horizontal(Val::Px(8.0)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(tokens::white(0.16)),
                    children![(
                        text(&key, Type::barlow(13.0, 700), IDLE_INK),
                        TextLayout::no_wrap()
                    )],
                ));
            }
        });
    });
}

/// The run card on the right.
fn run_card(c: &mut ChildSpawner, board: Board, card: &Card) {
    let accent = accent(card.classic);
    c.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(board.card_x),
        top: Val::Px(board.menu_y),
        width: Val::Px(board.card_w),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(16.0),
        ..default()
    })
    .with_children(|c| {
        // The game, its map line and pills.
        c.spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(12.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .with_children(|c| {
            c.spawn((
                text(&card.title, Type::barlow(24.0, 700), tokens::INK),
                TextLayout::no_wrap(),
            ));
            c.spawn(text(&card.map_line, Type::barlow(16.0, 400), tokens::SOFT));
            if card.classic {
                c.spawn(classic_pill());
            } else {
                for code in &card.mods {
                    c.spawn(mod_pill(code));
                }
            }
        });
        // The three numbers.
        c.spawn(Node {
            column_gap: Val::Px(40.0),
            align_items: AlignItems::FlexEnd,
            ..default()
        })
        .with_children(|c| {
            for (i, (k, v, d)) in card.big.iter().enumerate() {
                let size = if i == 0 { 56.0 } else { 40.0 };
                let note_ink = if i == 0 && card.live_note {
                    tokens::ACCENT_LIGHT
                } else {
                    tokens::MUTED
                };
                c.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(text(k, Type::LABEL, tokens::MUTED));
                    let mut value = c.spawn((
                        text_leading(v, Type::barlow(size, 600), tokens::INK, 1.0),
                        TextLayout::no_wrap(),
                    ));
                    if i == 0 {
                        value.insert(LiveTime);
                    }
                    c.spawn((text(d, Type::barlow(14.0, 600), note_ink), TextLayout::no_wrap()));
                });
            }
        });
        // Deaths by room, and the controls.
        c.spawn(Node {
            display: Display::Grid,
            grid_template_columns: vec![
                RepeatedGridTrack::minmax(1, MinTrackSizingFunction::Px(0.0), MaxTrackSizingFunction::Fraction(1.0)),
                RepeatedGridTrack::px(1, board.controls_w),
            ],
            column_gap: Val::Px(28.0),
            margin: UiRect::top(Val::Px(8.0)),
            ..default()
        })
        .with_children(|c| {
            c.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|c| {
                c.spawn(text(&card.rooms_key, Type::LABEL, tokens::MUTED));
                let max = card.rooms.first().map_or(1, |r| r.1.max(1));
                if card.rooms.is_empty() {
                    c.spawn(text(
                        locale::text("pause_no_deaths"),
                        Type::barlow(14.0, 400),
                        tokens::MUTED,
                    ));
                }
                for (i, (name, n)) in card.rooms.iter().enumerate() {
                    let fill = if i == 0 { accent } else { tokens::white(0.35) };
                    let ink = if i == 0 { Color::WHITE } else { tokens::SOFT };
                    c.spawn(Node {
                        display: Display::Grid,
                        grid_template_columns: vec![
                            RepeatedGridTrack::px(1, 120.0),
                            RepeatedGridTrack::minmax(
                                1,
                                MinTrackSizingFunction::Px(0.0),
                                MaxTrackSizingFunction::Fraction(1.0),
                            ),
                            RepeatedGridTrack::px(1, 40.0),
                        ],
                        column_gap: Val::Px(10.0),
                        align_items: AlignItems::Center,
                        height: Val::Px(26.0),
                        ..default()
                    })
                    .with_children(|c| {
                        c.spawn((
                            crate::ui::widgets::ellipsis_text(name, Type::barlow(14.0, 400), ink),
                        ));
                        c.spawn((
                            Node {
                                height: Val::Px(8.0),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(tokens::white(0.08)),
                        ))
                        .with_child((
                            Node {
                                width: Val::Percent(100.0 * *n as f32 / max as f32),
                                height: Val::Percent(100.0),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(fill),
                        ));
                        c.spawn((
                            Node {
                                justify_content: JustifyContent::FlexEnd,
                                ..default()
                            },
                            children![text(
                                &crate::card::grouped(u64::from(*n)),
                                Type::barlow(14.0, 600),
                                tokens::INK
                            )],
                        ));
                    });
                }
            });
            c.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|c| {
                c.spawn(text(
                    locale::text("pause_controls"),
                    Type::LABEL,
                    tokens::MUTED,
                ));
                for (glyph, action, extra) in &card.controls {
                    c.spawn(Node {
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(10.0),
                        ..default()
                    })
                    .with_children(|c| {
                        c.spawn((
                            Node {
                                min_width: Val::Px(30.0),
                                height: Val::Px(26.0),
                                padding: UiRect::horizontal(Val::Px(7.0)),
                                border_radius: BorderRadius::all(Val::Px(6.0)),
                                border: UiRect::bottom(Val::Px(3.0)),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BackgroundColor(tokens::white(0.88)),
                            BorderColor::all(tokens::black(0.25)),
                            children![(
                                text(glyph, Type::barlow(12.0, 700), tokens::ON_LIGHT),
                                TextLayout::no_wrap()
                            )],
                        ));
                        c.spawn((
                            text(
                                action,
                                Type::barlow(14.0, 600),
                                if *extra { tokens::AMBER } else { IDLE_INK },
                            ),
                            TextLayout::no_wrap(),
                        ));
                    });
                }
            });
        });
    });
}
