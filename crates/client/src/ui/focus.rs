//! Focus that never moves the layout (UI foundation, #1681).
//!
//! A screen marks the part the keyboard is on by setting [`Focused`]; it does not rebuild
//! anything. What focus looks like is declared once on the part when it is spawned:
//! [`FocusFill`] swaps its background, [`FocusRing`] draws a ring and [`FocusInk`] tints its icon
//! or text. The ring is an [`Outline`] on a plain node or the [`Shape`] ring on a sheared one:
//! both draw outside the layout box, so focusing a part never changes its size or moves its
//! neighbours (a border would: it takes room inside the box).
//!
//! Every change happens in one system, on `Changed<Focused>`: a focus move costs a few
//! component writes, not a respawn.

use bevy::prelude::*;

use crate::ui::shape::{Ink, Shape};

/// Whether the keyboard is on this part. Screens write it; only change it when it differs, so
/// `Changed` stays meaningful ([`set`]).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Focused(pub bool);

/// The background at rest and with focus.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FocusFill {
    pub rest: Color,
    pub focus: Color,
}

/// A ring outside the box while focused.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FocusRing {
    pub width: f32,
    pub color: Color,
}

/// An icon's or a text's colour at rest and with focus.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FocusInk {
    pub rest: Color,
    pub focus: Color,
}

/// A plain node's focus ring: the ring and the hidden outline it shows (keep the outline on the
/// node: inserting and removing one moves the entity between tables).
pub fn ring(width: f32, color: Color) -> (Focused, FocusRing, Outline) {
    (
        Focused(false),
        FocusRing { width, color },
        Outline::new(Val::Px(width), Val::ZERO, Color::NONE),
    )
}

/// A sheared node's focus ring (its [`Shape`] draws it).
pub fn shape_ring(width: f32, color: Color) -> (Focused, FocusRing) {
    (Focused(false), FocusRing { width, color })
}

/// Sets `focused` only when it differs.
pub fn set(focused: &mut Mut<Focused>, on: bool) {
    if focused.0 != on {
        focused.0 = on;
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, look.before(bevy::ui::UiSystems::Prepare));
}

type Looks<'a> = (
    &'a Focused,
    Option<&'a FocusFill>,
    Option<&'a FocusRing>,
    Option<&'a FocusInk>,
    Option<&'a mut BackgroundColor>,
    Option<&'a mut Outline>,
    Option<&'a mut Shape>,
    Option<&'a mut ImageNode>,
    Option<&'a mut TextColor>,
    Option<&'a mut Ink>,
);

/// Applies each changed focus.
pub fn look(mut parts: Query<Looks, Changed<Focused>>) {
    for (focused, fill, ring, ink, bg, outline, shape, image, text, base) in &mut parts {
        let on = focused.0;
        if let (Some(f), Some(mut bg)) = (fill, bg) {
            let want = if on { f.focus } else { f.rest };
            if bg.0 != want {
                bg.0 = want;
            }
        }
        if let Some(r) = ring {
            let want = on.then_some((r.width, r.color));
            if let Some(mut shape) = shape {
                if shape.ring != want {
                    shape.ring = want;
                }
            } else if let Some(mut o) = outline {
                let color = if on { r.color } else { Color::NONE };
                if o.color != color {
                    o.color = color;
                }
            }
        }
        if let Some(i) = ink {
            let want = if on { i.focus } else { i.rest };
            // Kit text and faded icons take their colour from `Ink` every frame (opacity and
            // list fades, `shape::sync_shapes`): the focus colour goes there too.
            if let Some(mut base) = base
                && base.0 != want
            {
                base.0 = want;
            }
            if let Some(mut image) = image
                && image.color != want
            {
                image.color = want;
            }
            if let Some(mut t) = text
                && t.0 != want
            {
                t.0 = want;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_changes_looks_and_never_the_node() {
        let mut app = App::new();
        app.add_systems(Update, look);
        let node = Node {
            width: Val::Px(100.0),
            height: Val::Px(40.0),
            ..default()
        };
        let plain = app
            .world_mut()
            .spawn((
                node.clone(),
                BackgroundColor(Color::NONE),
                FocusFill {
                    rest: Color::NONE,
                    focus: Color::WHITE,
                },
                ring(2.0, Color::WHITE),
            ))
            .id();
        let sheared = app
            .world_mut()
            .spawn((node.clone(), Shape::fill(Color::BLACK), shape_ring(2.0, Color::WHITE)))
            .id();
        app.update();
        for e in [plain, sheared] {
            app.world_mut().get_mut::<Focused>(e).unwrap().0 = true;
        }
        app.update();
        let w = app.world();
        assert_eq!(w.get::<BackgroundColor>(plain).unwrap().0, Color::WHITE);
        assert_eq!(w.get::<Outline>(plain).unwrap().color, Color::WHITE);
        assert_eq!(w.get::<Shape>(sheared).unwrap().ring, Some((2.0, Color::WHITE)));
        assert_eq!(w.get::<Node>(plain).unwrap(), &node, "focus never touches the layout");
        assert_eq!(w.get::<Node>(sheared).unwrap(), &node);
        app.world_mut().get_mut::<Focused>(sheared).unwrap().0 = false;
        let ink = FocusInk {
            rest: Color::BLACK,
            focus: Color::WHITE,
        };
        let label = app
            .world_mut()
            .spawn((TextColor(Color::BLACK), Ink(Color::BLACK), ink, Focused(true)))
            .id();
        app.update();
        assert_eq!(app.world().get::<Shape>(sheared).unwrap().ring, None);
        assert_eq!(
            app.world().get::<Ink>(label).unwrap().0,
            Color::WHITE,
            "kit text keeps its focus colour through the fade pass"
        );
    }
}
