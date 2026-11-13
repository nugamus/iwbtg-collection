//! The shared components of the M4 and M6 screens, each a function that returns a bundle:
//! sheared panel, pill, button (rest, hover, pressed, focus-visible), label/value stat, star pill,
//! mod pill, the Ranked/Unranked pills and one-line text cut with `…`. Sizes and colours are the mockups' (`Main.dc.html`,
//! `Loading.dc.html`, `RunInfo.dc.html`, `ModSelect.dc.html`).

use bevy::input_focus::tab_navigation::{TabIndex, TabNavigationPlugin};
use bevy::input_focus::{InputFocus, InputFocusVisible};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::Button;

use super::fonts::Type;
use super::icons::{Icon, icon};
use super::motion::{Ease, ReducedMotion, Scale, Tween};
use super::shape::{Ink, Shape, UiOpacity};
use super::tokens::{self, star_color};

pub(super) fn plugin(app: &mut App) {
    if !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_plugins(TabNavigationPlugin);
    }
    app.add_systems(Update, (button_looks, fit_ellipses));
}

/// The ellipsis that ends text cut at its box.
pub const ELLIPSIS: char = '…';

/// One line of text in a kit style that ends in `…` where it does not fit its box (CSS
/// `white-space: nowrap; overflow: hidden; text-overflow: ellipsis`). The returned bundle is the
/// box: its width must come from its parent (stretched across a column, or `flex_grow` in a row),
/// not from the text, which it clips. The cut re-fits when the box width, the UI scale or
/// [`Ellipsis::full`] changes, measured on the laid-out text (`fit_ellipses`).
pub fn ellipsis_text(content: &str, style: Type, color: Color) -> impl Bundle + use<> {
    (
        Node {
            min_width: Val::Px(0.0),
            overflow: Overflow::clip_x(),
            ..default()
        },
        children![(
            text(content, style, color),
            TextLayout::no_wrap(),
            Ellipsis::new(style.set(content)),
        )],
    )
}

/// On the text of an [`ellipsis_text`] box: the whole line. Set `full` to change the text; the
/// `Text` itself is written by `fit_ellipses`.
#[derive(Component, Debug)]
pub struct Ellipsis {
    pub full: String,
    /// The box width (physical px) and text the shown line was fitted for.
    fitted: Option<(f32, String)>,
    /// `full` plus `…` is being laid out, to be measured next frame.
    measuring: bool,
}

impl Ellipsis {
    pub fn new(full: impl Into<String>) -> Ellipsis {
        Ellipsis {
            full: full.into(),
            fitted: None,
            measuring: false,
        }
    }
}

