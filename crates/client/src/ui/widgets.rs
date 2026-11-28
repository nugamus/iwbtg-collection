//! The shared components of the M4 and M6 screens, each a function that returns a bundle:
//! sheared panel, pill, button (rest, hover, pressed, focus-visible), label/value stat, star pill,
//! mod pill, the Ranked/Unranked pills, the on/off switch and one-line text cut with `…`. Sizes and colours are the mockups' (`Main.dc.html`,
//! `Loading.dc.html`, `RunInfo.dc.html`, `ModSelect.dc.html`).

use bevy::input_focus::tab_navigation::{TabIndex, TabNavigationPlugin};
use bevy::input_focus::{InputFocus, InputFocusVisible};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::{Activate, Button};

use super::fonts::Type;
use super::icons::{Icon, icon};
use super::motion::{Ease, OffsetX, ReducedMotion, Scale, Tween};
use super::shape::{Ink, Shape, UiOpacity};
use super::tokens::{self, star_color};
use crate::locale;

pub(super) fn plugin(app: &mut App) {
    if !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_plugins(TabNavigationPlugin);
    }
    app.add_systems(
        Update,
        (
            chip_looks.before(button_looks),
            button_looks,
            fit_ellipses,
            switch_looks,
            mod_card_looks,
            search_placeholders,
        ),
    )
    .add_observer(flip_switch);
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

