//! Game select's footer (#148; m5 `A3_SelectTimeAttack`): Back and Options on the left; Random,
//! Practice and Mods (the accent button, with the picked codes) on the right, then the compact
//! player card in the corner.

use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use iwbtg_sim::mods::ModSet;

use super::AppScreen;
use super::list::{Selection, Shown};
use super::mod_sheet::{ModsValue, SheetButton, button_value};
use crate::card::{COLLECTION_GAMES, Card, Variant, card, profile_name};
use crate::ui::fonts::Type;
use crate::ui::icons::{Icon, icon};
use crate::ui::tokens;
use crate::ui::widgets::{ButtonKind, button_with, text};

/// What a footer button does (Practice and Mods have their own markers).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FooterButton {
    Back,
    Options,
    Random,
}

/// The footer along the bottom, 64 px in from each side.
pub(super) fn spawn(commands: &mut Commands, camera: Entity, set: ModSet, player: Card) {
    let secondary = |label: &'static str, icon_kind: Option<Icon>| {
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
                Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
                    if let Some(k) = icon_kind {
                        c.spawn(icon(k, 14.0, tokens::INK));
                    }
                    c.spawn(text(label, Type::BUTTON, tokens::INK));
                })),
            ),
        )
    };
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(64.0),
            right: Val::Px(64.0),
            bottom: Val::Px(26.0),
            align_items: AlignItems::FlexEnd,
            column_gap: Val::Px(12.0),
            ..default()
        },
        Pickable::IGNORE,
        UiTargetCamera(camera),
        DespawnOnExit(AppScreen::Select),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn((secondary("Back", Some(Icon::Back)), FooterButton::Back));
            c.spawn((secondary("Options", None), FooterButton::Options));
            c.spawn((
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
                Pickable::IGNORE,
            ));
            c.spawn((secondary("Random", Some(Icon::Random)), FooterButton::Random));
            c.spawn((secondary("Practice", None), super::practice::OpenButton));
            c.spawn((
                button_with(
                    ButtonKind::Primary,
                    52.0,
                    28.0,
                    (
                        Node {
                            align_items: AlignItems::Baseline,
                            column_gap: Val::Px(10.0),
                            ..default()
                        },
                        children![
                            text("Mods", Type::PRIMARY, tokens::INK),
                            (
                                text(
                                    &button_value(set),
                                    Type::barlow(13.0, 600),
                                    tokens::INK.with_alpha(0.85)
                                ),
                                ModsValue
                            ),
                        ],
                    ),
                ),
                SheetButton::Mods,
            ));
            c.spawn((
                Node {
                    margin: UiRect::left(Val::Px(6.0)),
                    ..default()
                },
                children![card(&player, Variant::Footer)],
            ));
        })),
    ));
}

/// The keys hint over the footer's left half; while a staged start prepares it says `Opening
/// <name>…` (the select screen's `HintText`).
pub(super) fn hint(commands: &mut Commands, camera: Entity, hint: &str) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(64.0),
            right: Val::Px(super::list::FROM_RIGHT + 60.0),
            bottom: Val::Px(96.0),
            ..default()
        },
        Pickable::IGNORE,
        UiTargetCamera(camera),
        DespawnOnExit(AppScreen::Select),
        children![(text(hint, Type::barlow(14.0, 500), tokens::MUTED), super::super::select::HintText)],
    ));
}

/// The player card for the footer, from the stats book.
pub(super) fn player(stats: Option<&crate::stats::Stats>) -> Card {
    let empty = crate::stats::Book::default();
    let book = stats.map_or(&empty, |s| &s.book);
    Card::from_book(book, &profile_name(), COLLECTION_GAMES)
}

/// Back goes to the main menu, Options opens the settings, Random picks a game among those
/// shown.
pub(super) fn on_activate(
    activate: On<Activate>,
    buttons: Query<&FooterButton>,
    shown: Option<Res<Shown>>,
    sel: Option<ResMut<Selection>>,
    next: Option<ResMut<NextState<AppScreen>>>,
    overlay: Option<ResMut<crate::settings_screen::Overlay>>,
) {
    let Ok(button) = buttons.get(activate.entity) else {
        return;
    };
    match button {
        FooterButton::Back => {
            if let Some(mut next) = next {
                next.set(AppScreen::Menu);
            }
        }
        FooterButton::Options => {
            if let Some(mut o) = overlay {
                o.show();
            }
        }
        FooterButton::Random => {
            let (Some(shown), Some(mut sel)) = (shown, sel) else {
                return;
            };
            let order = shown.order();
            let seed = crate::time::SystemTime::now()
                .duration_since(crate::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos() as usize);
            if let Some(game) = random_other(&order, sel.game, seed) {
                *sel = Selection { game, map: 0 };
            }
        }
    }
}

/// A game of `order` other than `current` (when there is another), picked by `seed`.
fn random_other(order: &[usize], current: usize, seed: usize) -> Option<usize> {
    let others: Vec<usize> = order.iter().copied().filter(|g| *g != current).collect();
    if others.is_empty() {
        return order.first().copied();
    }
    Some(others[seed % others.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_picks_another_game() {
        for seed in 0..10 {
            let g = random_other(&[0, 1, 2], 1, seed).unwrap();
            assert_ne!(g, 1);
        }
        assert_eq!(random_other(&[4], 4, 3), Some(4), "the only game stays");
        assert_eq!(random_other(&[], 0, 3), None);
    }
}