/// Fits each [`Ellipsis`] text to its box. When the box width (which follows the UI scale) or the
/// text changes, the text is set to the whole line plus `…` for one frame; the next frame reads
/// that layout's cluster widths and shows the whole line if it fits, else the longest prefix that
/// fits with the `…`.
fn fit_ellipses(
    mut texts: Query<(&mut Ellipsis, &mut Text, &bevy::text::ComputedTextBlock, &ChildOf)>,
    boxes: Query<&ComputedNode>,
) {
    for (mut ellipsis, mut text, block, child_of) in &mut texts {
        let Ok(node) = boxes.get(child_of.parent()) else {
            continue;
        };
        let width = node.content_box().width();
        if width <= 0.0 {
            // Not laid out yet.
            continue;
        }
        let ellipsis = ellipsis.bypass_change_detection();
        if !ellipsis.measuring {
            let same = ellipsis
                .fitted
                .as_ref()
                .is_some_and(|(w, full)| (w - width).abs() < 0.5 && *full == ellipsis.full);
            if !same {
                ellipsis.measuring = true;
                text.0 = format!("{}{ELLIPSIS}", ellipsis.full);
            }
            continue;
        }
        // The layout is the probe's once its text runs to the probe's end.
        let Some(shown) = fit_line(&ellipsis.full, block.buffer(), width) else {
            continue;
        };
        ellipsis.measuring = false;
        ellipsis.fitted = Some((width, ellipsis.full.clone()));
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

/// The line to show in a box `width` wide, from `layout` of `full` followed by `…` (one line, left
/// to right): `full` when it fits, else its longest prefix that fits with the `…`, trailing spaces
/// dropped. `None` when `layout` is not of that text.
fn fit_line<B: parley::Brush>(full: &str, layout: &parley::Layout<B>, width: f32) -> Option<String> {
    // The right edge of each cluster, by the end of its text.
    let mut edges: Vec<(usize, f32)> = Vec::new();
    for line in layout.lines() {
        for item in line.items() {
            if let parley::PositionedLayoutItem::GlyphRun(run) = item {
                let mut x = run.offset();
                for cluster in run.run().visual_clusters() {
                    x += cluster.advance();
                    edges.push((cluster.text_range().end, x));
                }
            }
        }
    }
    let end = full.len();
    if edges.iter().map(|e| e.0).max() != Some(end + ELLIPSIS.len_utf8()) {
        return None;
    }
    let right = |upto: usize| {
        edges
            .iter()
            .filter(|e| e.0 <= upto)
            .map(|e| e.1)
            .fold(0.0f32, f32::max)
    };
    let whole = right(end);
    if whole <= width + 0.01 {
        return Some(full.to_string());
    }
    let dots = right(end + ELLIPSIS.len_utf8()) - whole;
    let cut = edges
        .iter()
        .filter(|&&(at, x)| at <= end && x + dots <= width + 0.01)
        .map(|e| e.0)
        .max()
        .unwrap_or(0);
    Some(format!("{}{ELLIPSIS}", full[..cut].trim_end()))
}

/// Text in a kit style and colour. Tabular numerals are on; the colour follows `UiOpacity` and
/// `FadeMask` above it.
pub fn text(content: &str, style: Type, color: Color) -> impl Bundle + use<> {
    (
        Text::new(style.set(content)),
        style.font(),
        style.spacing(),
        bevy::text::LineHeight::RelativeToFont(1.15),
        TextColor(color),
        Ink(color),
    )
}

/// One line of text in several styles and colours (`Deaths <b>48,213</b>`): the first part is the
/// text node, the rest are spans of it. Unlike [`text`], the spans do not follow `UiOpacity` or
/// `FadeMask`, so keep it out of fading lists.
pub fn rich(parts: Vec<(String, Type, Color)>) -> impl Bundle {
    let mut parts = parts.into_iter();
    let (first, style, color) = parts
        .next()
        .unwrap_or_else(|| (String::new(), Type::BODY, tokens::INK));
    (
        text(&first, style, color),
        TextLayout::no_wrap(),
        Children::spawn(SpawnIter(parts.map(|(s, style, color)| {
            (
                TextSpan::new(style.set(&s)),
                style.font(),
                style.spacing(),
                TextColor(color),
            )
        }))),
    )
}

/// A sheared node `height` px tall, `pad` px of padding left and right, its children centred
/// on one row. Panels, pills and buttons are all this with a different shape and content.
pub fn sheared(height: f32, pad: f32, shape: Shape) -> (Node, Shape) {
    (
        Node {
            height: Val::Px(height),
            padding: UiRect::horizontal(Val::Px(pad)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(10.0),
            flex_shrink: 0.0,
            ..default()
        },
        shape,
    )
}

/// A sheared panel of a fixed size; its content is laid out from the left edge, 26 px in
/// (the game panels of the list).
pub fn panel(width: f32, height: f32, shape: Shape) -> (Node, Shape) {
    (
        Node {
            width: Val::Px(width),
            height: Val::Px(height),
            padding: UiRect::horizontal(Val::Px(26.0)),
            align_items: AlignItems::Center,
            column_gap: Val::Px(16.0),
            flex_shrink: 0.0,
            ..default()
        },
        shape,
    )
}

/// A one-line pill: `label` in `style` on `fill`.
pub fn pill(label: &str, style: Type, fill: Color, ink: Color, height: f32, pad: f32) -> impl Bundle + use<> {
    (
        sheared(height, pad, Shape::fill(fill)),
        children![text(label, style, ink)],
    )
}

/// `★ 6.84` on the star colour of that rating.
pub fn star_pill(stars: f32) -> impl Bundle {
    pill(
        &format!("★ {stars:.2}"),
        Type::STARS,
        star_color(stars),
        tokens::ON_LIGHT,
        26.0,
        12.0,
    )
}

/// A mod's two-letter code (`SD`, `SL`): at least 40 px wide.
pub fn mod_pill(code: &str) -> impl Bundle + use<> {
    let mut node = sheared(26.0, 8.0, Shape::fill(tokens::CHIP));
    node.0.min_width = Val::Px(40.0);
    (node, children![text(code, Type::MOD_CODE, tokens::INK)])
}

/// The amber Unranked pill.
pub fn unranked_pill() -> impl Bundle {
    let style = Type {
        tracking: 0.1,
        ..Type::PILL_CAPS
    };
    pill("Unranked", style, tokens::AMBER, tokens::ON_AMBER, 26.0, 12.0)
}

/// The blue Ranked pill.
pub fn ranked_pill() -> impl Bundle {
    pill(
        "Ranked",
        Type::PILL_CAPS,
        tokens::RANKED,
        tokens::ON_RANKED,
        26.0,
        12.0,
    )
}

/// A tag (`Needle`, `Avoidance`).
pub fn tag_pill(label: &str) -> impl Bundle + use<> {
    pill(
        label,
        Type::barlow(14.0, 500),
        tokens::white(0.14),
        tokens::INK,
        26.0,
        12.0,
    )
}

/// A label over a value (`STAGES` / `7`). With an icon, a muted 14 px mark sits in front of the
/// label (the stats row of `Main.dc.html`); the value takes `color`.
pub fn stat(kind: Option<Icon>, label: &str, value: &str, color: Color) -> impl Bundle + use<> {
    let label_owned = label.to_string();
    let label_style = Type {
        tracking: 0.12,
        ..Type::LABEL
    };
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        },
        children![
            (
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
                    if let Some(kind) = kind {
                        c.spawn(icon(kind, 14.0, tokens::MUTED));
                    }
                    c.spawn(text(&label_owned, label_style, tokens::MUTED));
                })),
            ),
            text(value, Type::STAT, color),
        ],
    )
}

