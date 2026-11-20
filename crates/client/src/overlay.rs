//! The F3 debug overlay (docs/architecture/client/overlay.md): ticks and frames per second, the
//! tick, the room, the kid's position and speed, and the input the last tick saw.
//!
//! A small UI panel on the HUD's camera, at a fixed size (the UI scale applies, the game's scale
//! never does): at the bottom of the run info column (narrow at its top in a small window), or
//! in the window's top-left corner with the HUD hidden, so it stays off the game and run info.
//! Off by default; F3 shows and hides it and saves the choice (`hud.overlay`). `--overlay` shows
//! it for one run without saving, also in a `--screenshot` check, which otherwise never draws it.

use bevy::prelude::*;
use iwbtg_sim::world::SimStatus;
use crate::time::{Duration, Instant};

use crate::hud::layout::{Density, Layout};
use crate::hud::{HudLayout, HudOptions};
use crate::input::Collector;
use crate::shell::{AppScreen, InRun, RUN_ONLY};
use crate::ui::fonts::Type;
use crate::ui::{notice, tokens, widgets};
use crate::view::{Game, ScreenshotTo, Stats};

/// The overlay's command-line flag.
#[derive(clap::Args, Debug, Default)]
pub struct OverlayArgs {
    /// Show the F3 debug overlay for this run (not saved), also in a `--screenshot` check.
    #[arg(long)]
    pub overlay: bool,
}

/// The overlay's place in the module list (main.rs): its flag and plugin.
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = OverlayArgs;

    fn add(
        app: &mut bevy::prelude::App,
        args: OverlayArgs,
        _startup: &crate::registration::Startup,
    ) {
        app.add_plugins(OverlayPlugin(args));
    }
}

/// The panel's width at most, in UI px.
const WIDTH: f32 = 260.0;
/// Narrower than this, the panel is narrow: shorter lines, smaller margins and padding.
const NARROW_BELOW: f32 = 140.0;
/// The gap to the window or column edge, wide and narrow.
const MARGIN: f32 = notice::MARGIN;
const NARROW_MARGIN: f32 = 6.0;
/// Inside the panel, wide and narrow.
const PADDING: (f32, f32) = (10.0, 8.0);
const NARROW_PADDING: (f32, f32) = (6.0, 6.0);
/// The room the window's left edge must leave beside the game for the panel to go there with the
/// HUD hidden; with less, it goes narrow in the window's top-left corner, over the game.
const MIN_BESIDE: f32 = 120.0;
/// Over the HUD's columns (10), under the pause overlay (20) and the notices (30).
const Z: i32 = 15;
/// The panel's text: small, tabular figures (the kit's `Type` always sets them).
const TEXT: Type = Type::barlow(12.0, 500);

pub struct OverlayPlugin(pub OverlayArgs);

impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Forced(self.0.overlay))
            .add_systems(OnEnter(AppScreen::Run), spawn.in_set(InRun))
            .add_systems(
                Update,
                (toggle, place.after(crate::screen::apply), show)
                    .chain()
                    .in_set(InRun),
            );
    }
}

/// `--overlay` was given.
#[derive(Resource)]
struct Forced(bool);

/// The overlay's panel; `narrow` while it uses the short lines.
#[derive(Component, Default)]
struct Panel {
    narrow: bool,
}

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
            padding: UiRect::axes(Val::Px(PADDING.0), Val::Px(PADDING.1)),
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
        Panel::default(),
        RUN_ONLY,
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
    /// The short lines (`NARROW_BELOW`).
    narrow: bool,
}

/// With the HUD shown, the bottom of the run info column: run info sits in the column's middle,
/// the leaderboard column's top holds the sound note and its bottom the notices. In a small window
/// (`Density::Small`) run info reaches below the column's middle, so the panel goes narrow at the
/// column's top instead. With the HUD hidden, the top left of the window (the sound note goes top
/// right then), beside the game when there is room; with less (4:3) it goes narrow over the
/// game's corner, the one case that covers the game: the game fills the window, so there is
/// nowhere else, and narrow it covers the least.
fn spot(l: &Layout, shown: bool, px: f32) -> Spot {
    if shown && l.right.width() > 0.0 {
        let small = l.density == Density::Small;
        let col = l.right.width() / px;
        let m = if small || col - 2.0 * MARGIN < NARROW_BELOW {
            NARROW_MARGIN
        } else {
            MARGIN
        };
        let width = (col - 2.0 * m).clamp(0.0, WIDTH);
        return Spot {
            left: l.right.min.x / px + m,
            edge: m,
            bottom: !small,
            width,
            narrow: small || width < NARROW_BELOW,
        };
    }
    let beside = l.game.min.x / px;
    if beside < MIN_BESIDE {
        return Spot {
            left: NARROW_MARGIN,
            edge: NARROW_MARGIN,
            bottom: false,
            width: NARROW_BELOW,
            narrow: true,
        };
    }
    let m = if beside - 2.0 * MARGIN < NARROW_BELOW {
        NARROW_MARGIN
    } else {
        MARGIN
    };
    let width = (beside - 2.0 * m).clamp(0.0, WIDTH);
    Spot {
        left: m,
        edge: m,
        bottom: false,
        width,
        narrow: width < NARROW_BELOW,
    }
}

