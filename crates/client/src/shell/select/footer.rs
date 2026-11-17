//! Game select's footer (`Main.dc.html`): for now only the Mods button, which opens the mod
//! select sheet (#497, `mod_sheet.rs`). Back, Options, Random and the player card are #148.

use bevy::prelude::*;
use iwbtg_sim::mods::ModSet;

use super::AppScreen;
use super::mod_sheet::{ModsValue, SheetButton, button_value};
use crate::ui::fonts::Type;
use crate::ui::tokens;
use crate::ui::widgets::{ButtonKind, button_with, text};

/// Game select's Mods button (`Main.dc.html`'s footer, left of the player card): `Mods` and the
/// picked codes. The rest of the footer is #148.
pub(super) fn spawn(commands: &mut Commands, camera: Entity, set: ModSet) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            // The player card (420 px) and its 18 px margin sit right of it, 64 px in.
            right: Val::Px(64.0 + 420.0 + 18.0),
            bottom: Val::Px(26.0),
            ..default()
        },
        UiTargetCamera(camera),
        DespawnOnExit(AppScreen::Select),
        children![(
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
        )],
    ));
}
