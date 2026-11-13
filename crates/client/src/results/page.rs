//! The results page from its artboards (`Results169.dc.html` at 1920×1080, the same file's `43`
//! layout at 1024×768): header; the frozen leaderboard on the left; the hero (time, placement,
//! rank change) and the player card, then the splits table and the deaths per stage, the death
//! timeline and (a setting, 16:10 and wider) the heatmap; the footer.
//! No bordered panels: spacing, type and quiet colour carry the hierarchy. Sizes are the
//! artboard's CSS px; `results::show` scales the page to the window.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::LineHeight;
use bevy::ui_render::prelude::MaterialNode;

use super::{Action, Heat, Stage, Summary, bar_label};
use crate::card::{self, Variant};
use crate::hud::layout::Density;
use crate::hud::leaderboard::{self, Live};
use crate::pause::FRUIT;
use crate::stats::fmt_ticks;
use crate::ui::filter::FilterMaterial;
use crate::ui::fonts::Type;
use crate::ui::shape::Shape;
use crate::ui::tokens;
use crate::ui::widgets::{ButtonKind, button_off, button_with, sheared, text, unranked_pill};

/// A splits row: hovering or clicking it shows that stage's heatmap.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageRow(pub usize);

/// Behind splits row `.0`: lit ([`tokens::ROW_PAST`]) while the heatmap shows that stage. Not in
/// the artboard, which has no design for it (#705); ours, cheap to undo.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowLight(pub usize);

/// At the left of splits row `.0`: the accent's selection mark while that row is the clicked one
/// (#705, ours as well).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowMark(pub usize);

/// How far the row light reaches past the table on each side, so the text does not touch its edge.
const ROW_LIGHT_PAD: f32 = 10.0;

/// The heatmap's place on the page; `results::heat` fills it for the stage shown (`drawn`).
#[derive(Component, Default)]
pub struct HeatPanel {
    pub drawn: Option<Option<usize>>,
}

/// The heatmap's room picture: the artboard's `224px` wide box (an 800×608 room at `.28`).
const HEAT_W: f32 = 224.0;
/// The tallest the picture gets, for rooms taller than they are wide.
const HEAT_MAX_H: f32 = 224.0;
/// The picture's corner radius (`6px`); the row light uses it too.
const HEAT_RADIUS: f32 = 6.0;

/// The two layouts of the artboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// `Results169`: 16:10 and wider.
    Wide,
    /// `Results43`: narrower than 16:10.
    Compact,
}

impl Size {
    /// The layout for a window `size` px: the player card's rule (`Variant::for_results`).
    pub fn for_window(size: Vec2) -> Size {
        match Variant::for_results(size.x / size.y.max(1.0)) {
            Variant::ResultsCompact => Size::Compact,
            _ => Size::Wide,
        }
    }

    /// The artboard's size, which the page is laid out at and scaled from.
    pub fn design(self) -> Vec2 {
        match self {
            Size::Wide => Vec2::new(1920.0, 1080.0),
            Size::Compact => Vec2::new(1024.0, 768.0),
        }
    }

    fn dims(self) -> Dims {
        match self {
            Size::Wide => Dims {
                pad: [40.0, 64.0, 36.0, 54.0],
                rivals: 300.0,
                col_gap: 72.0,
                row_gap: 28.0,
                title: 30.0,
                small: 15.0,
                section_gap: 34.0,
                hero_gap: 36.0,
                hero: 156.0,
                hero_cs: 84.0,
                place: 64.0,
                change_max: 760.0,
                change: 24.0,
                split_fr: 1.05,
                detail_gap: 72.0,
                table: 17.0,
                row: 40.0,
                head: 26.0,
                chart_gap: 30.0,
                chart: 120.0,
                timeline: 56.0,
                button: 48.0,
                button_text: 17.0,
                card: Variant::Results,
            },
            Size::Compact => Dims {
                pad: [20.0, 24.0, 18.0, 14.0],
                rivals: 206.0,
                col_gap: 26.0,
                row_gap: 14.0,
                title: 22.0,
                small: 13.0,
                section_gap: 16.0,
                hero_gap: 20.0,
                hero: 84.0,
                hero_cs: 46.0,
                place: 34.0,
                change_max: 480.0,
                change: 17.0,
                split_fr: 1.1,
                detail_gap: 28.0,
                table: 13.0,
                row: 27.0,
                head: 20.0,
                chart_gap: 16.0,
                chart: 92.0,
                timeline: 40.0,
                button: 38.0,
                button_text: 14.0,
                card: Variant::ResultsCompact,
            },
        }
    }
}

