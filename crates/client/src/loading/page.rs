//! The loading card from its artboards (`Loading.dc.html`, `LoadingUnranked.dc.html`, at
//! 1920×1080): the card (key art strip with the title and creator, the map, your PB and the
//! stage count, the mods), then the delicious fruit beside the progress line, then a tip. Sizes
//! are the artboard's CSS px; `loading::show` scales the page to the window.

use bevy::prelude::*;

use super::{Info, Standing, Tip};
use crate::pause::FRUIT;
use crate::ui::fonts::Type;
use crate::ui::shape::{Shape, Veil};
use crate::ui::tokens;
use crate::ui::widgets::{mod_pill, rich, text, unranked_pill};

/// The card's left and top on the artboard, and its width.
pub const CARD_TOP: f32 = 170.0;
pub const CARD_W: f32 = 760.0;
/// The key art strip's height.
const STRIP_H: f32 = 240.0;
/// The fruit is drawn at twice its size (`84×48` for both frames), one frame showing.
const FRUIT_SCALE: f32 = 2.0;

/// The card's glass: `rgba(12,14,22,.82)`.
pub(crate) fn glass() -> Color {
    tokens::dark(Color::srgb_u8(12, 14, 22), 0.82)
}

/// The delicious fruit; `loading::animate` steps its frame.
#[derive(Component)]
pub struct FruitFrame;

/// The progress line's fill; `loading::animate` sets its width.
#[derive(Component)]
pub struct Progress;

/// What the page draws with: the key art (the backdrop's) and the fruit (the pause menu's).
pub struct Look {
    pub art: Option<Handle<Image>>,
    pub fruit: Option<Handle<Image>>,
    /// Escape goes back to game select (`Esc back`) rather than quitting (`Esc quit`).
    pub back: bool,
}

/// The whole page, filling its parent: the card centred, `CARD_TOP` from the top.
pub fn page(info: &Info, tip: &Tip, look: &Look) -> impl Bundle + use<> {
    let info = info.clone();
    let tip = tip.clone();
    let art = look.art.clone();
    let fruit = look.fruit.clone();
    let back = look.back;
    (
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            padding: UiRect::top(Val::Px(CARD_TOP)),
            ..default()
        },
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn(Node {
                width: Val::Px(CARD_W),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(22.0),
                ..default()
            })
            .with_children(|c| {
                card(c, &info, art);
                loader(c, fruit, back);
                c.spawn((
                    Node {
                        padding: UiRect::horizontal(Val::Px(4.0)),
                        ..default()
                    },
                    children![tip_line(&tip)],
                ));
            });
        })),
    )
}

/// The card: no shear, the kit's radius, a soft shadow.
fn card(c: &mut ChildSpawner, info: &Info, art: Option<Handle<Image>>) {
    let shape = Shape {
        shear: 0.0,
        ..Shape::fill(glass())
    }
    .with_shadow(30.0, 80.0, tokens::black(0.5));
    c.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            ..default()
        },
        shape,
    ))
    .with_children(|c| {
        strip(c, &info.title, info.creator.as_deref(), art);
        body(c, info);
    });
}

/// The key art, 240 px tall, darkened towards the bottom, with the title and creator over it
/// (also the script error card's head).
pub(crate) fn strip(c: &mut ChildSpawner, title: &str, creator: Option<&str>, art: Option<Handle<Image>>) {
    // `Backdrop dim 0.1`, then `linear-gradient(0deg, rgba(12,14,22,.95) 0%, transparent 60%)`.
    let dim = tokens::black(0.1);
    let mut shape = Shape {
        shear: 0.0,
        ..Shape::fill(tokens::GROUND)
    }
    .with_veil(Veil {
        stops: [dim, dim, tokens::dark(Color::srgb_u8(12, 14, 22), 0.955)],
        mid: 0.4,
        vertical: true,
    });
    if let Some(art) = art {
        shape = shape.with_image(art);
    }
    let title = title.to_owned();
    let creator = creator.map(str::to_owned);
    c.spawn((
        Node {
            height: Val::Px(STRIP_H),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexEnd,
            row_gap: Val::Px(4.0),
            padding: UiRect {
                left: Val::Px(32.0),
                right: Val::Px(32.0),
                bottom: Val::Px(18.0),
                top: Val::Px(0.0),
            },
            ..default()
        },
        shape,
    ))
    .with_children(|c| {
        c.spawn((
            text(&title, Type::condensed(50.0, 700), tokens::INK),
            TextLayout::no_wrap(),
        ))
        .insert(bevy::text::LineHeight::RelativeToFont(1.0));
        if let Some(name) = creator {
            c.spawn(rich(vec![
                ("by ".to_owned(), Type::barlow(17.0, 400), tokens::SOFT),
                (name, Type::barlow(17.0, 600), tokens::INK),
            ]));
        }
    });
}

/// The map, the numbers and the mods.
fn body(c: &mut ChildSpawner, info: &Info) {
    c.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(22.0),
        padding: UiRect {
            left: Val::Px(32.0),
            right: Val::Px(32.0),
            top: Val::Px(22.0),
            bottom: Val::Px(28.0),
        },
        ..default()
    })
    .with_children(|c| {
        // The star pill waits for difficulty ratings; the map's name stands alone.
        c.spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            min_height: Val::Px(26.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn((
                text(&info.map, Type::barlow(18.0, 600), tokens::INK),
                TextLayout::no_wrap(),
            ));
        });
        numbers(c, info);
        mods(c, info);
    });
}