/// The kinds of button the mockups use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// The one primary action of a screen (Mods, Done): accent, 52 px, lifted by a red glow.
    Primary,
    /// Footer buttons (Back, Options, Random): light glass, 52 px.
    Secondary,
    /// Controls over art (Group, Sort, Scope): dark glass, 34 px.
    Control,
}

impl ButtonKind {
    fn height(self) -> f32 {
        match self {
            ButtonKind::Primary | ButtonKind::Secondary => 52.0,
            ButtonKind::Control => 34.0,
        }
    }

    fn pad(self) -> f32 {
        match self {
            ButtonKind::Primary => 28.0,
            ButtonKind::Secondary => 24.0,
            ButtonKind::Control => 14.0,
        }
    }

    fn style(self) -> Type {
        match self {
            ButtonKind::Primary => Type::PRIMARY,
            ButtonKind::Secondary => Type::BUTTON,
            ButtonKind::Control => Type::barlow(14.0, 600),
        }
    }

    fn rest(self) -> Color {
        match self {
            ButtonKind::Primary => tokens::ACCENT,
            ButtonKind::Secondary => tokens::SECONDARY,
            ButtonKind::Control => tokens::GLASS,
        }
    }

    fn rest_shape(self) -> Shape {
        let shape = Shape::fill(self.rest());
        match self {
            ButtonKind::Primary => {
                shape.with_shadow(8.0, 24.0, tokens::over(tokens::ACCENT, 0.35, tokens::GROUND))
            }
            _ => shape,
        }
    }
}

/// The colour of a button's fill: hover mixes in white, pressing mixes in black (both 0 to 1).
fn button_fill(kind: ButtonKind, hover: f32, press: f32) -> Color {
    let rest = kind.rest().to_srgba();
    let lift = |c: f32, toward: f32, by: f32| c + (toward - c) * by;
    let up = 0.12 * hover;
    let down = 0.18 * press;
    let mut c = Srgba::new(
        lift(lift(rest.red, 1.0, up), 0.0, down),
        lift(lift(rest.green, 1.0, up), 0.0, down),
        lift(lift(rest.blue, 1.0, up), 0.0, down),
        rest.alpha,
    );
    // Glass gets more opaque as it lights up, the way a lighter CSS rgba() would. White glass
    // takes its CSS alpha (.12 to .20) through `tokens::white`, as at rest (#692).
    if kind == ButtonKind::Secondary && hover > 0.0 {
        c.alpha = tokens::white(0.12 + 0.08 * hover).alpha();
    } else if rest.alpha < 1.0 {
        c.alpha = (rest.alpha + 0.08 * hover).min(1.0);
    }
    c.into()
}

/// The focus ring: drawn only when focus came from the keyboard (CSS `:focus-visible`).
/// `rgba(255,255,255,.85)` as [`tokens::white`]`(0.85)` (#692).
pub const FOCUS_RING: (f32, Color) = (2.0, Color::srgba(1.0, 1.0, 1.0, 0.7159));