/// [`text`] with its own line height, a multiple of the font size (a wrapped caption's 1.4,
/// a big title's 0.95).
pub fn text_leading(content: &str, style: Type, color: Color, leading: f32) -> impl Bundle + use<> {
    (
        Text::new(style.set(content)),
        style.font(),
        style.spacing(),
        bevy::text::LineHeight::RelativeToFont(leading),
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
    pill(locale::text("pill_unranked"), style, tokens::AMBER, tokens::ON_AMBER, 26.0, 12.0)
}

/// The blue Ranked pill.
pub fn ranked_pill() -> impl Bundle {
    pill(
        locale::text("pill_ranked"),
        Type::PILL_CAPS,
        tokens::RANKED,
        tokens::ON_RANKED,
        26.0,
        12.0,
    )
}

/// The CLASSIC pill (m5 A5, A8): violet with white capitals, the Ranked pill's size.
pub fn classic_pill() -> impl Bundle {
    pill(
        locale::text("pill_classic"),
        Type::PILL_CAPS,
        tokens::CLASSIC,
        tokens::INK,
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
    /// A control that is switched on (the Cleared chip, the picked option of a dropdown):
    /// `white(.2)` glass, 34 px (`Main.dc.html`).
    ControlOn,
    /// The on/off switch button ([`switch`]): white glass clearing 3:1 ([`tokens::SWITCH`]), 52 px.
    Switch,
}

impl ButtonKind {
    fn height(self) -> f32 {
        match self {
            ButtonKind::Primary | ButtonKind::Secondary | ButtonKind::Switch => 52.0,
            ButtonKind::Control | ButtonKind::ControlOn => 34.0,
        }
    }

    fn pad(self) -> f32 {
        match self {
            ButtonKind::Primary => 28.0,
            ButtonKind::Secondary => 24.0,
            ButtonKind::Control | ButtonKind::ControlOn => 14.0,
            ButtonKind::Switch => 20.0,
        }
    }

    fn style(self) -> Type {
        match self {
            ButtonKind::Primary => Type::PRIMARY,
            ButtonKind::Secondary | ButtonKind::Switch => Type::BUTTON,
            ButtonKind::Control | ButtonKind::ControlOn => Type::barlow(14.0, 600),
        }
    }

    fn rest(self) -> Color {
        self.rest_in(tokens::ACCENT)
    }

    /// The rest fill with `accent` as the primary colour (the mode's, `ui::mode`).
    fn rest_in(self, accent: Color) -> Color {
        match self {
            ButtonKind::Primary => accent,
            ButtonKind::Secondary => tokens::SECONDARY,
            ButtonKind::Control => tokens::GLASS,
            ButtonKind::ControlOn => tokens::white(0.2),
            ButtonKind::Switch => tokens::SWITCH,
        }
    }

    fn rest_shape(self) -> Shape {
        self.rest_shape_in(tokens::ACCENT)
    }

    /// The rest shape with `accent` as the primary colour; the primary's glow follows it.
    fn rest_shape_in(self, accent: Color) -> Shape {
        let shape = Shape::fill(self.rest_in(accent));
        match self {
            ButtonKind::Primary => {
                shape.with_shadow(8.0, 24.0, tokens::over(accent, 0.35, tokens::GROUND))
            }
            _ => shape,
        }
    }
}

/// The colour of a button's fill: hover mixes in white, pressing mixes in black (both 0 to 1).
#[cfg(test)]
fn button_fill(kind: ButtonKind, hover: f32, press: f32) -> Color {
    button_fill_in(kind, tokens::ACCENT, hover, press)
}

/// [`button_fill`] with `accent` as the primary colour.
fn button_fill_in(kind: ButtonKind, accent: Color, hover: f32, press: f32) -> Color {
    let rest = kind.rest_in(accent).to_srgba();
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
        // Glass gets more opaque as it lights up. The switch's rest is [`tokens::SWITCH`]'s
        // raised alpha, so its hover starts there and lightens, rather than the artboard's
        // `.06 + .08` which would dip below the rest now (#1492).
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

impl ButtonLook {
    /// Turns the button into another kind (a picked row becomes `ControlOn`); its fill follows
    /// on the next frame. Writes only when it differs.
    pub fn set_kind(&mut self, kind: ButtonKind) {
        if self.kind != kind {
            self.kind = kind;
        }
    }
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

/// The muted key of a [`control`] (`Sort`): 14 px, 400, `#a2a8bc`.
pub const CONTROL_KEY: Type = Type::barlow(14.0, 400);
/// Space between a [`control`]'s key and value, as in `Main.dc.html`.
pub const CONTROL_KEY_GAP: f32 = 6.0;

/// A control over art with a key and a value (`Sort  Stars ▾`, `Scope  Global ▾` in
/// `Main.dc.html`): the key muted in [`CONTROL_KEY`], the value in the Control style (14 px, 600,
/// ink), 6 px apart. An empty `key` gives a value-only control (`Cleared`), as the mockup does.
/// The value text carries [`ControlValue`], so a screen can write the picked value into it.
pub fn control(key: &str, value: &str) -> impl Bundle + use<> {
    let kind = ButtonKind::Control;
    button_with(kind, kind.height(), kind.pad(), control_content(key, value))
}

/// The value text of a [`control`] or [`control_off`].
#[derive(Component)]
pub struct ControlValue;

fn control_content(key: &str, value: &str) -> impl Bundle + use<> {
    let key = key.to_string();
    let value = (
        text(value, ButtonKind::Control.style(), tokens::INK),
        ControlValue,
    );
    (
        Node {
            column_gap: Val::Px(CONTROL_KEY_GAP),
            align_items: AlignItems::Baseline,
            ..default()
        },
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            if !key.is_empty() {
                c.spawn(text(&key, CONTROL_KEY, tokens::MUTED));
            }
            c.spawn(value);
        })),
    )
}

/// A [`control`] that cannot be used yet (Status and Year until the game metadata exists):
/// faded to [`OFF_OPACITY`], no hover, no press, no `Activate`.
pub fn control_off(key: &str, value: &str) -> impl Bundle + use<> {
    let kind = ButtonKind::Control;
    button_off(kind, kind.height(), kind.pad(), control_content(key, value))
}

/// A value-only control that is on or off (the `Cleared` and `Uncleared` chips of
/// `Main.dc.html`, a dropdown's options): off is dark glass with soft text, on is `white(.2)`
/// glass with white text. Whoever owns the state writes [`Chip::on`]; the look follows.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chip {
    pub on: bool,
}

fn chip_kind(on: bool) -> ButtonKind {
    if on {
        ButtonKind::ControlOn
    } else {
        ButtonKind::Control
    }
}

fn chip_ink(on: bool) -> Color {
    if on { Color::WHITE } else { tokens::SOFT }
}

/// A [`Chip`] reading `label`, starting `on`.
pub fn chip(label: &str, on: bool) -> impl Bundle + use<> {
    let kind = chip_kind(on);
    (
        Chip { on },
        button_with(
            kind,
            kind.height(),
            kind.pad(),
            text(label, kind.style(), chip_ink(on)),
        ),
    )
}

