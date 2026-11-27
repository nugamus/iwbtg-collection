//! The toolbar on every screen outside a run (m5 `Toolbar.dc.html`, states in `B8_Toolbar`): 56 px
//! across the top. Left to right: the mode switch (Time Attack, Classic) with the accent line
//! under the picked mode, then at the right the clock and session time, notifications, settings
//! and you. There is no Home button: Back or Escape steps towards the main menu on every screen
//! (docs/product.md, Look and screen). The run screens hide it.
//!
//! A screen spawns [`toolbar`] at its top; the kit keeps it in step with [`Mode`] and the clock.
//! A click, Enter or Space on a mode sets [`Mode`]. Until the parts behind them exist, the right
//! end is what we have: no "now playing" without menu music, no badge on the bell until
//! notifications (m5 D2), and Sign in until accounts (#159).

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use super::fonts::Type;
use super::icons::{Icon, icon};
use super::mode::Mode;
use super::shape::{Ink, Shape};
use super::tokens;
use super::widgets::text;

/// The toolbar's height, logical px (40 of 768 in osu!, 56 of 1080 here).
pub const HEIGHT: f32 = 56.0;
/// The clock shows from this toolbar width up (logical px), as on the artboard.
const CLOCK_FROM: f32 = 1600.0;
/// The mode labels: 17 px, 700 when picked, 500 when not.
const MODE_ON: Type = Type::barlow(17.0, 700);
const MODE_OFF: Type = Type::barlow(17.0, 500);
/// Under the picked mode: 3 px, 12 px in from each side.
const LINE: f32 = 3.0;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Mode>()
        .add_systems(Update, (tabs, clock, wide))
        .add_observer(on_activate);
}

/// The toolbar's root.
#[derive(Component)]
pub struct Toolbar;

/// A tab of the mode switch.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeTab(pub Mode);

/// The parts of a mode tab that follow the pick.
#[derive(Component, Clone, Copy)]
enum TabPart {
    Icon(Mode),
    Label(Mode),
    Line(Mode),
}

/// The buttons at the right end.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarButton {
    /// A newer build is ready (#1606, #1675): hidden until the updater shows it.
    Update,
    Notifications,
    Settings,
    SignIn,
}

/// The clock and the session time under it.
#[derive(Component)]
struct Clock;
#[derive(Component)]
struct ClockTime;
#[derive(Component)]
struct SessionTime;

/// The toolbar, full width, for the picked `mode`.
pub fn toolbar(mode: Mode) -> impl Bundle {
    (
        Toolbar,
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(HEIGHT),
            flex_shrink: 0.0,
            align_items: AlignItems::Stretch,
            border: UiRect::bottom(Val::Px(1.0)),
            ..default()
        },
        BorderColor::all(tokens::white(0.06)),
        // `linear-gradient(180deg, rgba(6,7,11,.92), rgba(6,7,11,.62))`.
        BackgroundGradient::from(LinearGradient::to_bottom(vec![
            tokens::dark(SHADE, 0.92).into(),
            tokens::dark(SHADE, 0.62).into(),
        ])),
        children![
            (
                Node {
                    align_items: AlignItems::Stretch,
                    column_gap: Val::Px(4.0),
                    padding: UiRect::left(Val::Px(20.0)),
                    ..default()
                },
                Children::spawn(SpawnIter(Mode::ALL.into_iter().map(move |m| tab(m, m == mode)))),
            ),
            Node {
                flex_grow: 1.0,
                ..default()
            },
            update_button(),
            clock_block(),
            square(ToolbarButton::Notifications, Icon::Bell),
            square(ToolbarButton::Settings, Icon::Gear),
            sign_in(),
        ],
    )
}

/// The toolbar's shade: `#06070b`.
const SHADE: Color = Color::srgb_u8(0x06, 0x07, 0x0b);

fn tab_ink(on: bool) -> Color {
    if on { tokens::INK } else { tokens::MUTED }
}