/// The artboard's numbers for one layout (`renderVals`).
#[derive(Clone, Copy)]
struct Dims {
    /// Top, right, bottom, left.
    pad: [f32; 4],
    rivals: f32,
    col_gap: f32,
    row_gap: f32,
    title: f32,
    small: f32,
    section_gap: f32,
    hero_gap: f32,
    hero: f32,
    hero_cs: f32,
    place: f32,
    change_max: f32,
    change: f32,
    split_fr: f32,
    detail_gap: f32,
    table: f32,
    row: f32,
    head: f32,
    chart_gap: f32,
    chart: f32,
    /// The death timeline's height (`tlH`).
    timeline: f32,
    button: f32,
    button_text: f32,
    card: Variant,
}

/// How the page is drawn: its layout, the delicious fruit (the pause menu's: the package's
/// `cherry`, else our placeholder), whether the replay was saved and whether the heatmap setting
/// is on (it shows only in the wide layout, as on the artboard).
pub struct Look {
    pub size: Size,
    pub fruit: Option<Handle<Image>>,
    pub saved: bool,
    pub heatmap: bool,
}

/// Small capitals over a value (`PERSONAL BEST`, `DEATHS PER STAGE`): the kit's label.
const LABEL: Type = Type::LABEL;

/// The whole page, filling its parent.
pub fn page(s: &Summary, look: &Look) -> impl Bundle + use<> {
    let d = look.size.dims();
    let s = s.clone();
    let fruit = look.fruit.clone();
    let saved = look.saved;
    let heatmap = look.heatmap && look.size == Size::Wide;
    (
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(d.row_gap),
            padding: UiRect {
                top: Val::Px(d.pad[0]),
                right: Val::Px(d.pad[1]),
                bottom: Val::Px(d.pad[2]),
                left: Val::Px(d.pad[3]),
            },
            ..default()
        },
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn(header(&s, &d));
            c.spawn(Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: Val::Px(d.col_gap),
                ..default()
            })
            .with_children(|c| {
                c.spawn(rivals(&s, &d));
                c.spawn(Node {
                    flex_grow: 1.0,
                    flex_basis: Val::Px(0.0),
                    min_width: Val::Px(0.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(d.section_gap),
                    ..default()
                })
                .with_children(|c| {
                    hero(c, &s, &d, fruit.clone());
                    c.spawn(Node {
                        flex_grow: 1.0,
                        min_height: Val::Px(0.0),
                        column_gap: Val::Px(d.detail_gap),
                        ..default()
                    })
                    .with_children(|c| {
                        splits(c, &s, &d);
                        deaths(c, &s, &d, heatmap);
                    });
                });
            });
            footer(c, &s, &d, saved);
        })),
    )
}

/// `I wanna be the Kaenbyou   Very Hard · Any%   Cleared 3 Nov 2026, 21:14`.
fn header(s: &Summary, d: &Dims) -> impl Bundle + use<> {
    (
        Node {
            align_items: AlignItems::Baseline,
            column_gap: Val::Px(16.0),
            padding: UiRect::left(Val::Px(10.0)),
            flex_shrink: 0.0,
            ..default()
        },
        children![
            (
                text(&s.about.title, Type::condensed(d.title, 700), tokens::INK),
                TextLayout::no_wrap()
            ),
            (
                text(&s.about.subtitle, Type::barlow(d.small, 600), tokens::SOFT),
                TextLayout::no_wrap()
            ),
            (
                text(&s.cleared(), Type::barlow(d.small, 400), tokens::MUTED),
                TextLayout::no_wrap()
            ),
        ],
    )
}