/// A chip that cannot be picked yet (a dropdown option whose data does not exist).
pub fn chip_off(label: &str) -> impl Bundle + use<> {
    let kind = ButtonKind::Control;
    button_off(
        kind,
        kind.height(),
        kind.pad(),
        text(label, kind.style(), tokens::SOFT),
    )
}

/// Turns each changed chip's fill and text on or off.
fn chip_looks(
    mut chips: Query<(Entity, &Chip, &mut ButtonLook), Changed<Chip>>,
    children: Query<&Children>,
    mut inks: Query<&mut Ink>,
) {
    for (entity, chip, mut look) in &mut chips {
        let kind = chip_kind(chip.on);
        if look.kind != kind {
            look.kind = kind;
        }
        for part in children.iter_descendants(entity) {
            if let Ok(mut ink) = inks.get_mut(part) {
                ink.0 = chip_ink(chip.on);
            }
        }
    }
}

/// The dropdown under a control: an upright column of [`chip`]s on dark glass, drawn over
/// everything. Place it with `left`/`top` on its `Node`.
pub fn menu_panel() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(12.0)),
            ..default()
        },
        Shape {
            shear: 0.0,
            ..Shape::fill(tokens::CARD).with_shadow(10.0, 30.0, tokens::black(0.5))
        },
        GlobalZIndex(20),
    )
}

/// The search field's height (`Main.dc.html`).
pub const SEARCH_HEIGHT: f32 = 48.0;
/// The search field's text: 16 px, 400.
pub const SEARCH_TYPE: Type = Type::barlow(16.0, 400);

/// The entity of a [`search_field`] that holds the text (`EditableText`).
#[derive(Component)]
pub struct SearchField;

/// The placeholder of a [`search_field`], shown while it is empty.
#[derive(Component)]
struct SearchPlaceholder;

