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
/// shown and spins the list to it ([`Spin`]).
#[allow(clippy::too_many_arguments, reason = "one observer for the footer's buttons")]
pub(super) fn on_activate(
    activate: On<Activate>,
    mut commands: Commands,
    buttons: Query<&FooterButton>,
    shown: Option<Res<Shown>>,
    sel: Option<ResMut<Selection>>,
    next: Option<ResMut<NextState<AppScreen>>>,
    overlay: Option<ResMut<crate::settings_screen::Overlay>>,
    reduced: Option<Res<crate::ui::motion::ReducedMotion>>,
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
            let Some(game) = random_other(&order, sel.game, seed) else {
                return;
            };
            let path = spin_path(&order, game);
            if reduced.is_some_and(|r| r.0) || path.len() < 2 {
                *sel = Selection { game, map: 0 };
                return;
            }
            commands.insert_resource(Spin { path, seconds: 0.0 });
        }
    }
}

/// How long Random's spin takes: under `tokens::motion::SCREEN` (0.3 s).
pub const SPIN_SECONDS: f32 = 0.28;
const _: () = assert!(SPIN_SECONDS < crate::ui::tokens::motion::SCREEN);
/// The most games the spin walks over, the pick included.
const SPIN_STEPS: usize = 6;

/// Random's spin (#1698): the selection walks over the games before the pick in the list's order
/// and lands on it, each step longer than the one before. Any key or click lands at once.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Spin {
    /// The games it walks over, the pick last.
    pub path: Vec<usize>,
    /// Time spun so far.
    pub seconds: f32,
}

impl Spin {
    /// The step shown after `seconds`: the first at once, then step `i` of `n` (from 1) at
    /// `SPIN_SECONDS * (i/n)^2`, so the steps slow down toward the pick.
    pub fn step(&self, seconds: f32) -> usize {
        let n = self.path.len();
        let done = ((seconds / SPIN_SECONDS).max(0.0).sqrt() * n as f32).floor() as usize;
        done.saturating_sub(1).min(n - 1)
    }
}

/// The games the spin walks over to land on `pick`: the ones before it in `order`, wrapping, at
/// most [`SPIN_STEPS`] with the pick.
pub fn spin_path(order: &[usize], pick: usize) -> Vec<usize> {
    let Some(at) = order.iter().position(|g| *g == pick) else {
        return vec![pick];
    };
    let n = SPIN_STEPS.min(order.len());
    (0..n).map(|j| order[(at + order.len() + 1 + j - n) % order.len()]).collect()
}

/// Walks the spin; a key, a mouse button or the end lands it on the pick.
pub(super) fn spin(
    mut commands: Commands,
    time: Res<Time>,
    spin: Option<ResMut<Spin>>,
    sel: Option<ResMut<Selection>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
) {
    let (Some(mut spin), Some(mut sel)) = (spin, sel) else {
        return;
    };
    // Not on its first frame: the key that chose Random may still read as just pressed.
    let first = spin.seconds == 0.0;
    let pressed = !first
        && (keys.is_some_and(|k| k.get_just_pressed().next().is_some())
            || mouse.is_some_and(|m| m.get_just_pressed().next().is_some()));
    spin.seconds += time.delta_secs();
    let last = spin.path.len() - 1;
    let at = if pressed || spin.seconds >= SPIN_SECONDS {
        last
    } else {
        spin.step(spin.seconds)
    };
    let game = spin.path[at];
    if sel.game != game || sel.map != 0 {
        *sel = Selection { game, map: 0 };
    }
    if at == last {
        commands.remove_resource::<Spin>();
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

    /// The spin walks the games before the pick in the list's order, wrapping, and ends on it.
    #[test]
    fn the_spin_walks_to_the_pick_and_slows_down() {
        let order = [7, 3, 9, 1, 4, 8, 2];
        assert_eq!(spin_path(&order, 4), vec![2, 7, 3, 9, 1, 4]);
        assert_eq!(spin_path(&order, 7), vec![3, 9, 1, 4, 8, 2, 7][1..].to_vec());
        assert_eq!(spin_path(&[5, 6], 6), vec![5, 6]);
        assert_eq!(spin_path(&[5, 6], 0), vec![0], "not shown: no walk");
        let spin = Spin {
            path: spin_path(&order, 4),
            seconds: 0.0,
        };
        assert_eq!(spin.step(0.0), 0, "something moves at once");
        assert_eq!(spin.step(SPIN_SECONDS), 5, "the end is the pick");
        // Each step lasts longer than the one before.
        let at = |i: f32| SPIN_SECONDS * (i / 6.0).powi(2);
        let gaps: Vec<f32> = (1..6).map(|i| at(i as f32 + 1.0) - at(i as f32)).collect();
        assert!(gaps.windows(2).all(|w| w[1] > w[0]), "{gaps:?}");
    }

    /// The spin system walks the selection and lands on the pick by the end; a key lands it at
    /// once.
    #[test]
    fn the_spin_lands_on_the_pick_or_at_a_key() {
        use bevy::ecs::system::RunSystemOnce;
        let run = |press: bool, frames: usize| {
            let mut world = World::new();
            world.insert_resource(Time::<()>::default());
            world.insert_resource(Selection { game: 3, map: 1 });
            world.init_resource::<ButtonInput<KeyCode>>();
            world.insert_resource(Spin {
                path: vec![5, 6, 7, 8],
                seconds: 0.0,
            });
            let mut seen = vec![];
            for f in 0..frames {
                world.resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
                if press && f == 1 {
                    world.resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ArrowDown);
                }
                world.run_system_once(spin).unwrap();
                seen.push(world.resource::<Selection>().game);
            }
            (seen, world.contains_resource::<Spin>(), *world.resource::<Selection>())
        };
        let (seen, spinning, sel) = run(false, 30);
        assert_eq!(seen[0], 5, "the first step at once");
        assert!(seen.windows(2).all(|w| w[1] >= w[0]), "it walks forward: {seen:?}");
        assert_eq!(sel, Selection { game: 8, map: 0 });
        assert!(!spinning, "done");
        let (seen, spinning, _) = run(true, 3);
        assert_eq!(seen[1], 8, "a key lands at once");
        assert!(!spinning);
    }
}