/// The three-column grid: your PB and the stages. Global #1 waits for online play; its column
/// stays empty so the others keep their place.
fn numbers(c: &mut ChildSpawner, info: &Info) {
    let pb = info.pb.clone().unwrap_or_else(|| "—".to_owned());
    let pb_note = info.pb.is_none().then(|| "No clear yet".to_owned());
    let stages = info.stages.to_string();
    c.spawn(Node {
        display: Display::Grid,
        grid_template_columns: RepeatedGridTrack::flex(3, 1.0),
        column_gap: Val::Px(24.0),
        ..default()
    })
    .with_children(|c| {
        number(c, "Your PB", &pb, pb_note);
        number(c, "Stages", &stages, None);
    });
}

/// A label over a value and a note under it (an empty line without one).
fn number(c: &mut ChildSpawner, label: &str, value: &str, note: Option<String>) {
    c.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(2.0),
        min_width: Val::Px(0.0),
        ..default()
    })
    .with_children(|c| {
        c.spawn((text(label, Type::LABEL, tokens::MUTED), TextLayout::no_wrap()));
        c.spawn((
            text(value, Type::barlow(26.0, 600), tokens::INK),
            TextLayout::no_wrap(),
        ));
        // The note's line stays when empty, so the card keeps the artboard's height.
        c.spawn((
            text(
                note.as_deref().unwrap_or(" "),
                Type::barlow(14.0, 400),
                tokens::MUTED,
            ),
            TextLayout::no_wrap(),
        ));
    });
}

/// `MODS  [UNRANKED] [SL]  Slow motion 75% is a practice assist, so ...` or `MODS  [SD]  Sudden
/// Death · ranked`.
fn mods(c: &mut ChildSpawner, info: &Info) {
    let codes = info.mods.clone();
    let (unranked, line) = match &info.standing {
        Standing::Ranked(line) => (false, line.clone()),
        Standing::Unranked(reason) => (true, reason.clone()),
    };
    c.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: Val::Px(10.0),
        min_height: Val::Px(28.0),
        ..default()
    })
    .with_children(|c| {
        c.spawn((
            Node {
                width: Val::Px(56.0),
                flex_shrink: 0.0,
                ..default()
            },
            children![text("Mods", Type::LABEL, tokens::MUTED)],
        ));
        if unranked {
            c.spawn(unranked_pill());
        }
        for code in &codes {
            c.spawn(mod_pill(code));
        }
        c.spawn((
            Node {
                flex_shrink: 1.0,
                min_width: Val::Px(0.0),
                ..default()
            },
            children![text(&line, Type::barlow(15.0, 400), tokens::ROW_NAME)],
        ));
    });
}

/// The fruit, then `Loading · starts by itself` and `Esc back` or `Esc quit` over the progress line.
fn loader(c: &mut ChildSpawner, fruit: Option<Handle<Image>>, back: bool) {
    c.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: Val::Px(16.0),
        padding: UiRect::horizontal(Val::Px(4.0)),
        ..default()
    })
    .with_children(|c| {
        let mut slot = c.spawn(Node {
            width: Val::Px(FRUIT.x as f32 * FRUIT_SCALE),
            height: Val::Px(FRUIT.y as f32 * FRUIT_SCALE),
            flex_shrink: 0.0,
            ..default()
        });
        if let Some(image) = fruit {
            slot.insert((
                ImageNode {
                    image,
                    rect: Some(Rect::new(0.0, 0.0, FRUIT.x as f32, FRUIT.y as f32)),
                    ..default()
                },
                FruitFrame,
            ));
        }
        c.spawn(Node {
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|c| {
            let plain = Type::barlow(15.0, 400);
            let bold = Type::barlow(15.0, 700);
            c.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Baseline,
                ..default()
            })
            .with_children(|c| {
                c.spawn(rich(vec![
                    ("Loading".to_owned(), bold, tokens::INK),
                    (" · starts by itself".to_owned(), plain, tokens::SOFT),
                ]));
                c.spawn(rich(vec![
                    ("Esc".to_owned(), bold, tokens::INK),
                    (
                        if back { " back" } else { " quit" }.to_owned(),
                        plain,
                        tokens::SOFT,
                    ),
                ]));
            });
            c.spawn((
                Node {
                    height: Val::Px(3.0),
                    overflow: Overflow::clip(),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(tokens::white(0.14)),
            ))
            .with_children(|c| {
                c.spawn((
                    Node {
                        width: Val::Percent(0.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    Shape {
                        shear: 0.0,
                        radius: 0.0,
                        ..Shape::fill(Color::NONE)
                    }
                    .with_veil(Veil::across(
                        tokens::ACCENT,
                        tokens::ACCENT.mix(&Color::WHITE, 0.5),
                        Color::WHITE,
                        0.5,
                    )),
                    Progress,
                ));
            });
        });
    });
}

/// `Tip: hold R on the results screen to retry straight away.`, the keys in bold.
fn tip_line(tip: &Tip) -> impl Bundle + use<> {
    let mut parts = vec![("Tip: ".to_owned(), Type::barlow(15.0, 400), tokens::MUTED)];
    for (words, key) in tip.0 {
        let (style, color) = if *key {
            (Type::barlow(15.0, 700), tokens::ROW_NAME)
        } else {
            (Type::barlow(15.0, 400), tokens::MUTED)
        };
        parts.push(((*words).to_owned(), style, color));
    }
    rich(parts)
}