/// The search field of `Main.dc.html`: 48 px of sheared dark glass, the search icon, then the
/// text (16 px, ink) or `placeholder` (muted) while empty. It takes the keyboard focus when it
/// spawns; read the text from the `EditableText` on the [`SearchField`] entity.
pub fn search_field(placeholder: &str, initial: &str) -> impl Bundle + use<> {
    use bevy::input_focus::AutoFocus;
    use bevy::text::{EditableText, TextCursorStyle};
    use bevy::ui_widgets::TextInput;
    let mut outer = sheared(SEARCH_HEIGHT, 18.0, Shape::fill(tokens::GLASS));
    outer.0.flex_grow = 1.0;
    outer.0.justify_content = JustifyContent::FlexStart;
    outer.0.column_gap = Val::Px(12.0);
    let shown = if initial.is_empty() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    (
        outer,
        children![
            icon(Icon::Search, 18.0, tokens::MUTED),
            (
                Node {
                    flex_grow: 1.0,
                    min_width: Val::Px(0.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                children![
                    (
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            ..default()
                        },
                        text(placeholder, SEARCH_TYPE, tokens::MUTED),
                        SearchPlaceholder,
                        shown,
                    ),
                    (
                        Node {
                            width: Val::Percent(100.0),
                            ..default()
                        },
                        TextInput,
                        EditableText::new(initial),
                        SEARCH_TYPE.font(),
                        TextColor(tokens::INK),
                        TextCursorStyle {
                            color: tokens::INK,
                            selection_color: tokens::white(0.25),
                            ..default()
                        },
                        TabIndex(0),
                        AutoFocus,
                        SearchField,
                    ),
                ],
            ),
        ],
    )
}

/// Shows a search field's placeholder only while its text is empty.
fn search_placeholders(
    fields: Query<(&bevy::text::EditableText, &ChildOf), With<SearchField>>,
    children: Query<&Children>,
    mut marks: Query<&mut Visibility, With<SearchPlaceholder>>,
) {
    for (field, parent) in &fields {
        let want = if field.value().to_string().is_empty() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        for c in children.iter_descendants(parent.parent()) {
            if let Ok(mut v) = marks.get_mut(c)
                && *v != want
            {
                *v = want;
            }
        }
    }
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
        Tween::<Scale>::at_rest(1.0, tokens::motion::FEEDBACK, Ease::OutQuint),
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

/// An on/off switch: a [`ButtonKind::Switch`] button holding the track and knob and a label over
/// a hint (`ModSelect.dc.html`'s Auto-retry: 38 x 22 px track, 16 px knob, 15/700 over 12/400).
/// Off is the artboard's track (`white(.2)`) with the knob at the left; on, the track is the
/// accent and the knob slides right. The knob is ink in both states so it clears 3:1 against
/// either track, and the position (not the colour alone) shows the state (#1492). A click, Enter or Space flips [`Switch::on`];
/// whoever owns the setting reads it on `Changed<Switch>`, and writing `on` moves the switch.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Switch {
    pub on: bool,
}

/// The track of a [`switch`].
#[derive(Component)]
struct SwitchTrack;

/// The knob of a [`switch`].
#[derive(Component)]
struct SwitchKnob;

/// How far the knob slides: the track less the knob and its two 3 px insets.
const KNOB_TRAVEL: f32 = 38.0 - 16.0 - 2.0 * 3.0;

pub(crate) fn track_fill(on: bool) -> Color {
    if on { tokens::ACCENT } else { tokens::white(0.2) }
}

/// The knob, ink in both states. The artboard's off knob is `SOFT`, but over [`tokens::SWITCH`]'s
/// raised surface the off track is light enough that `SOFT` measures 2.66 to 2.78:1 against it;
/// ink clears 3:1 on both tracks (3.82 to 3.99:1 off, 3.16:1 on the accent). The state is the
/// knob's position (and the track colour), never the knob colour (#1492).
pub(crate) fn knob_fill() -> Color {
    tokens::INK
}

/// An upright rounded shape (the switch's track and knob are not sheared).
fn round(fill: Color, radius: f32) -> Shape {
    Shape {
        shear: 0.0,
        radius,
        ..Shape::fill(fill)
    }
}

/// The on/off switch button: `label` over `hint`, starting `on` (see [`Switch`]).
pub fn switch(label: &str, hint: &str, on: bool) -> impl Bundle + use<> {
    let knob_x = if on { KNOB_TRAVEL } else { 0.0 };
    (
        Switch { on },
        button_with(
            ButtonKind::Switch,
            ButtonKind::Switch.height(),
            ButtonKind::Switch.pad(),
            (
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(12.0),
                    ..default()
                },
                children![
                    (
                        Node {
                            width: Val::Px(38.0),
                            height: Val::Px(22.0),
                            flex_shrink: 0.0,
                            ..default()
                        },
                        round(track_fill(on), 11.0),
                        SwitchTrack,
                        children![(
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(3.0),
                                top: Val::Px(3.0),
                                width: Val::Px(16.0),
                                height: Val::Px(16.0),
                                ..default()
                            },
                            round(knob_fill(), 8.0),
                            SwitchKnob,
                            UiTransform::from_translation(Val2::px(knob_x, 0.0)),
                            Tween::<OffsetX>::at_rest(knob_x, tokens::motion::SELECT, Ease::OutQuint),
                        )],
                    ),
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            ..default()
                        },
                        children![
                            text(label, Type::barlow(15.0, 700), tokens::INK),
                            text(hint, Type::barlow(12.0, 400), tokens::MUTED),
                        ],
                    ),
                ],
            ),
        ),
    )
}

/// A click, Enter or Space on a switch flips it.
fn flip_switch(activate: On<Activate>, mut switches: Query<&mut Switch>) {
    if let Ok(mut switch) = switches.get_mut(activate.entity) {
        switch.on = !switch.on;
    }
}

/// What `switch_looks` writes on a switch's knob.
type KnobParts = (&'static mut Shape, &'static mut Tween<OffsetX>);

