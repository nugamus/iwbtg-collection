//! The pause menu's Options view (#1654; m5 `D15_PauseOptions`, `Pause.dc.html` view=options):
//! only what is safe mid-run. It takes the run card's place on the right while the Options item
//! stays lit on the left. Nothing here changes the run, its timer or its replay.
//!
//! The rows: Music, Sound effects and Master (sliders; they apply to the voices playing at once,
//! `audio::apply_levels`, and save), Backdrop blur (slider, saved), Show the HUD (the Shift+Tab
//! switch, `hud::HudOptions`), Input display (`[hud.show] input`, saved), and Key bindings and
//! Audio offset, which open their screens from the menus for now and say so.
//!
//! Up and Down move, Left and Right step a slider by 10 % or flip a switch, Enter flips a switch,
//! Escape (or B) goes back to the items. The mouse hovers a row, clicks a switch, and clicks a
//! slider's track to set it there.
//!
//! The panel is built with the page and updated in place (UI foundation): a value change writes
//! a width, a colour or a text, never a respawn.

use bevy::picking::events::PointerClick;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use bevy::ui_widgets::{Activate, Button};

use super::Menu;
use crate::locale;
use crate::settings;
use crate::shell::RunScreen;
use crate::ui::fonts::Type;
use crate::ui::shape::Shape;
use crate::ui::tokens;
use crate::ui::widgets::{sheared, text};

/// A row of the view, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Opt {
    #[default]
    Music,
    Sounds,
    Master,
    Blur,
    Hud,
    InputDisplay,
    Bindings,
    Offset,
}

/// What a row's control is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Slider,
    Switch,
    /// A button for a screen that opens from the menus for now (shown faded).
    Later,
}

impl Opt {
    pub const ALL: [Opt; 8] = [
        Opt::Music,
        Opt::Sounds,
        Opt::Master,
        Opt::Blur,
        Opt::Hud,
        Opt::InputDisplay,
        Opt::Bindings,
        Opt::Offset,
    ];

    fn kind(self) -> Kind {
        match self {
            Opt::Music | Opt::Sounds | Opt::Master | Opt::Blur => Kind::Slider,
            Opt::Hud | Opt::InputDisplay => Kind::Switch,
            Opt::Bindings | Opt::Offset => Kind::Later,
        }
    }

    fn label(self) -> &'static str {
        locale::text(match self {
            Opt::Music => "pause_opt_music",
            Opt::Sounds => "pause_opt_sounds",
            Opt::Master => "pause_opt_master",
            Opt::Blur => "pause_opt_blur",
            Opt::Hud => "pause_opt_hud",
            Opt::InputDisplay => "pause_opt_input",
            Opt::Bindings => "pause_opt_bindings",
            Opt::Offset => "pause_opt_offset",
        })
    }

    fn desc(self) -> &'static str {
        match self {
            Opt::Music | Opt::Sounds | Opt::Master => "",
            Opt::Blur => locale::text("pause_opt_blur_desc"),
            Opt::Hud => locale::text("pause_opt_hud_desc"),
            Opt::InputDisplay => locale::text("pause_opt_input_desc"),
            Opt::Bindings => locale::text("pause_opt_bindings_desc"),
            Opt::Offset => locale::text("pause_opt_offset_desc"),
        }
    }

    /// The button's word on a row whose screen opens from the menus.
    fn button(self) -> &'static str {
        match self {
            Opt::Offset => locale::text("pause_opt_change"),
            _ => locale::text("pause_opt_open"),
        }
    }

    /// The next row down (`+1`) or up (`-1`), stopping at the ends.
    pub fn step(self, by: i32) -> Opt {
        let i = Opt::ALL.iter().position(|o| *o == self).unwrap_or(0) as i32;
        Opt::ALL[(i + by).clamp(0, Opt::ALL.len() as i32 - 1) as usize]
    }
}

/// What the rows show, read from the settings and the HUD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Values {
    pub music: u32,
    pub sounds: u32,
    pub master: u32,
    pub blur: u32,
    pub hud: bool,
    pub input: bool,
    pub offset_ms: u32,
}

impl Values {
    pub fn read(world: &World) -> Values {
        let s = settings::get();
        let hud = world
            .get_resource::<crate::hud::HudOptions>()
            .is_none_or(|h| h.0.shown);
        Values::of(&s, hud)
    }

    fn of(s: &settings::Settings, hud: bool) -> Values {
        Values {
            music: s.music_volume,
            sounds: s.sound_volume,
            master: s.master_volume,
            blur: s.backdrop_blur,
            hud,
            input: s.hud_show.input,
            offset_ms: s.audio_offset_ms,
        }
    }