fn place(
    hud: Res<HudOptions>,
    layout: Res<HudLayout>,
    ui_scale: Res<UiScale>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    panel: Single<(&mut Node, &mut Panel)>,
) {
    let (mut node, mut panel) = panel.into_inner();
    let px = window.resolution.scale_factor() * ui_scale.0;
    if px <= 0.0 {
        return;
    }
    let s = spot(&layout.0, hud.0.shown, px);
    let (top, bottom) = if s.bottom {
        (Val::Auto, Val::Px(s.edge))
    } else {
        (Val::Px(s.edge), Val::Auto)
    };
    let (x, y) = if s.narrow { NARROW_PADDING } else { PADDING };
    let want = (
        Val::Px(s.left),
        top,
        bottom,
        Val::Px(s.width),
        UiRect::axes(Val::Px(x), Val::Px(y)),
    );
    if (node.left, node.top, node.bottom, node.max_width, node.padding) != want {
        (node.left, node.top, node.bottom, node.max_width, node.padding) = want;
    }
    if panel.narrow != s.narrow {
        panel.narrow = s.narrow;
    }
}

/// Once a second recounts ticks and frames per second; writes the text while the panel shows.
fn show(
    game: NonSend<Game>,
    fixed: Res<Time<Fixed>>,
    mut stats: ResMut<Stats>,
    collector: Res<Collector>,
    panel: Single<(&Visibility, &Panel)>,
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
    let (visibility, panel) = *panel;
    if *visibility == Visibility::Hidden {
        return;
    }
    let world = game.sim.world();
    let status = match &world.status {
        SimStatus::Running => String::new(),
        SimStatus::Halted(e) => format!("\nHALTED: {e}"),
        SimStatus::Ended => "\nended".to_string(),
    };
    let room = game.sim.def().room(world.room);
    let kid = world.kid(&game.sim.rules().tick).and_then(|id| world.get(id));
    let kid = kid.map(|k| [k.m.x(), k.m.y(), k.m.hspeed(), k.m.vspeed()].map(|v| two(v.get())));
    let tps = stats.ticks_per_second;
    let rate = fixed.timestep().as_secs_f64().recip().round();
    let fps = stats.frames_per_second;
    let tick = world.tick;
    let input = collector.describe();
    let text = if panel.narrow {
        // About 16 characters a line: the room's size, the `[F3 hides]` hint and the long
        // labels go.
        let room = room.map_or("?", |r| r.name.as_str());
        let kid = kid.map_or_else(
            || "no kid".to_string(),
            |[x, y, h, v]| {
                format!(
                    "x {x}  y {y}
hs {h}  vs {v}"
                )
            },
        );
        format!(
            "tps {tps:.1} ({rate})
fps {fps:.0}  tick {tick}{status}
room {room}
{kid}
{input}"
        )
    } else {
        let room = room.map_or_else(
            || "?".to_string(),
            |r| format!("{} ({}x{})", r.name, r.width, r.height),
        );
        let kid = kid.map_or_else(
            || "no kid".to_string(),
            |[x, y, h, v]| {
                format!(
                    "kid x {x}  y {y}
hspeed {h}  vspeed {v}"
                )
            },
        );
        format!(
            "ticks/s {tps:.1} (game {rate})  fps {fps:.0}
tick {tick}  [F3 hides]{status}
room {room}
{kid}
{input}"
        )
    };
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

    /// At 16:9, 4:3, 21:9 and a small window, HUD shown or hidden, the panel stays off the game,
    /// except with the HUD hidden at 4:3, where the game fills the window: accepted there (#753),
    /// narrow in the window's top-left corner. With the HUD shown it is in the run info
    /// column; in the small window narrow at the column's top, clear of run info's rows below
    /// the column's middle (#753).
    #[test]
    fn the_panel_stays_off_the_game() {
        let view = Vec2::new(800.0, 608.0);
        for (w, h) in [
            (1920.0, 1080.0),
            (1024.0, 768.0),
            (2560.0, 1080.0),
            (960.0, 540.0),
        ] {
            let window = Vec2::new(w, h);
            let area = Rect::from_corners(Vec2::ZERO, window);
            for shown in [true, false] {
                let o = Options {
                    shown,
                    ..Options::default()
                };
                let l = compute(area, 1.0, view, o);
                let px = l.content;
                let s = spot(&l, shown, px);
                let case = format!("{w}x{h} shown {shown}");
                assert!(s.width > 0.0 && s.edge >= 0.0, "{case}");
                let (x0, x1) = (s.left * px, (s.left + s.width) * px);
                assert!(x0 >= 0.0 && x1 <= w, "{case}: off the window");
                let off = x1 <= l.game.min.x || x0 >= l.game.max.x;
                let small = l.density == Density::Small;
                if shown {
                    assert!(off, "{case}: {x0}..{x1} vs {:?}", l.game);
                    assert!(
                        x0 >= l.right.min.x && x1 <= l.right.max.x,
                        "{case}: run info column"
                    );
                    // Bottom of the column, or the top of a small window's column, narrow.
                    assert_eq!((s.bottom, s.narrow), (!small, small), "{case}");
                } else if l.game.min.x >= MIN_BESIDE {
                    assert!(off, "{case}: {x0}..{x1} vs {:?}", l.game);
                    assert!(!s.bottom, "{case}: top left");
                    assert_eq!(s.narrow, s.width < NARROW_BELOW, "{case}");
                } else {
                    // 4:3 without a HUD: narrow over the game's top-left corner, accepted.
                    assert_eq!((w, h), (1024.0, 768.0), "{case}: only 4:3 has no room beside");
                    let corner = Spot {
                        left: NARROW_MARGIN,
                        edge: NARROW_MARGIN,
                        bottom: false,
                        width: NARROW_BELOW,
                        narrow: true,
                    };
                    assert_eq!(s, corner, "{case}");
                }
            }
        }
    }
}