/// Moves each changed switch's knob and colours its track and knob.
fn switch_looks(
    switches: Query<(Entity, &Switch), Changed<Switch>>,
    children: Query<&Children>,
    mut tracks: Query<&mut Shape, (With<SwitchTrack>, Without<SwitchKnob>)>,
    mut knobs: Query<KnobParts, (With<SwitchKnob>, Without<SwitchTrack>)>,
) {
    for (entity, switch) in &switches {
        for part in children.iter_descendants(entity) {
            if let Ok(mut shape) = tracks.get_mut(part) {
                shape.fill = track_fill(switch.on);
            } else if let Ok((mut shape, mut tween)) = knobs.get_mut(part) {
                shape.fill = knob_fill();
                tween.retarget(if switch.on { KNOB_TRAVEL } else { 0.0 });
            }
        }
    }
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
    mode: Option<Res<super::mode::Mode>>,
    mut buttons: Query<ButtonParts>,
) {
    let accent = super::mode::accent(mode.as_deref());
    let step = if reduced.0 {
        1.0
    } else {
        time.delta_secs() / tokens::motion::FEEDBACK
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
        let mut want = look.kind.rest_shape_in(accent);
        want.fill = button_fill_in(
            look.kind,
            accent,
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

/// The ground the mod select sheet's cards sit on (the sheet's glass): card fills are the
/// artboard's whites and column colours mixed over it ([`tokens::over`]).
const MOD_GROUND: Color = tokens::CARD_SOLID;

/// A mod card on the mod select sheet (`ModSelect.dc.html`): 70 px, sheared, a code tile, the
/// name over a one-line description and optional option pills (Slow motion's 75% / 50%). Off it
/// is `white(.05)` glass with a `white(.1)` tile; on, the column `color` at .22 with a 2 px ring
/// and a solid tile. A click, Enter or Space sends `Activate`; the screen flips `on`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ModCard {
    pub color: Color,
    pub on: bool,
    hover: f32,
}

/// An option pill on a [`mod_card`]: its place in the card's options and whether it is the one
/// picked. The screen sets `on`; a click on it reaches the screen as a `PointerClick`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModOption {
    pub index: usize,
    pub on: bool,
}

/// The parts of a [`mod_card`] whose colour follows `on`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum CardPart {
    Tile,
    Code,
    Name,
    Description,
}

fn card_fill(card: &ModCard) -> Color {
    let lift = 0.05 * Ease::OutQuint.at(card.hover);
    if card.on {
        tokens::over(card.color, 0.22 + lift, MOD_GROUND)
    } else {
        tokens::over(Color::WHITE, 0.05 + lift, MOD_GROUND)
    }
}

fn card_part(part: CardPart, card: &ModCard) -> Color {
    match (part, card.on) {
        (CardPart::Tile, true) => card.color,
        (CardPart::Tile, false) => tokens::over(Color::WHITE, 0.1, MOD_GROUND),
        (CardPart::Code, true) => tokens::ON_LIGHT,
        (CardPart::Code, false) => tokens::SOFT,
        (CardPart::Name, true) => Color::WHITE,
        (CardPart::Name, false) => tokens::ROW_NAME,
        (CardPart::Description, true) => tokens::ROW_NAME,
        (CardPart::Description, false) => tokens::MUTED,
    }
}

fn option_look(on: bool) -> (Color, Color) {
    if on {
        (tokens::over(Color::WHITE, 0.18, MOD_GROUND), tokens::INK)
    } else {
        (Color::NONE, tokens::MUTED)
    }
}

/// The inside of a mod card: tile, name over description, options.
fn card_content(
    code: &str,
    name: &str,
    description: &str,
    card: &ModCard,
    options: &[(String, bool)],
    tag: Option<&str>,
) -> impl Bundle + use<> {
    let (code, name, description) = (code.to_owned(), name.to_owned(), description.to_owned());
    let tag = tag.map(str::to_owned);
    let card = *card;
    let options = options.to_vec();
    Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
        c.spawn((
            Node {
                width: Val::Px(48.0),
                height: Val::Px(44.0),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Shape {
                radius: 9.0,
                ..Shape::fill(card_part(CardPart::Tile, &card))
            },
            CardPart::Tile,
        ))
        .with_child((
            text(
                &code,
                Type::condensed(20.0, 700),
                card_part(CardPart::Code, &card),
            ),
            CardPart::Code,
        ));
        c.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            flex_grow: 1.0,
            min_width: Val::Px(0.0),
            overflow: Overflow::clip_x(),
            ..default()
        })
        .with_children(|t| {
            // The name, and the tag after it (`New`, `Coming later`; m5 D9).
            t.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    text(&name, Type::barlow(17.0, 700), card_part(CardPart::Name, &card)),
                    TextLayout::no_wrap(),
                    CardPart::Name,
                ));
                if let Some(tag) = &tag {
                    row.spawn((
                        Node {
                            height: Val::Px(17.0),
                            padding: UiRect::horizontal(Val::Px(6.0)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        Shape {
                            shear: 0.0,
                            radius: 3.0,
                            ..Shape::fill(tokens::white(0.1))
                        },
                        children![text(
                            tag,
                            Type {
                                tracking: 0.1,
                                caps: true,
                                ..Type::barlow(10.0, 700)
                            },
                            tokens::SOFT
                        )],
                    ));
                }
            });
            t.spawn((
                text(
                    &description,
                    Type::barlow(13.0, 400),
                    card_part(CardPart::Description, &card),
                ),
                TextLayout::no_wrap(),
                CardPart::Description,
            ));
        });
        if !options.is_empty() {
            c.spawn(Node {
                column_gap: Val::Px(4.0),
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|o| {
                for (index, (label, on)) in options.iter().enumerate() {
                    let (fill, ink) = option_look(*on);
                    o.spawn((
                        Node {
                            height: Val::Px(24.0),
                            padding: UiRect::horizontal(Val::Px(8.0)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        round(fill, 6.0),
                        ModOption { index, on: *on },
                    ))
                    .with_child(text(label, Type::barlow(13.0, 700), ink));
                }
            });
        }
    }))
}

