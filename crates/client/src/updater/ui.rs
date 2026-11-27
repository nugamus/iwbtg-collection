//! The updater's dialog (#1606): the m5 B10 "update ready" dialog over the main menu. The
//! Settings → Updates rows live in the settings overlay (`settings_screen.rs`, #1675), the ready
//! notification in the bell's list, and the update button in the toolbar.
//!
//! Both are built from the #490 kit (tokens, `Type`, `Shape`, buttons), never one-off styles. The
//! dialog is upright, not sheared, because it holds paragraphs (`MenuDialog.dc.html`); it names
//! the build's version and lists the PR titles `build.json` carries under "What's new".

use bevy::prelude::*;
use bevy::text::LineBreak;
use bevy::ui_widgets::Activate;

use crate::ui::fonts::Type;
use crate::ui::icons::{Icon, icon};
use crate::ui::shape::Shape;
use crate::ui::tokens;
use crate::ui::widgets::{self, ButtonKind, text};

use super::{Dialog, Phase, ReadyUpdate, Updater, short};

/// The dialog's root (the dim and the card).
#[derive(Component)]
pub struct UpdateDialog;

/// What a dialog or panel button does.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Restart,
    Later,
    WhatsNew,
}

/// The card's fill and shadow (`MenuDialog.dc.html`: `#12141d`, `0 30px 80px rgba(0,0,0,.6)`).
fn card_fill() -> Color {
    Color::srgb_u8(0x12, 0x14, 0x1d)
}

/// The dim behind a dialog (the mockup's `rgba(5,6,10,.62)`).
fn dim() -> Color {
    Color::srgba(5.0 / 255.0, 6.0 / 255.0, 10.0 / 255.0, 0.62)
}

/// The dialog's or panel's full-window root: the dim, centred on the child card.
fn overlay(commands: &mut Commands, marker: impl Component) -> Entity {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(dim()),
            // Over the menu (20) and notices (30); a dialog is the one thing the player answers.
            GlobalZIndex(40),
            marker,
        ))
        .id()
}

/// An upright card: a 4 px accent line, then padding and a column of content (no shear, radius
/// 10). Returns the card and the content node the caller fills.
fn card(commands: &mut Commands, width: f32) -> (Entity, Entity) {
    let mut content = Entity::PLACEHOLDER;
    let card = commands
        .spawn((
            Node {
                width: Val::Px(width),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            Shape {
                shear: 0.0,
                radius: 10.0,
                ..Shape::fill(card_fill()).with_shadow(30.0, 80.0, tokens::black(0.6))
            },
            Pickable::IGNORE,
        ))
        .with_children(|card| {
            card.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(4.0),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(tokens::ACCENT),
            ));
            content = card
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(16.0),
                        padding: UiRect {
                            left: Val::Px(38.0),
                            right: Val::Px(38.0),
                            top: Val::Px(28.0),
                            bottom: Val::Px(30.0),
                        },
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .id();
        })
        .id();
    (card, content)
}

/// Spawns the B10 dialog for a verified build. `expanded` shows every "What's new" title the
/// manifest carries (up to a legible cap); `preview` is a screenshot check that must not install.
pub(super) fn spawn_dialog(
    commands: &mut Commands,
    ready: &ReadyUpdate,
    expanded: bool,
    preview: bool,
) {
    let root = overlay(commands, UpdateDialog);
    let (card, body) = card(commands, 700.0);
    commands.entity(root).add_child(card);

    let title = format!("Version {} is ready", ready.build.version);
    let shown: Vec<String> = ready
        .build
        .prs
        .iter()
        .take(if expanded { 8 } else { 3 })
        .cloned()
        .collect();
    let count = ready.build.prs.len();
    let build = format!(
        "Built {} from {}.",
        ready.build.date,
        short(&ready.build.commit)
    );

    commands.entity(body).with_children(|body| {
        body.spawn(text(&title, Type::barlow(30.0, 700), tokens::INK));
        body.spawn((
            text(
                "It downloaded in the background and its checksum matches. Restart now, or keep playing and update later.",
                Type::barlow(17.0, 400),
                tokens::SOFT,
            ),
            TextLayout::linebreak(LineBreak::WordOrCharacter),
        ));
        for line in &shown {
            body.spawn((
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(12.0),
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(tokens::white(0.06)),
                Pickable::IGNORE,
                children![
                    icon(Icon::Check, 20.0, tokens::AHEAD),
                    text(line, Type::barlow(16.0, 700), tokens::INK),
                ],
            ));
        }
        if count > shown.len() {
            body.spawn(text(
                &format!("{} more under What's new.", count - shown.len()),
                Type::barlow(14.0, 400),
                tokens::MUTED,
            ));
        }
        body.spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                flex_wrap: FlexWrap::Wrap,
                margin: UiRect::top(Val::Px(8.0)),
                ..default()
            },
            Pickable::IGNORE,
            children![
                (
                    widgets::button("Restart and update", ButtonKind::Primary),
                    Action::Restart,
                ),
                (widgets::button("Later", ButtonKind::Secondary), Action::Later),
                (
                    widgets::button("What's new", ButtonKind::Switch),
                    Action::WhatsNew,
                ),
            ],
        ));
        body.spawn((
            text(&build, Type::barlow(14.0, 400), tokens::MUTED),
            TextLayout::linebreak(LineBreak::WordOrCharacter),
        ));
        if preview {
            body.spawn(text(
                "Preview: this screenshot check installs nothing.",
                Type::barlow(13.0, 600),
                tokens::AMBER,
            ));
        }
    });
}

/// A click (or Enter/Space) on any updater button.
pub(super) fn on_activate(
    activate: On<Activate>,
    action: Query<&Action>,
    mut updater: ResMut<Updater>,
    mut dialog: ResMut<Dialog>,
    mut commands: Commands,
    shown: Query<Entity, With<UpdateDialog>>,
    toolbar: Query<&crate::ui::toolbar::ToolbarButton>,
) {
    // The toolbar's update button shows the dialog again.
    if toolbar.get(activate.entity) == Ok(&crate::ui::toolbar::ToolbarButton::Update) {
        dialog.dismissed = false;
        return;
    }
    let Ok(action) = action.get(activate.entity) else {
        return;
    };
    match action {
        Action::Restart => {
            let exe = match &updater.phase {
                Phase::Ready(ready) => ready.exe.clone(),
                _ => return,
            };
            if updater.preview {
                return;
            }
            if let Err(message) = super::install::install_and_restart(&exe) {
                // The old exe is still in place; say so and take the dialog away.
                crate::ui::notice::post(crate::ui::notice::Kind::Update, &message);
                updater.last = Some(message.clone());
                updater.phase = Phase::Failed(message);
                for entity in &shown {
                    commands.entity(entity).despawn();
                }
            } else {
                // Two clients must never share one settings file: this one ends here.
                std::process::exit(0);
            }
        }
        Action::Later => {
            dialog.dismissed = true;
            for entity in &shown {
                commands.entity(entity).despawn();
            }
        }
        Action::WhatsNew => {
            // The prompt respawns next frame with the fuller list.
            dialog.expanded = !dialog.expanded;
            for entity in &shown {
                commands.entity(entity).despawn();
            }
        }
    }
}