/// A button's state, eased.
#[derive(Component, Debug)]
pub struct ButtonLook {
    kind: ButtonKind,
    hover: f32,
    press: f32,
}

/// A button: `label` in the kind's style. Clicks, Enter and Space send `bevy::ui_widgets::Activate`
/// to it; Tab moves the focus ring between buttons.
pub fn button(label: &str, kind: ButtonKind) -> impl Bundle + use<> {
    button_with(
        kind,
        kind.height(),
        kind.pad(),
        text(label, kind.style(), tokens::INK),
    )
}

/// A button of `kind` at another size (`height`, `pad` px left and right) with `content` in it:
/// the results footer's 48 and 38 px buttons, the Retry button's `hold R` beside its label.
pub fn button_with<B: Bundle>(kind: ButtonKind, height: f32, pad: f32, content: B) -> impl Bundle + use<B> {
    (
        sheared(height, pad, kind.rest_shape()),
        Button,
        Hovered::default(),
        TabIndex(0),
        ButtonLook {
            kind,
            hover: 0.0,
            press: 0.0,
        },
        UiTransform::default(),
        Tween::<Scale>::at_rest(1.0, tokens::motion::HOVER, Ease::OutQuint),
        Children::spawn(Spawn(content)),
    )
}

/// How much of a button that cannot be used yet shows.
pub const OFF_OPACITY: f32 = 0.4;

/// A button that cannot be used yet (Upload replay until M7): the kind's rest fill and `content`
/// faded to [`OFF_OPACITY`]; no hover, no press, no focus, no `Activate`.
pub fn button_off<B: Bundle>(kind: ButtonKind, height: f32, pad: f32, content: B) -> impl Bundle + use<B> {
    (
        sheared(height, pad, Shape::fill(kind.rest())),
        UiOpacity(OFF_OPACITY),
        Children::spawn(Spawn(content)),
    )
}

/// What `button_looks` reads and writes on each button.
type ButtonParts = (
    Entity,
    &'static mut ButtonLook,
    &'static Hovered,
    Has<Pressed>,
    &'static mut Shape,
    &'static mut Tween<Scale>,
);