/// The live leaderboard, frozen at this run's place with #1 pinned.
fn rivals(s: &Summary, d: &Dims) -> impl Bundle + use<> {
    let live = Live::frozen(s.others.clone(), s.about.rate, s.ticks);
    let density = if d.rivals >= 240.0 {
        Density::Full
    } else {
        Density::Compact
    };
    (
        Node {
            width: Val::Px(d.rivals),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            ..default()
        },
        children![leaderboard::build(d.rivals, density, &live)],
    )
}

/// Text with the hero's tight line height (`line-height: .82`).
fn tight(c: &mut ChildSpawner, content: &str, style: Type, color: Color) {
    c.spawn((text(content, style, color), TextLayout::no_wrap()))
        .insert(LineHeight::RelativeToFont(0.82));
}

/// The time, the PB chip and placement beside it, the rank change under it; the card right.
fn hero(c: &mut ChildSpawner, s: &Summary, d: &Dims, fruit: Option<Handle<Image>>) {
    let (main, cs) = s.hero();
    let d = *d;
    let s = s.clone();
    c.spawn(Node {
        align_items: AlignItems::FlexStart,
        column_gap: Val::Px(d.hero_gap),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|c| {
        c.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(d.section_gap),
            min_width: Val::Px(0.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(Node {
                align_items: AlignItems::FlexEnd,
                column_gap: Val::Px(d.hero_gap),
                ..default()
            })
            .with_children(|c| {
                c.spawn(Node {
                    align_items: AlignItems::Baseline,
                    ..default()
                })
                .with_children(|c| {
                    let big = Type {
                        tracking: -0.02,
                        ..Type::condensed(d.hero, 600)
                    };
                    tight(c, &main, big, tokens::INK);
                    tight(c, &cs, Type::condensed(d.hero_cs, 600), tokens::SOFT);
                });
                c.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    padding: UiRect::bottom(Val::Px(4.0)),
                    ..default()
                })
                .with_children(|c| {
                    if s.is_pb {
                        c.spawn(pb_chip(fruit.clone()));
                    } else {
                        let not = Type {
                            tracking: 0.12,
                            caps: true,
                            ..Type::barlow(d.small, 700)
                        };
                        c.spawn((text("Not a PB", not, tokens::MUTED), TextLayout::no_wrap()));
                    }
                    match s.placement() {
                        Some((place, color)) => {
                            let label = Type {
                                tracking: 0.14,
                                caps: true,
                                ..Type::barlow(d.small, 700)
                            };
                            c.spawn(Node {
                                align_items: AlignItems::Baseline,
                                column_gap: Val::Px(10.0),
                                ..default()
                            })
                            .with_children(|c| {
                                c.spawn((
                                    text(&place, Type::condensed(d.place, 700), color),
                                    TextLayout::no_wrap(),
                                ))
                                .insert(LineHeight::RelativeToFont(1.0));
                                c.spawn((text("Local", label, tokens::SOFT), TextLayout::no_wrap()));
                            });
                        }
                        None => {
                            c.spawn((
                                Node {
                                    align_self: AlignSelf::FlexStart,
                                    ..default()
                                },
                                children![unranked_pill()],
                            ));
                        }
                    }
                });
            });
            c.spawn(Node {
                max_width: Val::Px(d.change_max),
                column_gap: Val::Px(24.0),
                ..default()
            })
            .with_children(|c| {
                for change in s.changes() {
                    c.spawn(Node {
                        flex_grow: 1.0,
                        flex_basis: Val::Px(0.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        ..default()
                    })
                    .with_children(|c| {
                        c.spawn((text(change.label, LABEL, tokens::MUTED), TextLayout::no_wrap()));
                        value(c, &change.value, Type::barlow(d.change, 600));
                        c.spawn((
                            text(&change.note, Type::barlow(d.small, 600), change.color),
                            TextLayout::no_wrap(),
                        ));
                    });
                }
            });
        });
        c.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        c.spawn(card::card(&s.card, d.card));
    });
}

