//! The F3 debug overlay (docs/architecture/client/overlay.md): ticks and frames per second, the
//! tick, the room, the kid's position and speed, and the input the last tick saw.
//!
//! A small UI panel on the HUD's camera, at a fixed size (the UI scale applies, the game's scale
//! never does), at the bottom of the run info column, or in the window's top-left corner with the
//! HUD hidden, so it stays off the game. Off by default; F3 shows and hides it and saves the
//! choice (`hud.overlay`). `--overlay` shows it for one run without saving, also in a
//! `--screenshot` check, which otherwise never draws it.

use bevy::prelude::*;
use iwbtg_sim::world::SimStatus;
use std::time::{Duration, Instant};

use crate::hud::layout::Layout;
use crate::hud::{HudLayout, HudOptions};
use crate::input::Collector;
use crate::ui::fonts::Type;
use crate::ui::{notice, tokens, widgets};
use crate::view::{Game, ScreenshotTo, Stats};

/// The overlay's command-line flag.
#[derive(clap::Args, Debug)]
pub struct OverlayArgs {
    /// Show the F3 debug overlay for this run (not saved), also in a `--screenshot` check.
    #[arg(long)]
    pub overlay: bool,
}

/// The panel's width at most, in UI px.
const WIDTH: f32 = 260.0;
/// The room the window's left edge must leave beside the game for the panel to go there with the
/// HUD hidden; with less, it goes in the window's top-left corner, over the game.
const MIN_BESIDE: f32 = 120.0;
/// Over the HUD's columns (10), under the pause overlay (20) and the notices (30).
const Z: i32 = 15;
/// The panel's text: small, tabular figures (the kit's `Type` always sets them).
const TEXT: Type = Type::barlow(12.0, 500);

pub struct OverlayPlugin(pub OverlayArgs);

impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Forced(self.0.overlay))
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                (toggle, place.after(crate::screen::apply), show).chain(),
            );
    }
}

/// `--overlay` was given.
#[derive(Resource)]
struct Forced(bool);

/// The overlay's panel.
#[derive(Component)]
struct Panel;

/// The overlay's text.
#[derive(Component)]
struct Label;

fn spawn(mut commands: Commands, forced: Res<Forced>, screenshot: Option<Res<ScreenshotTo>>) {
    // A `--screenshot` check draws it only with `--overlay`, whatever the setting.
    let on = forced.0 || (crate::settings::get().overlay && screenshot.is_none());
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            max_width: Val::Px(WIDTH),
            padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(tokens::GLASS),
        GlobalZIndex(Z),
        Pickable::IGNORE,
        if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
        Panel,
        children![(widgets::text("", TEXT, tokens::SOFT), Label)],
    ));
}

/// F3 shows or hides the overlay and saves the choice.
fn toggle(keys: Res<ButtonInput<KeyCode>>, mut panel: Single<&mut Visibility, With<Panel>>) {
    if !keys.just_pressed(KeyCode::F3) {
        return;
    }
    let on = **panel == Visibility::Hidden;
    **panel = if on {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    crate::settings::update(|s| s.overlay = on);
}

/// Where the panel goes, in UI px (`px` physical px per UI px).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spot {
    left: f32,
    /// From the window's top edge, or with `bottom` from its bottom edge.
    edge: f32,
    bottom: bool,
    /// The panel's width at most.
    width: f32,
}

/// With the HUD shown, the bottom of the run info column: run info sits in the column's middle
/// and a small window's runs past its top third, the leaderboard column's top holds the sound
/// note and its bottom the notices. With the HUD hidden, the top left of the window (the sound
/// note goes top right then), beside the game when there is room.
fn spot(l: &Layout, shown: bool, window: Vec2, px: f32) -> Spot {
    let m = notice::MARGIN;
    if shown && l.right.width() > 0.0 {
        return Spot {
            left: l.right.min.x / px + m,
            edge: m,
            bottom: true,
            width: (l.right.width() / px - 2.0 * m).clamp(0.0, WIDTH),
        };
    }
    let beside = l.game.min.x / px;
    let room = if beside >= MIN_BESIDE { beside } else { window.x / px };
    Spot {
        left: m,
        edge: m,
        bottom: false,
        width: (room - 2.0 * m).clamp(0.0, WIDTH),
    }
}