/// Eases each button toward its hover and press state and sets its fill, ring and scale.
fn button_looks(
    time: Res<Time>,
    reduced: Res<ReducedMotion>,
    focus: Res<InputFocus>,
    focus_visible: Res<InputFocusVisible>,
    mut buttons: Query<ButtonParts>,
) {
    let step = if reduced.0 {
        1.0
    } else {
        time.delta_secs() / tokens::motion::HOVER
    };
    for (entity, mut look, hovered, pressed, mut shape, mut scale) in &mut buttons {
        let toward = |v: f32, target: f32| {
            if v < target {
                (v + step).min(target)
            } else {
                (v - step).max(target)
            }
        };
        let hover = toward(look.hover, if hovered.get() { 1.0 } else { 0.0 });
        let press = toward(look.press, if pressed { 1.0 } else { 0.0 });
        if hover.to_bits() != look.hover.to_bits() || press.to_bits() != look.press.to_bits() {
            look.hover = hover;
            look.press = press;
        }
        scale.retarget(if pressed { 0.96 } else { 1.0 });
        let mut want = look.kind.rest_shape();
        want.fill = button_fill(
            look.kind,
            Ease::OutQuint.at(look.hover),
            Ease::OutQuint.at(look.press),
        );
        if focus_visible.0 && focus.get() == Some(entity) {
            want.ring = Some(FOCUS_RING);
        }
        if *shape != want {
            *shape = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `text` laid out in Barlow 15/600 at `scale` on one line, as Bevy lays out UI text.
    fn barlow_line(text: &str, scale: f32) -> parley::Layout<[u8; 4]> {
        use parley::{FontContext, FontFamily, FontWeight, LayoutContext, StyleProperty};
        let mut fonts = FontContext::new();
        fonts.collection.register_fonts(
            parley::fontique::Blob::new(std::sync::Arc::new(
                include_bytes!("../../assets/fonts/Barlow-SemiBold.ttf").to_vec(),
            )),
            None,
        );
        let mut layouts = LayoutContext::new();
        let mut builder = layouts.ranged_builder(&mut fonts, text, scale, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source("Barlow".into())));
        builder.push_default(StyleProperty::FontSize(15.0));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(600.0)));
        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        layout
    }

    #[test]
    fn a_line_that_does_not_fit_ends_in_an_ellipsis_at_any_scale() {
        let full = "3 Nov 2026";
        for scale in [1.0, 1.5, 2.0] {
            let probe = barlow_line(&format!("{full}{ELLIPSIS}"), scale);
            let whole = barlow_line(full, scale).width();
            assert_eq!(fit_line(full, &probe, whole + 1.0).as_deref(), Some(full), "fits");

            let narrow = whole * 0.8;
            let shown = fit_line(full, &probe, narrow).expect("probe layout");
            assert!(
                shown.ends_with(ELLIPSIS) && shown.len() < full.len() + 3,
                "{shown}"
            );
            assert!(full.starts_with(shown.trim_end_matches(ELLIPSIS)), "{shown}");
            let drawn = barlow_line(&shown, scale).width();
            assert!(drawn <= narrow + 0.01, "{shown} is {drawn} px in {narrow}");
            // The next visible character would not have fitted.
            let kept = shown.len() - ELLIPSIS.len_utf8();
            let next = full[kept..]
                .char_indices()
                .find(|(_, c)| !c.is_whitespace())
                .map(|(i, c)| kept + i + c.len_utf8())
                .expect("a character was cut");
            let one_more = format!("{}{ELLIPSIS}", &full[..next]);
            assert!(
                barlow_line(&one_more, scale).width() > narrow,
                "{one_more} also fits"
            );

            assert_eq!(fit_line(full, &probe, 1.0).as_deref(), Some("…"), "no room");
        }
    }

    #[test]
    fn a_layout_of_other_text_is_not_measured() {
        let stale = barlow_line("3 Nov 2026", 1.0);
        assert_eq!(fit_line("3 Nov 2026", &stale, 40.0), None);
    }

    #[test]
    fn white_glass_goes_through_the_backdrop_rule() {
        assert!((FOCUS_RING.1.alpha() - tokens::white(0.85).alpha()).abs() < 0.0005);
        let rest = button_fill(ButtonKind::Secondary, 0.0, 0.0);
        assert!((rest.alpha() - tokens::SECONDARY.alpha()).abs() < 0.0005);
        let lit = button_fill(ButtonKind::Secondary, 1.0, 0.0);
        assert!((lit.alpha() - tokens::white(0.20).alpha()).abs() < 0.0005);
    }

    #[test]
    fn hover_lightens_and_press_darkens() {
        let rest = button_fill(ButtonKind::Primary, 0.0, 0.0).to_srgba();
        let hover = button_fill(ButtonKind::Primary, 1.0, 0.0).to_srgba();
        let press = button_fill(ButtonKind::Primary, 0.0, 1.0).to_srgba();
        assert_eq!(rest.to_hex(), "#FF3D55");
        assert!(hover.green > rest.green && hover.blue > rest.blue);
        assert!(press.red < rest.red);
    }

    #[test]
    fn a_button_follows_hover_press_and_keyboard_focus() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ReducedMotion(true))
            .init_resource::<InputFocus>()
            .init_resource::<InputFocusVisible>()
            .add_systems(Update, button_looks);
        let kind = ButtonKind::Secondary;
        let b = app
            .world_mut()
            .spawn((
                ButtonLook {
                    kind,
                    hover: 0.0,
                    press: 0.0,
                },
                Hovered(false),
                kind.rest_shape(),
                Tween::<Scale>::at_rest(1.0, tokens::motion::HOVER, Ease::OutQuint),
            ))
            .id();
        app.update();
        let shape = |app: &App| app.world().get::<Shape>(b).cloned().unwrap();
        assert_eq!(shape(&app), kind.rest_shape(), "at rest: the mockup colour");

        app.world_mut().entity_mut(b).insert(Hovered(true));
        app.update();
        assert_eq!(shape(&app).fill, button_fill(kind, 1.0, 0.0));

        app.world_mut().entity_mut(b).insert(Pressed);
        app.update();
        assert_eq!(shape(&app).fill, button_fill(kind, 1.0, 1.0));
        assert_eq!(
            app.world().get::<Tween<Scale>>(b).unwrap().to.to_bits(),
            0.96f32.to_bits()
        );

        app.world_mut().entity_mut(b).remove::<Pressed>();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(b, bevy::input_focus::FocusCause::Navigated);
        app.update();
        assert_eq!(shape(&app).ring, None, "focus from a click shows no ring");
        app.world_mut().resource_mut::<InputFocusVisible>().0 = true;
        app.update();
        assert_eq!(
            shape(&app).ring,
            Some(FOCUS_RING),
            "keyboard focus shows the ring"
        );
    }
}