/// A rank change value (`a → b` sets the arrow in text, from the Inter arrows fallback).
fn value(c: &mut ChildSpawner, value: &str, style: Type) {
    c.spawn((text(value, style, tokens::INK), TextLayout::no_wrap()));
}

/// The white `NEW PB` chip with the delicious fruit.
fn pb_chip(fruit: Option<Handle<Image>>) -> impl Bundle {
    let style = Type {
        tracking: 0.12,
        caps: true,
        ..Type::barlow(13.0, 700)
    };
    let mut chip = sheared(26.0, 12.0, Shape::fill(tokens::INK));
    chip.0.align_self = AlignSelf::FlexStart;
    chip.0.column_gap = Val::Px(7.0);
    (
        chip,
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            if let Some(image) = fruit {
                c.spawn((
                    Node {
                        width: Val::Px(14.0),
                        height: Val::Px(16.0),
                        ..default()
                    },
                    ImageNode {
                        image,
                        rect: Some(Rect::new(0.0, 0.0, FRUIT.x as f32, FRUIT.y as f32)),
                        ..default()
                    },
                ));
            }
            c.spawn(text("New PB", style, tokens::GROUND));
        })),
    )
}

/// The splits table's column weights (`1.2fr 1fr 1fr 1fr`).
const COLUMNS: [f32; 4] = [1.2, 1.0, 1.0, 1.0];

/// One row of the splits table: four cells, the first left-aligned, the rest right.
fn table_row(
    c: &mut ChildSpawner,
    node: Node,
    rule: Option<Color>,
    tag: Option<StageRow>,
    cells: [Option<(String, Type, Color)>; 4],
) {
    let mut row = c.spawn(node);
    if let Some(color) = rule {
        row.insert(BorderColor::all(color));
    }
    if let Some(tag) = tag {
        row.insert((tag, Hovered::default()));
        // Behind the cells: the light and the mark `results::light_rows` turns on.
        row.with_children(|c| {
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-ROW_LIGHT_PAD),
                    right: Val::Px(-ROW_LIGHT_PAD),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    border_radius: BorderRadius::all(Val::Px(HEAT_RADIUS)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                RowLight(tag.0),
            ));
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-ROW_LIGHT_PAD),
                    top: Val::Percent(25.0),
                    width: Val::Px(3.0),
                    height: Val::Percent(50.0),
                    border_radius: BorderRadius::all(Val::Px(1.5)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                RowMark(tag.0),
            ));
        });
    }
    row.with_children(|c| {
        for (i, cell) in cells.into_iter().enumerate() {
            c.spawn(Node {
                flex_grow: COLUMNS[i],
                flex_basis: Val::Px(0.0),
                min_width: Val::Px(0.0),
                justify_content: if i == 0 {
                    JustifyContent::FlexStart
                } else {
                    JustifyContent::FlexEnd
                },
                overflow: Overflow::clip_x(),
                ..default()
            })
            .with_children(|c| {
                if let Some((t, style, color)) = cell {
                    c.spawn((text(&t, style, color), TextLayout::no_wrap()));
                }
            });
        }
    });
}