fn tab(mode: Mode, on: bool) -> impl Bundle {
    let icon_ink = if on { mode.accent() } else { tokens::MUTED };
    (
        ModeTab(mode),
        Button,
        Hovered::default(),
        TabIndex(0),
        Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            padding: UiRect::horizontal(Val::Px(16.0)),
            ..default()
        },
        children![
            (
                icon(mode.small_icon(), 20.0, icon_ink),
                Ink(icon_ink),
                TabPart::Icon(mode)
            ),
            (
                text(mode.label(), if on { MODE_ON } else { MODE_OFF }, tab_ink(on)),
                TabPart::Label(mode)
            ),
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(12.0),
                    right: Val::Px(12.0),
                    bottom: Val::Px(0.0),
                    height: Val::Px(LINE),
                    border_radius: BorderRadius::top(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(if on { mode.accent() } else { Color::NONE }),
                TabPart::Line(mode)
            ),
        ],
    )
}

fn clock_block() -> impl Bundle {
    (
        Clock,
        Node {
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexEnd,
            padding: UiRect::horizontal(Val::Px(16.0)),
            ..default()
        },
        children![
            (text("", Type::barlow(16.0, 700), tokens::INK), ClockTime),
            (text("", Type::barlow(12.0, 400), tokens::MUTED), SessionTime),
        ],
    )
}

/// The update button, osu!lazer's top-right one: a blue sheared pill, `Display::None` until the
/// updater has a verified build ([`ToolbarButton::Update`]; `updater::toolbar_button`). A click
/// opens the update dialog.
fn update_button() -> impl Bundle {
    (
        Node {
            align_items: AlignItems::Center,
            padding: UiRect::horizontal(Val::Px(8.0)),
            display: Display::None,
            ..default()
        },
        UpdateSlot,
        children![(
            ToolbarButton::Update,
            Button,
            Hovered::default(),
            TabIndex(0),
            Node {
                height: Val::Px(36.0),
                padding: UiRect::horizontal(Val::Px(16.0)),
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            },
            Shape::fill(tokens::over(tokens::RANKED, 0.35, SHADE)).with_ring(1.5, tokens::RANKED),
            children![
                icon(Icon::Update, 18.0, tokens::INK),
                text(crate::locale::text("toolbar_update"), Type::barlow(15.0, 700), tokens::INK),
            ],
        )],
    )
}

/// The update button's slot: the updater sets its `display`.
#[derive(Component)]
pub struct UpdateSlot;