fn place(
    hud: Res<HudOptions>,
    layout: Res<HudLayout>,
    ui_scale: Res<UiScale>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    mut panel: Single<&mut Node, With<Panel>>,
) {
    let px = window.resolution.scale_factor() * ui_scale.0;
    if px <= 0.0 {
        return;
    }
    let size = window.physical_size().as_vec2();
    let s = spot(&layout.0, hud.0.shown, size, px);
    let (top, bottom) = if s.bottom {
        (Val::Auto, Val::Px(s.edge))
    } else {
        (Val::Px(s.edge), Val::Auto)
    };
    let want = (Val::Px(s.left), top, bottom, Val::Px(s.width));
    if (panel.left, panel.top, panel.bottom, panel.max_width) != want {
        (panel.left, panel.top, panel.bottom, panel.max_width) = want;
    }
}

/// Once a second recounts ticks and frames per second; writes the text while the panel shows.
fn show(
    game: NonSend<Game>,
    fixed: Res<Time<Fixed>>,
    mut stats: ResMut<Stats>,
    collector: Res<Collector>,
    panel: Single<&Visibility, With<Panel>>,
    mut label: Single<&mut Text, With<Label>>,
) {
    stats.frames += 1;
    let elapsed = stats.window_start.elapsed();
    if elapsed >= Duration::from_secs(1) {
        let secs = elapsed.as_secs_f64();
        stats.ticks_per_second = f64::from(stats.ticks) / secs;
        stats.frames_per_second = f64::from(stats.frames) / secs;
        stats.ticks = 0;
        stats.frames = 0;
        stats.window_start = Instant::now();
    }
    if **panel == Visibility::Hidden {
        return;
    }
    let world = game.sim.world();
    let status = match &world.status {
        SimStatus::Running => String::new(),
        SimStatus::Halted(e) => format!("\nHALTED: {e}"),
        SimStatus::Ended => "\nended".to_string(),
    };
    let room = game.sim.def().room(world.room).map_or_else(
        || "?".to_string(),
        |r| format!("{} ({}x{})", r.name, r.width, r.height),
    );
    let kid = match world.kid(&game.sim.rules().tick).and_then(|id| world.get(id)) {
        Some(k) => format!(
            "kid x {}  y {}\nhspeed {}  vspeed {}",
            two(k.m.x().get()),
            two(k.m.y().get()),
            two(k.m.hspeed().get()),
            two(k.m.vspeed().get()),
        ),
        None => "no kid".to_string(),
    };
    let text = format!(
        "ticks/s {:.1} (game {})  fps {:.0}\ntick {}  [F3 hides]{status}\nroom {room}\n{kid}\n{input}",
        stats.ticks_per_second,
        fixed.timestep().as_secs_f64().recip().round(),
        stats.frames_per_second,
        world.tick,
        input = collector.describe(),
    );
    if label.0 != text {
        label.0 = text;
    }
}

/// A number with two decimals, without the sign of a tiny negative that rounds to zero.
fn two(v: f64) -> String {
    let text = format!("{v:.2}");
    if text == "-0.00" { "0.00".to_string() } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::layout::{Options, compute};

    #[test]
    fn two_decimals_never_show_a_negative_zero() {
        assert_eq!(two(-0.000_001), "0.00");
        assert_eq!(two(-1.5), "-1.50");
        assert_eq!(two(567.4), "567.40");
    }

    /// At 16:9, 4:3, 21:9 and a small window, HUD shown or hidden, the panel's left edge is never
    /// inside the game, except with the HUD hidden and no room beside the game (4:3).
    #[test]
    fn the_panel_stays_off_the_game() {
        let view = Vec2::new(800.0, 608.0);
        for (w, h) in [(1920.0, 1080.0), (1024.0, 768.0), (2560.0, 1080.0), (960.0, 540.0)] {
            let window = Vec2::new(w, h);
            let area = Rect::from_corners(Vec2::ZERO, window);
            for shown in [true, false] {
                let o = Options {
                    shown,
                    ..Options::default()
                };
                let l = compute(area, 1.0, view, o);
                let px = l.content;
                let s = spot(&l, shown, window, px);
                assert!(s.width > 0.0 && s.edge >= 0.0, "{w}x{h} shown {shown}");
                assert_eq!(s.bottom, shown, "{w}x{h}: bottom of run info, or top left without a HUD");
                let (x0, x1) = (s.left * px, (s.left + s.width) * px);
                let off = x1 <= l.game.min.x || x0 >= l.game.max.x;
                let room_beside = l.game.min.x >= MIN_BESIDE;
                assert!(off || (!shown && !room_beside), "{w}x{h} shown {shown}: {x0}..{x1} vs {:?}", l.game);
                assert!(x1 <= w, "{w}x{h} shown {shown}: off the window");
            }
        }
    }
}