/// Stage, segment, split and ± vs PB per segment; sum of best and possible time save; the legend.
fn splits(c: &mut ChildSpawner, s: &Summary, d: &Dims) {
    let rate = s.about.rate;
    let t = move |ticks| fmt_ticks(ticks, rate);
    let row = |height: f32, rule: bool| Node {
        height: Val::Px(height),
        flex_shrink: 0.0,
        align_items: AlignItems::Center,
        border: if rule {
            UiRect::top(Val::Px(1.0))
        } else {
            UiRect::ZERO
        },
        ..default()
    };
    let size = d.table;
    c.spawn(Node {
        flex_grow: d.split_fr,
        flex_basis: Val::Px(0.0),
        min_width: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        ..default()
    })
    .with_children(|c| {
        let head = |label: &str| Some((label.to_owned(), LABEL, tokens::MUTED));
        table_row(
            c,
            Node {
                height: Val::Px(d.head),
                flex_shrink: 0.0,
                align_items: AlignItems::FlexEnd,
                padding: UiRect::bottom(Val::Px(6.0)),
                ..default()
            },
            None,
            None,
            [head("Stage"), head("Segment"), head("Split"), head("vs PB")],
        );
        // Many stages: the rows past the room are cut, the totals stay.
        c.spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_shrink: 1.0,
            min_height: Val::Px(0.0),
            overflow: Overflow::clip_y(),
            ..default()
        })
        .with_children(|c| {
            for (i, stage) in s.stages.iter().enumerate() {
                let segment = if stage.gold {
                    (
                        format!("★ {}", t(stage.segment)),
                        Type::barlow(size, 700),
                        tokens::GOLD,
                    )
                } else {
                    (t(stage.segment), Type::barlow(size, 500), tokens::INK)
                };
                let delta = match stage.delta {
                    Some(dt) => (
                        leaderboard::fmt_delta(dt, rate),
                        Type::barlow(size, 600),
                        if dt < 0 { tokens::AHEAD } else { tokens::BEHIND },
                    ),
                    None => ("—".to_owned(), Type::barlow(size, 600), tokens::MUTED),
                };
                table_row(
                    c,
                    row(d.row, true),
                    Some(tokens::RULE),
                    Some(StageRow(i)),
                    [
                        Some((stage.name.clone(), Type::barlow(size, 600), tokens::INK)),
                        Some(segment),
                        Some((t(stage.split), Type::barlow(size, 400), tokens::SOFT)),
                        Some(delta),
                    ],
                );
            }
        });
        let total = |ticks: Option<u64>| {
            Some((
                ticks.map_or("—".to_owned(), t),
                Type::barlow(size, 600),
                tokens::INK,
            ))
        };
        let label = |l: &str| Some((l.to_owned(), Type::barlow(size, 400), tokens::SOFT));
        table_row(
            c,
            row(d.row, true),
            Some(tokens::RULE_STRONG),
            None,
            [label("Sum of best"), None, total(s.sum_of_best), None],
        );
        table_row(
            c,
            row(d.row, false),
            None,
            None,
            [label("Possible time save"), None, total(s.time_save()), None],
        );
        c.spawn((
            Node {
                padding: UiRect::top(Val::Px(8.0)),
                flex_shrink: 0.0,
                ..default()
            },
            children![(
                text(
                    "★ gold = best segment ever · − ahead of PB · + behind PB",
                    Type::barlow(12.0, 400),
                    tokens::MUTED
                ),
                TextLayout::no_wrap()
            )],
        ));
    });
}