    /// A slider's value in percent of its range (0 to 100).
    fn percent(&self, opt: Opt) -> Option<u32> {
        match opt {
            Opt::Music => Some(self.music),
            Opt::Sounds => Some(self.sounds),
            Opt::Master => Some(self.master),
            Opt::Blur => Some(self.blur * 100 / settings::MAX_BACKDROP_BLUR.max(1)),
            _ => None,
        }
    }

    fn switch(&self, opt: Opt) -> Option<bool> {
        match opt {
            Opt::Hud => Some(self.hud),
            Opt::InputDisplay => Some(self.input),
            _ => None,
        }
    }

    /// The text right of the control.
    fn shown(&self, opt: Opt) -> String {
        match opt {
            Opt::Offset => format!("+{} ms", self.offset_ms),
            o => self.percent(o).map_or(String::new(), |p| format!("{p}%")),
        }
    }
}

/// A change asked for by a key, a click or the pad; `apply` does it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adjust {
    /// Step a slider by `by` tenths, or flip a switch (any `by`, including 0 for Enter).
    Step(Opt, i32),
    /// Set a slider to this percent of its range (a click on its track).
    Set(Opt, u32),
}

/// One slider step, in percent of the range.
const STEP: i32 = 10;

/// Does `adjust`: saves the setting and applies it live.
pub fn apply(world: &mut World, adjust: Adjust) {
    let (opt, by, set) = match adjust {
        Adjust::Step(o, by) => (o, by, None),
        Adjust::Set(o, p) => (o, 0, Some(p.min(100))),
    };
    let slide = |v: u32, max: u32| -> u32 {
        match set {
            Some(p) => p * max / 100,
            None => (v as i32 + by * STEP * max as i32 / 100).clamp(0, max as i32) as u32,
        }
    };
    match opt {
        Opt::Music | Opt::Sounds | Opt::Master => {
            settings::update(|s| {
                let v = match opt {
                    Opt::Music => &mut s.music_volume,
                    Opt::Sounds => &mut s.sound_volume,
                    _ => &mut s.master_volume,
                };
                *v = slide(*v, settings::MAX_VOLUME);
            });
            crate::audio::apply_levels(world);
        }
        Opt::Blur => {
            settings::update(|s| s.backdrop_blur = slide(s.backdrop_blur, settings::MAX_BACKDROP_BLUR));
            let blur = settings::get().backdrop_blur as f32 / 100.0;
            if let Some(mut b) = world.get_resource_mut::<crate::ui::backdrop::Backdrop>() {
                b.blur = blur;
            }
        }
        Opt::Hud => {
            if set.is_none()
                && let Some(mut hud) = world.get_resource_mut::<crate::hud::HudOptions>()
            {
                hud.0.shown = !hud.0.shown;
            }
        }
        Opt::InputDisplay => {
            if set.is_none() {
                settings::update(|s| s.hud_show.input = !s.hud_show.input);
                let show = settings::get().hud_show;
                if let Some(mut shown) = world.get_resource_mut::<crate::hud::Shown>() {
                    shown.0 = show;
                }
            }
        }
        Opt::Bindings | Opt::Offset => {}
    }
}

/// The view's root (shown while `Menu::options`) and the run card's (hidden then).
#[derive(Component)]
pub struct Panel;
#[derive(Component)]
pub struct RunCard;
/// The hint bar of each view.
#[derive(Component)]
pub struct Hints(pub bool);
/// `Paused`, or `Paused · Options`.
#[derive(Component)]
pub struct Title;

#[derive(Component)]
pub struct OptRow(pub Opt);
#[derive(Component)]
pub struct Track(pub Opt);
#[derive(Component)]
pub struct Fill(pub Opt);
#[derive(Component)]
pub struct Knob(pub Opt);
#[derive(Component)]
pub struct SwitchTrack(pub Opt);
#[derive(Component)]
pub struct SwitchKnob(pub Opt);
#[derive(Component)]
pub struct Value(pub Opt);

const SLIDER_W: f32 = 200.0;
const SWITCH_OFF: Color = Color::srgba(1.0, 1.0, 1.0, 0.2);

/// The view, `card_w` wide at the run card's place; hidden unless `open`.
pub fn panel(c: &mut ChildSpawner, at: Vec2, card_w: f32, values: Values, accent: Color, open: bool) {
    c.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(at.x),
            top: Val::Px(at.y),
            width: Val::Px(card_w),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            display: if open { Display::Flex } else { Display::None },
            ..default()
        },
        Panel,
    ))
    .with_children(|c| {
        c.spawn(text(locale::text("pause_options"), Type::barlow(26.0, 700), tokens::INK));
        c.spawn((
            Node {
                padding: UiRect::bottom(Val::Px(6.0)),
                ..default()
            },
            children![text(locale::text("pause_options_note"), Type::barlow(15.0, 400), tokens::MUTED)],
        ));
        for opt in Opt::ALL {
            row(c, opt, &values, accent);
        }
    });
}