/// The bell and the gear: 56 px squares with a 22 px icon.
fn square(button: ToolbarButton, glyph: Icon) -> impl Bundle {
    (
        button,
        Button,
        Hovered::default(),
        TabIndex(0),
        Node {
            width: Val::Px(HEIGHT),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![icon(glyph, 22.0, tokens::INK)],
    )
}

/// Signed out: a sheared `Sign in` button, 36 px, `white(.12)` glass.
fn sign_in() -> impl Bundle {
    (
        Node {
            align_items: AlignItems::Center,
            padding: UiRect::new(Val::Px(10.0), Val::Px(20.0), Val::Px(0.0), Val::Px(0.0)),
            ..default()
        },
        children![(
            ToolbarButton::SignIn,
            Button,
            Hovered::default(),
            TabIndex(0),
            Node {
                height: Val::Px(36.0),
                padding: UiRect::horizontal(Val::Px(18.0)),
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            },
            Shape::fill(tokens::white(0.12)),
            children![
                icon(Icon::SignIn, 16.0, tokens::INK),
                text("Sign in", Type::barlow(15.0, 700), tokens::INK),
            ],
        )],
    )
}

/// A click, Enter or Space on a mode tab picks it.
fn on_activate(activate: On<Activate>, tabs: Query<&ModeTab>, mode: Option<ResMut<Mode>>) {
    if let (Ok(tab), Some(mut mode)) = (tabs.get(activate.entity), mode)
        && *mode != tab.0
    {
        *mode = tab.0;
    }
}

/// A mode tab's part and what of it can follow the pick.
type TabParts = (
    &'static TabPart,
    Option<&'static mut Ink>,
    Option<&'static mut TextFont>,
    Option<&'static mut BackgroundColor>,
);

/// The mode switch follows [`Mode`]: the picked tab in ink at 700 with its accent icon and line.
fn tabs(
    mode: Res<Mode>,
    added: Query<(), Added<TabPart>>,
    mut parts: Query<TabParts>,
) {
    if !mode.is_changed() && added.is_empty() {
        return;
    }
    for (part, ink, font, background) in &mut parts {
        match *part {
            TabPart::Icon(m) => {
                if let Some(mut ink) = ink {
                    ink.0 = if m == *mode { m.accent() } else { tokens::MUTED };
                }
            }
            TabPart::Label(m) => {
                if let Some(mut ink) = ink {
                    ink.0 = tab_ink(m == *mode);
                }
                if let Some(mut font) = font {
                    *font = if m == *mode { MODE_ON } else { MODE_OFF }.font();
                }
            }
            TabPart::Line(m) => {
                if let Some(mut bg) = background {
                    bg.0 = if m == *mode { m.accent() } else { Color::NONE };
                }
            }
        }
    }
}

/// The local time (`21:14`) and how long the client has been open (`Session 1:12:05`).
fn clock(
    time: Res<Time<Real>>,
    mut last: Local<Option<(String, u64)>>,
    mut clock: Query<&mut Text, (With<ClockTime>, Without<SessionTime>)>,
    mut session: Query<&mut Text, (With<SessionTime>, Without<ClockTime>)>,
    added: Query<(), Added<Clock>>,
) {
    let now = chrono::Local::now().format("%H:%M").to_string();
    let secs = time.elapsed().as_secs();
    let fresh = !added.is_empty();
    if !fresh && last.as_ref().is_some_and(|(n, s)| *n == now && *s == secs) {
        return;
    }
    *last = Some((now.clone(), secs));
    let session_line = format!("Session {}", session_time(secs));
    for mut t in &mut clock {
        t.0 = Type::barlow(16.0, 700).set(&now);
    }
    for mut t in &mut session {
        t.0 = Type::barlow(12.0, 400).set(&session_line);
    }
}

/// `h:mm:ss`.
fn session_time(secs: u64) -> String {
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// The clock shows only on a wide toolbar.
fn wide(
    bars: Query<&ComputedNode, (With<Toolbar>, Changed<ComputedNode>)>,
    mut clocks: Query<&mut Node, With<Clock>>,
) {
    for bar in &bars {
        let w = bar.size().x * bar.inverse_scale_factor();
        let want = if w >= CLOCK_FROM { Display::Flex } else { Display::None };
        for mut node in &mut clocks {
            if node.display != want {
                node.display = want;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_time_reads_hours_minutes_seconds() {
        assert_eq!(session_time(0), "0:00:00");
        assert_eq!(session_time(4325), "1:12:05");
    }

    /// A click on a mode tab picks the mode, and the tabs follow.
    #[test]
    fn a_mode_tab_picks_its_mode() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(plugin);
        app.world_mut().spawn(toolbar(Mode::TimeAttack));
        app.update();
        let classic = app
            .world_mut()
            .query::<(Entity, &ModeTab)>()
            .iter(app.world())
            .find(|(_, t)| t.0 == Mode::Classic)
            .map(|(e, _)| e)
            .unwrap();
        app.world_mut().trigger(Activate { entity: classic });
        app.update();
        assert_eq!(*app.world().resource::<Mode>(), Mode::Classic);
        let lines: Vec<(Mode, Color)> = app
            .world_mut()
            .query::<(&TabPart, &BackgroundColor)>()
            .iter(app.world())
            .filter_map(|(p, bg)| match p {
                TabPart::Line(m) => Some((*m, bg.0)),
                _ => None,
            })
            .collect();
        assert!(lines.contains(&(Mode::Classic, tokens::CLASSIC)));
        assert!(lines.contains(&(Mode::TimeAttack, Color::NONE)));
    }
}