/// The row a mod card lays out: 70 px, 14 px in on the left and 20 px on the right, 16 px gaps.
fn card_node() -> Node {
    Node {
        height: Val::Px(70.0),
        padding: UiRect {
            left: Val::Px(14.0),
            right: Val::Px(20.0),
            ..default()
        },
        align_items: AlignItems::Center,
        column_gap: Val::Px(16.0),
        flex_shrink: 0.0,
        ..default()
    }
}

/// A mod card (see [`ModCard`]) in the column `color`, `on` or off, with `options` (label,
/// picked) at its right end.
pub fn mod_card(
    code: &str,
    name: &str,
    description: &str,
    color: Color,
    on: bool,
    options: &[(String, bool)],
    tag: Option<&str>,
) -> impl Bundle + use<> {
    let card = ModCard {
        color,
        on,
        hover: 0.0,
    };
    let mut shape = Shape::fill(card_fill(&card));
    if on {
        shape.ring = Some((2.0, color));
    }
    (
        card_node(),
        shape,
        card,
        Button,
        Hovered::default(),
        TabIndex(0),
        UiTransform::default(),
        Tween::<Scale>::at_rest(1.0, tokens::motion::FEEDBACK, Ease::OutQuint),
        card_content(code, name, description, &card, options, tag),
    )
}

/// A mod card that cannot be picked yet (a mod the client does not play): off, faded to
/// [`OFF_OPACITY`], no hover, focus or `Activate`. `tag` is the small mark after the name
/// (`Coming later`).
pub fn mod_card_off(code: &str, name: &str, description: &str, tag: Option<&str>) -> impl Bundle + use<> {
    let card = ModCard {
        color: tokens::MUTED,
        on: false,
        hover: 0.0,
    };
    (
        card_node(),
        Shape::fill(card_fill(&card)),
        UiOpacity(OFF_OPACITY),
        card_content(code, name, description, &card, &[], tag),
    )
}

/// A picked mod's code in its column colour (the mod select footer): 32 px, at least 48 px wide,
/// Barlow Condensed 17/700 in dark ink.
pub fn selected_mod_pill(code: &str, color: Color) -> impl Bundle + use<> {
    let mut node = sheared(32.0, 10.0, Shape::fill(color));
    node.0.min_width = Val::Px(48.0);
    (
        node,
        children![text(code, Type::condensed(17.0, 700), tokens::ON_LIGHT)],
    )
}

/// What `mod_card_looks` reads and writes on each card.
type CardParts = (
    Entity,
    &'static mut ModCard,
    &'static Hovered,
    Has<Pressed>,
    &'static mut Shape,
    &'static mut Tween<Scale>,
);

