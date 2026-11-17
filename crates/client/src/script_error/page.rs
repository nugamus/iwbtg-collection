//! The script error card, in the loading card's look (`loading/page.rs`; no artboard draws it,
//! #101): the key art strip with the title and creator, then the accent `Script error` pill and
//! where (file and line), what went wrong, where in the game, and the buttons. Sizes are CSS px
//! at a 1080 px tall page; `script_error::show` scales the page to the window.

use bevy::prelude::*;

use super::{Action, Report};
use crate::loading::page::{CARD_TOP, CARD_W, glass, strip};
use crate::ui::fonts::Type;
use crate::ui::shape::Shape;
use crate::ui::tokens;
use crate::ui::widgets::{ButtonKind, button, pill, rich, text};

/// What the page draws with besides the report.
pub struct Look {
    pub title: String,
    pub creator: Option<String>,
    /// The key art (the backdrop's).
    pub art: Option<Handle<Image>>,
    /// Back goes to game select (`Back to game select`) rather than quitting (`Quit`).
    pub back: bool,
}

/// The message text, so tests can find it.
#[derive(Component)]
pub struct Message;

/// The whole page, filling its parent: the card centred, `CARD_TOP` from the top.
pub fn page(report: &Report, look: &Look) -> impl Bundle + use<> {
    let report = report.clone();
    let title = look.title.clone();
    let creator = look.creator.clone();
    let art = look.art.clone();
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
                    strip(c, &title, creator.as_deref(), art);
                    body(c, &report);
                });
                footer(c, back);
            });
        })),
    )
}

/// The pill and the place, the message, the context and what happened to the run.
fn body(c: &mut ChildSpawner, report: &Report) {
    c.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(16.0),
        padding: UiRect {
            left: Val::Px(32.0),
            right: Val::Px(32.0),
            top: Val::Px(22.0),
            bottom: Val::Px(28.0),
        },
        ..default()
    })
    .with_children(|c| {
        c.spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(12.0),
            min_height: Val::Px(26.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(pill(
                report.heading,
                Type::PILL_CAPS,
                tokens::ACCENT,
                tokens::INK,
                26.0,
                12.0,
            ));
            if let Some(place) = report.place() {
                c.spawn((
                    text(&place, Type::barlow(18.0, 600), tokens::INK),
                    TextLayout::no_wrap(),
                ));
            }
        });
        c.spawn((text(&report.message, Type::barlow(24.0, 500), tokens::INK), Message));
        if let Some(context) = &report.context {
            c.spawn(rich(vec![
                ("In ".to_owned(), Type::barlow(15.0, 400), tokens::MUTED),
                (context.clone(), Type::barlow(15.0, 600), tokens::SOFT),
            ]));
        }
        c.spawn(text(
            "The game stopped here; nothing else is affected. The package's author can fix it \
             at the place above.",
            Type::barlow(15.0, 400),
            tokens::MUTED,
        ));
    });
}

/// `Esc back` on the left; Retry and the primary Back (or Quit) on the right.
fn footer(c: &mut ChildSpawner, back: bool) {
    c.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: Val::Px(12.0),
        padding: UiRect::horizontal(Val::Px(4.0)),
        ..default()
    })
    .with_children(|c| {
        let plain = Type::barlow(15.0, 400);
        let bold = Type::barlow(15.0, 700);
        c.spawn(rich(vec![
            ("Esc".to_owned(), bold, tokens::INK),
            (
                if back { " back" } else { " quit" }.to_owned(),
                plain,
                tokens::SOFT,
            ),
        ]));
        c.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        c.spawn((button("Retry", ButtonKind::Secondary), Action::Retry));
        let label = if back { "Back to game select" } else { "Quit" };
        c.spawn((button(label, ButtonKind::Primary), Action::Back));
    });
}