/// Deaths per stage: a bar per segment, the most deaths brightest, the count over each bar. Under
/// it the death timeline and, with `heatmap`, the heatmap's panel.
fn deaths(c: &mut ChildSpawner, s: &Summary, d: &Dims, heatmap: bool) {
    let most = s.stages.iter().map(|st| st.deaths).max().unwrap_or(0);
    let total: u64 = s.stages.iter().map(|st| u64::from(st.deaths)).sum();
    let columns = |c: &mut ChildSpawner, height: Option<f32>, f: &dyn Fn(&mut ChildSpawner, usize)| {
        let mut node = Node {
            column_gap: Val::Px(10.0),
            ..default()
        };
        if let Some(h) = height {
            node.height = Val::Px(h);
            node.align_items = AlignItems::FlexEnd;
            node.border = UiRect::bottom(Val::Px(1.0));
        }
        let mut row = c.spawn(node);
        if height.is_some() {
            row.insert(BorderColor::all(tokens::AXIS));
        }
        row.with_children(|c| {
            for i in 0..s.stages.len() {
                f(c, i);
            }
        });
    };
    c.spawn(Node {
        flex_grow: 1.0,
        flex_basis: Val::Px(0.0),
        min_width: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(d.chart_gap),
        ..default()
    })
    .with_children(|c| {
        c.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Baseline,
                ..default()
            })
            .with_children(|c| {
                c.spawn((
                    text("Deaths per stage", LABEL, tokens::MUTED),
                    TextLayout::no_wrap(),
                ));
                c.spawn((
                    text(
                        &format!("{} total", card::grouped(total)),
                        Type::barlow(d.small, 600),
                        tokens::INK,
                    ),
                    TextLayout::no_wrap(),
                ));
            });
            columns(c, Some(d.chart), &|c, i| {
                let n = s.stages[i].deaths;
                let share = if most == 0 {
                    0.0
                } else {
                    (n as f32 / most as f32 * 82.0).round()
                };
                let fill = if n == most && most > 0 {
                    tokens::BAR_PEAK
                } else {
                    tokens::BAR
                };
                c.spawn(Node {
                    flex_grow: 1.0,
                    flex_basis: Val::Px(0.0),
                    min_width: Val::Px(0.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::FlexEnd,
                    row_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|c| {
                    c.spawn((
                        text(&n.to_string(), Type::barlow(12.0, 600), tokens::SOFT),
                        TextLayout::no_wrap(),
                    ));
                    c.spawn((
                        Node {
                            width: Val::Percent(60.0),
                            height: Val::Percent(share),
                            border_radius: BorderRadius::top(Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(fill),
                    ));
                });
            });
            columns(c, None, &|c, i| {
                c.spawn((
                    Node {
                        flex_grow: 1.0,
                        flex_basis: Val::Px(0.0),
                        min_width: Val::Px(0.0),
                        justify_content: JustifyContent::Center,
                        overflow: Overflow::clip_x(),
                        ..default()
                    },
                    children![(
                        text(
                            bar_label(&s.stages[i].name),
                            Type::barlow(12.0, 400),
                            tokens::MUTED
                        ),
                        TextLayout::no_wrap()
                    )],
                ));
            });
        });
        timeline(c, s, d);
        if heatmap {
            c.spawn((
                Node {
                    flex_shrink: 0.0,
                    ..default()
                },
                HeatPanel::default(),
            ));
        }
    });
}

/// `0:00 → 38:12.46` over a base line with a tick per death (taller where more died) and the
/// stage boundaries taller and wider (the artboard's SVG, its 1000×40 box stretched to the width).
fn timeline(c: &mut ChildSpawner, s: &Summary, d: &Dims) {
    let (ticks, bounds) = s.timeline();
    let end = fmt_ticks(s.ticks, s.about.rate);
    let small = Type::barlow(12.0, 400);
    let line = |c: &mut ChildSpawner, left: f32, top: f32, w: f32, h: f32, color: Color| {
        c.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(left * 100.0),
                top: Val::Percent(top * 100.0),
                width: Val::Px(w),
                height: Val::Percent(h * 100.0),
                margin: UiRect::left(Val::Px(-w / 2.0)),
                ..default()
            },
            BackgroundColor(color),
        ));
    };
    c.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(8.0),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|c| {
        c.spawn(Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|c| {
            c.spawn((
                text("Death timeline", LABEL, tokens::MUTED),
                TextLayout::no_wrap(),
            ));
            c.spawn((
                text(&format!("0:00 → {end}"), small, tokens::MUTED),
                TextLayout::no_wrap(),
            ));
        });
        c.spawn(Node {
            height: Val::Px(d.timeline),
            width: Val::Percent(100.0),
            ..default()
        })
        .with_children(|c| {
            // The base line: `y 19.5`, 1 of 40 high.
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Percent(48.75),
                    height: Val::Px(1.0),
                    ..default()
                },
                BackgroundColor(tokens::AXIS),
            ));
            // Boundaries: `M x 2 v36`, stroke 2.
            for b in bounds {
                line(c, b, 0.05, 2.0, 0.9, tokens::BAR_PEAK);
            }
            for t in ticks {
                line(c, t.at, 0.5 - t.height / 2.0, 1.0, t.height, tokens::BAR);
            }
        });
    });
}