/// The mod cards' hover, press, focus ring and colours, and their option pills.
#[allow(
    clippy::type_complexity,
    clippy::too_many_arguments,
    reason = "the card, its parts and its options"
)]
fn mod_card_looks(
    time: Res<Time>,
    reduced: Res<ReducedMotion>,
    focus: Res<InputFocus>,
    focus_visible: Res<InputFocusVisible>,
    mut cards: Query<CardParts, Without<CardPart>>,
    children: Query<&Children>,
    mut parts: Query<(&CardPart, Option<&mut Shape>, Option<&mut Ink>), Without<ModCard>>,
    mut options: Query<
        (&ModOption, &mut Shape, &Children),
        (Changed<ModOption>, Without<ModCard>, Without<CardPart>),
    >,
    mut inks: Query<&mut Ink, (Without<CardPart>, Without<ModOption>)>,
) {
    let step = if reduced.0 {
        1.0
    } else {
        time.delta_secs() / tokens::motion::FEEDBACK
    };
    for (entity, mut card, hovered, pressed, mut shape, mut scale) in &mut cards {
        let target = if hovered.get() { 1.0 } else { 0.0 };
        let hover = if card.hover < target {
            (card.hover + step).min(target)
        } else {
            (card.hover - step).max(target)
        };
        let recolour = card.is_changed();
        if hover.to_bits() != card.hover.to_bits() {
            card.hover = hover;
        }
        scale.retarget(if pressed { 0.96 } else { 1.0 });
        let mut want = shape.clone();
        want.fill = card_fill(&card);
        want.ring = if focus_visible.0 && focus.get() == Some(entity) {
            Some(FOCUS_RING)
        } else if card.on {
            Some((2.0, card.color))
        } else {
            None
        };
        if *shape != want {
            *shape = want;
        }
        if !recolour {
            continue;
        }
        for part in children.iter_descendants(entity) {
            if let Ok((&kind, part_shape, ink)) = parts.get_mut(part) {
                let color = card_part(kind, &card);
                if let Some(mut s) = part_shape
                    && s.fill != color
                {
                    s.fill = color;
                }
                if let Some(mut ink) = ink
                    && ink.0 != color
                {
                    ink.0 = color;
                }
            }
        }
    }
    for (option, mut shape, kids) in &mut options {
        let (fill, ink) = option_look(option.on);
        if shape.fill != fill {
            shape.fill = fill;
        }
        for kid in kids.iter() {
            if let Ok(mut i) = inks.get_mut(kid)
                && i.0 != ink
            {
                i.0 = ink;
            }
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
                Tween::<Scale>::at_rest(1.0, tokens::motion::FEEDBACK, Ease::OutQuint),
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

    #[test]
    fn a_switch_off_is_the_artboards_and_on_is_accent() {
        assert_eq!(track_fill(false), tokens::white(0.2));
        assert_eq!(track_fill(true), tokens::ACCENT);
        assert_eq!(knob_fill(), tokens::INK, "ink clears 3:1 on either track (#1492)");
        assert_eq!(KNOB_TRAVEL, 16.0, "the knob ends 3 px from the track's right end");
        assert_eq!(
            ButtonKind::Switch.rest(),
            tokens::SWITCH,
            "the surface is the 3:1 token, not an inline alpha (#1492)"
        );
        assert_eq!(ButtonKind::Switch.height(), 52.0);
        assert_eq!(ButtonKind::Switch.pad(), 20.0);
    }

    #[test]
    fn activating_a_switch_flips_it_and_moves_the_knob() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ReducedMotion(true))
            .init_resource::<InputFocus>()
            .init_resource::<InputFocusVisible>()
            .add_systems(Update, switch_looks)
            .add_observer(flip_switch);
        let s = app.world_mut().spawn(switch("Auto-retry", "hint", false)).id();
        app.update();
        // #1492: on and off differ by the knob's position as well as the track colour, so the
        // state does not rely on colour alone.
        assert_ne!(KNOB_TRAVEL, 0.0, "the knob moves between off and on");
        let parts = |app: &mut App| {
            let world = app.world_mut();
            let mut track = world.query_filtered::<&Shape, With<SwitchTrack>>();
            let track = track.single(world).unwrap().fill;
            let mut knob = world.query_filtered::<&Tween<OffsetX>, With<SwitchKnob>>();
            let knob = knob.single(world).unwrap().to;
            (track, knob)
        };
        assert_eq!(parts(&mut app), (track_fill(false), 0.0));

        app.world_mut().trigger(Activate { entity: s });
        app.update();
        assert!(app.world().get::<Switch>(s).unwrap().on);
        assert_eq!(parts(&mut app), (track_fill(true), KNOB_TRAVEL));

        // The setting's owner writes `on`; the switch follows.
        app.world_mut().get_mut::<Switch>(s).unwrap().on = false;
        app.update();
        assert_eq!(parts(&mut app), (track_fill(false), 0.0));
    }
}
