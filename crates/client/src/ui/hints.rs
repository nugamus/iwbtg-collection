//! The hint bar (m5 `Hints.dc.html`, #117): what each key or button does on this screen, as a row
//! of glyphs and labels. Controller faces (A, B, X, Y) are round light discs, everything else
//! (keys, shoulder buttons) a keycap; the glyphs are neutral, never coloured by brand.
//!
//! A screen spawns [`hints`] with the items for the device in use. Until the client tracks the last
//! device used (#117), screens pass keyboard keys.

use bevy::prelude::*;

use super::fonts::Type;
use super::tokens;
use super::widgets::text;

/// The hint bar's height.
pub const HEIGHT: f32 = 40.0;
/// A glyph: 26 px, `rgba(255,255,255,.88)` with dark text.
const GLYPH: f32 = 26.0;
const LABEL: Type = Type::barlow(15.0, 600);
const GLYPH_TYPE: Type = Type::barlow(13.0, 700);
/// `#e3e5ee`.
const LABEL_INK: Color = Color::srgb_u8(0xe3, 0xe5, 0xee);

/// One hint: the glyph's text (`A`, `Enter`, `LB/RB`) and what it does.
pub type Hint<'a> = (&'a str, &'a str);

/// Whether `glyph` is a controller face button, drawn round.
pub fn is_face(glyph: &str) -> bool {
    matches!(glyph, "A" | "B" | "X" | "Y")
}

/// The hint bar for `items`, left to right, 22 px apart.
pub fn hints(items: &[Hint]) -> impl Bundle + use<> {
    let items: Vec<(String, String)> = items
        .iter()
        .map(|(g, l)| ((*g).to_owned(), (*l).to_owned()))
        .collect();
    (
        Node {
            height: Val::Px(HEIGHT),
            align_items: AlignItems::Center,
            column_gap: Val::Px(22.0),
            padding: UiRect::horizontal(Val::Px(4.0)),
            ..default()
        },
        Children::spawn(SpawnIter(items.into_iter().map(|(g, l)| hint(&g, &l)))),
    )
}

fn hint(glyph: &str, label: &str) -> impl Bundle + use<> {
    let round = is_face(glyph);
    (
        Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        },
        children![
            (
                Node {
                    min_width: Val::Px(GLYPH),
                    height: Val::Px(GLYPH),
                    padding: UiRect::horizontal(Val::Px(if round { 0.0 } else { 7.0 })),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(Val::Px(if round {
                        GLYPH / 2.0
                    } else {
                        6.0
                    })),
                    border: UiRect::bottom(Val::Px(if round { 0.0 } else { 3.0 })),
                    ..default()
                },
                BackgroundColor(tokens::white(0.88)),
                // The keycap's lip: `inset 0 -3px 0 rgba(0,0,0,.25)`.
                BorderColor::all(tokens::black(0.25)),
                children![text(glyph, GLYPH_TYPE, tokens::ON_LIGHT)],
            ),
            text(label, LABEL, LABEL_INK),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_are_round_and_keys_are_caps() {
        assert!(is_face("A") && is_face("Y"));
        assert!(!is_face("Enter") && !is_face("LB/RB") && !is_face("Esc"));
    }

    #[test]
    fn the_bar_holds_one_hint_per_item() {
        let mut world = World::new();
        let bar = world
            .spawn(hints(&[("Enter", "Play"), ("Esc", "Back")]))
            .id();
        assert_eq!(world.entity(bar).get::<Children>().map(|c| c.len()), Some(2));
    }
}