/// The heatmap panel's contents for `stage`: the room picture, dimmed, with a soft red dot per
/// spot where deaths gathered, and what it shows beside it.
pub fn heat_contents(
    stage: Option<&Stage>,
    heat: Option<&Heat>,
    picture: Option<(MaterialNode<FilterMaterial>, Vec2)>,
) -> impl Bundle + use<> {
    let label = match stage {
        Some(st) => format!("Where you died · {}", st.name),
        None => "Where you died".to_owned(),
    };
    let body = heat_note(stage.map_or(0, |s| s.deaths), heat.map(|h| h.room_deaths));
    let heat = heat.cloned();
    (
        Node {
            column_gap: Val::Px(20.0),
            align_items: AlignItems::FlexStart,
            ..default()
        },
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            heat_parts(c, &label, &body, heat.as_ref(), picture);
        })),
    )
}

fn heat_parts(
    c: &mut ChildSpawner,
    label: &str,
    body: &str,
    heat: Option<&Heat>,
    picture: Option<(MaterialNode<FilterMaterial>, Vec2)>,
) {
    if let (Some(h), Some((image, room))) = (heat, picture) {
        let scale = HEAT_W / room.x.max(1.0);
        let height = (room.y * scale).min(HEAT_MAX_H);
        c.spawn(Node {
            width: Val::Px(HEAT_W),
            height: Val::Px(height),
            flex_shrink: 0.0,
            overflow: Overflow::clip(),
            border_radius: BorderRadius::all(Val::Px(HEAT_RADIUS)),
            ..default()
        })
        .with_children(|c| {
            // `filter: saturate(.4) brightness(.7)`, drawn by the kit's filter (`results::heat`).
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(room.x * scale),
                    height: Val::Px(room.y * scale),
                    // The clip above is square; the filter rounds the picture itself.
                    border_radius: BorderRadius::all(Val::Px(HEAT_RADIUS)),
                    ..default()
                },
                image,
            ));
            for dot in &h.dots {
                // The artboard's dots: 19 to 40 px across, alpha .3 to .55.
                let r = 18.0 + 22.0 * dot.weight;
                // Red over the dimmed room, blended as the browser does (#627).
                let color = tokens::over(tokens::HEAT, 0.3 + 0.25 * dot.weight, tokens::GROUND);
                c.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(dot.x * scale - r / 2.0),
                        top: Val::Px(dot.y * scale - r / 2.0),
                        width: Val::Px(r),
                        height: Val::Px(r),
                        ..default()
                    },
                    BackgroundGradient::from(RadialGradient::new(
                        UiPosition::CENTER,
                        RadialGradientShape::ClosestSide,
                        vec![
                            ColorStop::new(color, Val::Percent(0.0)),
                            ColorStop::new(color.with_alpha(0.0), Val::Percent(100.0)),
                        ],
                    )),
                ));
            }
        });
    }
    c.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(6.0),
        padding: UiRect::top(Val::Px(2.0)),
        min_width: Val::Px(0.0),
        ..default()
    })
    .with_children(|c| {
        c.spawn((text(label, LABEL, tokens::MUTED), TextLayout::no_wrap()));
        c.spawn(Node {
            max_width: Val::Px(240.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(text(body, Type::barlow(14.0, 400), tokens::SOFT))
                .insert(LineHeight::RelativeToFont(1.4));
        });
    });
}

/// The heatmap's note: how to pick a stage, and how many deaths it shows.
pub fn heat_note(stage_deaths: u32, room_deaths: Option<u32>) -> String {
    let n = |k: u32| {
        let unit = if k == 1 { "death" } else { "deaths" };
        format!("{} {unit}", card::grouped(u64::from(k)))
    };
    let count = match room_deaths {
        None if stage_deaths == 0 => "No deaths here.".to_owned(),
        None => format!("{}, none with a known place.", n(stage_deaths)),
        Some(r) if r == stage_deaths => format!("{}, all in this room.", n(stage_deaths)),
        Some(r) => format!(
            "{}, {} in this room.",
            n(stage_deaths),
            card::grouped(u64::from(r))
        ),
    };
    format!("Hover or click a stage in the splits. {count}")
}