fn row(c: &mut ChildSpawner, opt: Opt, values: &Values, accent: Color) {
    c.spawn((
        Button,
        Hovered::default(),
        OptRow(opt),
        crate::ui::focus::FocusFill {
            rest: Color::NONE,
            focus: tokens::white(0.08),
        },
        crate::ui::focus::ring(2.0, tokens::white(0.5)),
        Node {
            min_height: Val::Px(60.0),
            padding: UiRect::axes(Val::Px(16.0), Val::Px(4.0)),
            border_radius: BorderRadius::all(Val::Px(10.0)),
            align_items: AlignItems::Center,
            column_gap: Val::Px(18.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|c| {
        c.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            flex_grow: 1.0,
            min_width: Val::Px(0.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(text(opt.label(), Type::barlow(16.0, 600), tokens::INK));
            let desc = opt.desc();
            if !desc.is_empty() {
                c.spawn(text(desc, Type::barlow(13.0, 400), tokens::MUTED));
            }
        });
        c.spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|c| {
            match opt.kind() {
                Kind::Slider => slider(c, opt, values.percent(opt).unwrap_or(0), accent),
                Kind::Switch => switch(c, opt, values.switch(opt).unwrap_or(false), accent),
                Kind::Later => {
                    c.spawn((
                        sheared(36.0, 16.0, Shape::fill(tokens::white(0.1))),
                        children![text(opt.button(), Type::barlow(14.0, 700), tokens::INK.with_alpha(0.45))],
                    ));
                }
            }
            c.spawn((
                Node {
                    min_width: Val::Px(56.0),
                    justify_content: JustifyContent::FlexEnd,
                    ..default()
                },
                children![(
                    text(&values.shown(opt), Type::barlow(15.0, 600), tokens::INK),
                    TextLayout::no_wrap(),
                    Value(opt),
                )],
            ));
        });
    });
}

fn slider(c: &mut ChildSpawner, opt: Opt, pct: u32, accent: Color) {
    c.spawn((
        Node {
            width: Val::Px(SLIDER_W),
            height: Val::Px(8.0),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(tokens::white(0.12)),
        RelativeCursorPosition::default(),
        Track(opt),
    ))
    .with_children(|c| {
        c.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Percent(pct as f32),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(accent),
            Pickable::IGNORE,
            Fill(opt),
        ));
        c.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(pct as f32),
                top: Val::Px(-6.0),
                width: Val::Px(12.0),
                height: Val::Px(20.0),
                margin: UiRect::left(Val::Px(-6.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::WHITE),
            Pickable::IGNORE,
            Knob(opt),
        ));
    });
}

fn switch(c: &mut ChildSpawner, opt: Opt, on: bool, accent: Color) {
    c.spawn((
        Node {
            width: Val::Px(42.0),
            height: Val::Px(24.0),
            border_radius: BorderRadius::all(Val::Px(12.0)),
            ..default()
        },
        BackgroundColor(if on { accent } else { SWITCH_OFF }),
        SwitchTrack(opt),
    ))
    .with_child((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(3.0),
            left: Val::Px(if on { 21.0 } else { 3.0 }),
            width: Val::Px(18.0),
            height: Val::Px(18.0),
            border_radius: BorderRadius::all(Val::Px(9.0)),
            ..default()
        },
        BackgroundColor(Color::WHITE),
        SwitchKnob(opt),
    ));
}