/// Back; Retry (hold R); Watch replay (the replay viewer, #42), Save replay, Upload replay (M7).
fn footer(c: &mut ChildSpawner, s: &Summary, d: &Dims, saved: bool) {
    let style = Type::barlow(d.button_text, 600);
    let label = move |l: &str| (text(l, style, tokens::INK), TextLayout::no_wrap());
    let h = d.button;
    c.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: Val::Px(12.0),
        padding: UiRect::left(Val::Px(10.0)),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|c| {
        c.spawn((
            button_with(ButtonKind::Secondary, h, 22.0, label("Back")),
            Action::Back,
        ));
        c.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        let retry = (
            Node {
                align_items: AlignItems::Baseline,
                column_gap: Val::Px(10.0),
                ..default()
            },
            children![
                (
                    text("Retry", Type::barlow(d.button_text, 700), tokens::INK),
                    TextLayout::no_wrap()
                ),
                (
                    text("hold R", Type::barlow(12.0, 600), tokens::INK.with_alpha(0.8)),
                    TextLayout::no_wrap()
                ),
            ],
        );
        c.spawn((button_with(ButtonKind::Primary, h, 22.0, retry), Action::Retry));
        c.spawn(button_off(ButtonKind::Secondary, h, 20.0, label("Watch replay")));
        if s.replay.is_some() && !saved {
            c.spawn((
                button_with(ButtonKind::Secondary, h, 20.0, label("Save replay")),
                Action::SaveReplay,
            ));
        } else {
            let l = if saved { "Replay saved" } else { "Save replay" };
            c.spawn(button_off(ButtonKind::Secondary, h, 20.0, label(l)));
        }
        c.spawn(button_off(ButtonKind::Secondary, h, 20.0, label("Upload replay")));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_follows_the_window_ratio() {
        assert_eq!(Size::for_window(Vec2::new(1920.0, 1080.0)), Size::Wide);
        assert_eq!(Size::for_window(Vec2::new(1680.0, 1050.0)), Size::Wide);
        assert_eq!(Size::for_window(Vec2::new(1024.0, 768.0)), Size::Compact);
        assert_eq!(Size::Compact.design(), Vec2::new(1024.0, 768.0));
    }

    /// The page spawns with every part: the frozen board's `This run`, the hero time, the
    /// splits rows with a gold mark and the footer's actions.
    #[test]
    fn the_page_has_every_part() {
        let mut world = World::new();
        let s = super::super::sample::summary(super::super::Scenario::Pb, None);
        for size in [Size::Wide, Size::Compact] {
            let look = Look {
                size,
                fruit: None,
                saved: false,
                heatmap: true,
            };
            let root = world.spawn(page(&s, &look)).id();
            let mut texts = world.query::<&Text>();
            let all: Vec<String> = texts.iter(&world).map(|t| t.0.clone()).collect();
            for want in [
                "This run",
                "38:12",
                ".46",
                "NEW PB",
                "★ 3:25.84",
                "−1:27.66",
                "DEATHS PER STAGE",
                "1,284 total",
                "Boss",
                "Save replay",
                "DEATH TIMELINE",
                "0:00 → 38:12.46",
                "39:40.12 → 38:12.46",
            ] {
                assert!(all.iter().any(|t| t == want), "{want} missing from {all:?}");
            }
            let mut actions = world.query::<&Action>();
            assert_eq!(actions.iter(&world).count(), 3, "Back, Retry, Save replay");
            let mut rows = world.query::<&StageRow>();
            assert_eq!(rows.iter(&world).count(), 7, "a row per stage");
            let mut panels = world.query::<&HeatPanel>();
            let want = usize::from(size == Size::Wide);
            assert_eq!(
                panels.iter(&world).count(),
                want,
                "the heatmap only in the wide layout"
            );
            world.entity_mut(root).despawn();
        }
    }

    #[test]
    fn the_heatmap_note_counts_the_stage_and_the_room() {
        let lead = "Hover or click a stage in the splits.";
        assert_eq!(
            heat_note(288, Some(240)),
            format!("{lead} 288 deaths, 240 in this room.")
        );
        assert_eq!(
            heat_note(1, Some(1)),
            format!("{lead} 1 death, all in this room.")
        );
        assert_eq!(heat_note(0, None), format!("{lead} No deaths here."));
        assert_eq!(
            heat_note(1500, None),
            format!("{lead} 1,500 deaths, none with a known place.")
        );
    }
}