/// Keeps the view in step: which side shows, the title and hints, the focused row and every
/// control's value. Writes only what changed.
#[allow(clippy::type_complexity, reason = "one pass over the view's parts")]
#[allow(clippy::too_many_arguments, reason = "one pass over the view's parts")]
pub fn looks(
    menu: Res<Menu>,
    hud: Option<Res<crate::hud::HudOptions>>,
    mut titles: Query<&mut Text, With<Title>>,
    mut rows: Query<(&OptRow, &mut crate::ui::focus::Focused)>,
    mut parts: ParamSet<(
        Query<(&mut Node, Has<Panel>), Or<(With<Panel>, With<RunCard>)>>,
        Query<(&mut Node, &Hints)>,
        Query<(&Fill, &mut Node)>,
        Query<(&Knob, &mut Node)>,
        Query<(&SwitchKnob, &mut Node)>,
    )>,
    mut tracks: Query<(&SwitchTrack, &mut BackgroundColor)>,
    mut values: Query<(&Value, &mut Text), Without<Title>>,
    mut last: Local<Option<Values>>,
) {
    if parts.p0().is_empty() {
        *last = None;
        return;
    }
    let open = menu.options;
    for (mut node, is_panel) in &mut parts.p0() {
        let want = if is_panel == open { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }
    for (mut node, h) in &mut parts.p1() {
        let want = if h.0 == open { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }
    let title = locale::text(if open { "pause_title_options" } else { "pause_paused" });
    for mut t in &mut titles {
        if t.0 != title {
            t.0 = title.to_owned();
        }
    }
    for (row, mut focused) in &mut rows {
        crate::ui::focus::set(&mut focused, open && row.0 == menu.opt);
    }
    if !open {
        return;
    }
    let v = Values::of(&settings::get(), hud.is_none_or(|h| h.0.shown));
    if *last == Some(v) {
        return;
    }
    *last = Some(v);
    let accent = super::page::accent(menu.classic);
    for (Fill(opt), mut node) in &mut parts.p2() {
        let want = Val::Percent(v.percent(*opt).unwrap_or(0) as f32);
        if node.width != want {
            node.width = want;
        }
    }
    for (Knob(opt), mut node) in &mut parts.p3() {
        let want = Val::Percent(v.percent(*opt).unwrap_or(0) as f32);
        if node.left != want {
            node.left = want;
        }
    }
    for (SwitchKnob(opt), mut node) in &mut parts.p4() {
        let want = Val::Px(if v.switch(*opt).unwrap_or(false) { 21.0 } else { 3.0 });
        if node.left != want {
            node.left = want;
        }
    }
    for (SwitchTrack(opt), mut bg) in &mut tracks {
        let want = if v.switch(*opt).unwrap_or(false) { accent } else { SWITCH_OFF };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (Value(opt), mut t) in &mut values {
        let want = v.shown(*opt);
        if t.0 != want {
            t.0 = want;
        }
    }
}

/// The pointer over a row focuses it.
pub fn hover(rows: Query<(&OptRow, &Hovered), Changed<Hovered>>, mut menu: ResMut<Menu>) {
    if !menu.options {
        return;
    }
    for (row, hovered) in &rows {
        if hovered.get() && menu.opt != row.0 {
            menu.opt = row.0;
            menu.pad = false;
        }
    }
}

/// A click on a switch's row flips it.
pub fn on_activate(
    activate: On<Activate>,
    rows: Query<&OptRow>,
    screen: Option<Res<State<RunScreen>>>,
    menu: Option<ResMut<Menu>>,
) {
    if let Some(mut menu) = menu
        && let Ok(row) = rows.get(activate.entity)
        && super::is_open(screen.as_deref())
        && menu.options
        && row.0.kind() == Kind::Switch
    {
        menu.opt = row.0;
        menu.adjust = Some(Adjust::Step(row.0, 0));
    }
}

/// A click on a slider's track sets it there.
pub fn on_track_click(
    click: On<PointerClick>,
    tracks: Query<(&Track, &RelativeCursorPosition)>,
    menu: Option<ResMut<Menu>>,
) {
    let Ok((track, position)) = tracks.get(click.entity) else {
        return;
    };
    let (Some(mut menu), Some(at)) = (menu, position.normalized) else {
        return;
    };
    if !menu.options {
        return;
    }
    let pct = ((at.x + 0.5).clamp(0.0, 1.0) * 100.0).round() as u32;
    menu.opt = track.0;
    menu.adjust = Some(Adjust::Set(track.0, pct));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_step_and_stop_at_the_ends() {
        assert_eq!(Opt::Music.step(-1), Opt::Music);
        assert_eq!(Opt::Music.step(1), Opt::Sounds);
        assert_eq!(Opt::Offset.step(1), Opt::Offset);
        assert_eq!(Opt::Blur.step(2), Opt::InputDisplay);
    }

    #[test]
    fn values_show_sliders_in_percent_and_the_offset_in_ms() {
        let s = settings::Settings {
            music_volume: 60,
            backdrop_blur: settings::MAX_BACKDROP_BLUR / 2,
            audio_offset_ms: 24,
            ..Default::default()
        };
        let v = Values::of(&s, true);
        assert_eq!(v.shown(Opt::Music), "60%");
        assert_eq!(v.shown(Opt::Blur), "50%");
        assert_eq!(v.shown(Opt::Offset), "+24 ms");
        assert_eq!(v.shown(Opt::Hud), "");
        assert_eq!(v.switch(Opt::Hud), Some(true));
    }
}
