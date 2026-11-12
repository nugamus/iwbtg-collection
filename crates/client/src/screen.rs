//! The screen: fullscreen ratios, windowed presets, the game camera and the backdrop behind the
//! game (docs/product.md, Look and screen). The game keeps its native size; this only decides how
//! big one game pixel is on the monitor (`hud::layout`, which also places the HUD columns) and what
//! fills the rest.
//!
//! There is no settings screen yet (#53), so the choices are flags (`--fullscreen`, `--ratio`,
//! `--window`) and keys: F11 fullscreen, F10 next ratio, F9 next windowed size. Each choice is
//! saved in the settings file (`settings.rs`), so the next start keeps it. `[` and `]` blur the
//! backdrop less and more by 10% and save it (`display.backdrop_blur`; the settings screen, #53,
//! replaces the keys with a slider).

use std::sync::OnceLock;

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use iwbtg_format::key_art::{self, KeyArt};
use iwbtg_format::manifest::Manifest;
use iwbtg_sim::def::RoomId;

use crate::art::ArtFiles;
use crate::hud::layout;
use crate::hud::{HudLayout, HudOptions};
use crate::package::decode_png;
use crate::settings::{self, Settings};
use crate::ui::backdrop::{self, Backdrop, BackdropPlugin};
use crate::view::{Game, GameCamera, RoomSize, ViewRect};

/// The aspect ratios fullscreen offers; `None` is the monitor's own.
pub const RATIOS: [Option<(u32, u32)>; 6] = [
    None,
    Some((4, 3)),
    Some((16, 9)),
    Some((16, 10)),
    Some((21, 9)),
    Some((32, 9)),
];

/// The windowed sizes F9 cycles through.
pub const WINDOWS: [(u32, u32); 6] = [
    (800, 608),
    (1280, 720),
    (1600, 1216),
    (1920, 1080),
    (2560, 1440),
    (3200, 2432),
];

/// Parses `16:9` (or `native`).
pub fn parse_ratio(s: &str) -> Result<Option<(u32, u32)>, String> {
    if s.eq_ignore_ascii_case("native") {
        return Ok(None);
    }
    let r = s
        .split_once(':')
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .filter(|r| RATIOS.contains(&Some(*r)));
    r.map(Some)
        .ok_or_else(|| "a ratio is native, 4:3, 16:9, 16:10, 21:9 or 32:9".to_string())
}

/// Parses `1600x1216`.
pub fn parse_size(s: &str) -> Result<(u32, u32), String> {
    s.split_once(['x', 'X'])
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .filter(|(w, h)| *w >= 200 && *h >= 200)
        .ok_or_else(|| "a window size is WIDTHxHEIGHT, at least 200 x 200".to_string())
}

/// The screen's command-line flags.
#[derive(clap::Args, Debug)]
pub struct ScreenArgs {
    /// Start fullscreen (F11 toggles). Like `--ratio` and `--window`, saved in the settings file,
    /// so the next start without the flag keeps it.
    #[arg(long)]
    pub fullscreen: bool,
    /// The shape of the area the game may use: native, 4:3, 16:9, 16:10, 21:9 or 32:9 (F10 cycles).
    /// The game and the HUD are laid out inside it; the rest is the blurred backdrop.
    /// Default: the saved choice, else native.
    #[arg(long, value_name = "RATIO")]
    pub ratio: Option<String>,
    /// Windowed size, WIDTHxHEIGHT (F9 cycles the presets: 800x608, 1280x720, 1600x1216, 1920x1080,
    /// 2560x1440, 3200x2432).
    #[arg(long, value_name = "WxH", value_parser = parse_size)]
    pub window: Option<(u32, u32)>,
}

impl ScreenArgs {
    /// Puts the flags given on the command line into the settings; the rest keep their saved value.
    pub fn apply(&self, s: &mut Settings) -> Result<(), String> {
        if let Some(r) = &self.ratio {
            s.ratio = parse_ratio(r).map_err(|e| format!("--ratio {r}: {e}"))?;
        }
        s.fullscreen |= self.fullscreen;
        if self.window.is_some() {
            s.window = self.window;
        }
        Ok(())
    }
}

/// What the command line asked for.
#[derive(Clone, Copy, Default)]
pub struct Options {
    pub fullscreen: bool,
    pub ratio: Option<(u32, u32)>,
    pub window: Option<(u32, u32)>,
}

static OPTIONS: OnceLock<Options> = OnceLock::new();

/// Called by `main` after `settings::load` and before the window opens: saves the flags and
/// starts from the saved choices.
pub fn configure(args: &ScreenArgs) -> Result<(), String> {
    let mut s = settings::get();
    args.apply(&mut s)?;
    settings::update(|saved| *saved = s.clone());
    let _ = OPTIONS.set(Options {
        fullscreen: s.fullscreen,
        ratio: s.ratio,
        window: s.window,
    });
    Ok(())
}

/// The part of the window the game and the HUD may use, in physical px: the largest rectangle
/// of `ratio` that fits, centred (the whole window for `None`).
pub fn area(window: (u32, u32), ratio: Option<(u32, u32)>) -> Rect {
    let (ww, wh) = (window.0.max(1) as f32, window.1.max(1) as f32);
    let (sw, sh) = match ratio {
        Some((rw, rh)) if ww * rh as f32 >= wh * rw as f32 => (wh * rw as f32 / rh as f32, wh),
        Some((rw, rh)) => (ww, ww * rh as f32 / rw as f32),
        None => (ww, wh),
    };
    let min = Vec2::new(((ww - sw) / 2.0).floor(), ((wh - sh) / 2.0).floor());
    Rect::from_corners(min, min + Vec2::new(sw.floor(), sh.floor()))
}

/// The current choices.
#[derive(Resource)]
pub struct Screen {
    fullscreen: bool,
    ratio: usize,
    window: usize,
    title: Option<String>,
}

/// The black panel under the game and the room it is sized for.
#[derive(Resource, Default)]
struct Letterbox {
    room: Option<RoomId>,
    panel: Option<Entity>,
}

pub struct ScreenPlugin;

impl Plugin for ScreenPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<BackdropPlugin>() {
            app.add_plugins(BackdropPlugin);
        }
        let o = OPTIONS.get().copied().unwrap_or_default();
        app.insert_resource(Screen {
            fullscreen: o.fullscreen,
            ratio: RATIOS.iter().position(|r| *r == o.ratio).unwrap_or(0),
            window: o
                .window
                .and_then(|w| WINDOWS.iter().position(|p| *p == w))
                .unwrap_or(0),
            title: None,
        })
        .insert_resource(StartSize(o.window))
        .init_resource::<Letterbox>()
        .init_resource::<HudOptions>()
        .init_resource::<HudLayout>()
        .add_systems(
            Update,
            (keys, apply, room_panel, keep_panel, game_backdrop)
                .chain()
                .after(crate::view::follow_view),
        );
    }
}

/// A window size from `--window` that is not one of the presets.
#[derive(Resource)]
struct StartSize(Option<(u32, u32)>);

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut screen: ResMut<Screen>,
    mut start: ResMut<StartSize>,
    mut backdrop: ResMut<Backdrop>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    shot: Option<Res<crate::view::ScreenshotTo>>,
) {
    if let Some((w, h)) = start.0.take() {
        // A `--screenshot` check lays out at one physical px per logical px, so the picture is
        // the same on every machine whatever its display scaling (like the gallery).
        if shot.is_some() {
            window.resolution.set_scale_factor_override(Some(1.0));
        }
        window.resolution.set_physical_resolution(w, h);
    }
    // Stand-ins for the blur slider of the settings screen (#53), which replaces them.
    let step = i32::from(keys.just_pressed(KeyCode::BracketRight))
        - i32::from(keys.just_pressed(KeyCode::BracketLeft));
    if step != 0 {
        let pct = settings::get()
            .backdrop_blur
            .saturating_add_signed(step * 10)
            .min(settings::MAX_BACKDROP_BLUR);
        settings::update(|s| s.backdrop_blur = pct);
        backdrop.blur = pct as f32 / 100.0;
    }
    if keys.just_pressed(KeyCode::F11) {
        screen.fullscreen = !screen.fullscreen;
        settings::update(|s| s.fullscreen = screen.fullscreen);
    }
    if keys.just_pressed(KeyCode::F10) {
        screen.ratio = (screen.ratio + 1) % RATIOS.len();
        settings::update(|s| s.ratio = RATIOS[screen.ratio]);
    }
    if keys.just_pressed(KeyCode::F9) && !screen.fullscreen {
        screen.window = (screen.window + 1) % WINDOWS.len();
        let (w, h) = WINDOWS[screen.window];
        window.resolution.set_physical_resolution(w, h);
        settings::update(|s| s.window = Some((w, h)));
    }
}

fn ratio_name(r: Option<(u32, u32)>) -> String {
    r.map_or("native".to_string(), |(w, h)| format!("{w}:{h}"))
}

/// Sets the window mode, lays out the game and the HUD columns (`hud::layout`) and puts the game
/// camera on the game's rectangle: the camera is clipped to it (`Camera.viewport`) and scaled so
/// the view fills it, so nothing of the room shows beside the view.
#[allow(
    clippy::too_many_arguments,
    reason = "one Bevy system reading what the layout needs"
)]
pub(crate) fn apply(
    mut screen: ResMut<Screen>,
    view: Res<ViewRect>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut camera: Single<(&mut Projection, &mut Camera), With<GameCamera>>,
    backdrop: Res<Backdrop>,
    hud: Res<HudOptions>,
    mut placed: ResMut<HudLayout>,
) {
    let mode = if screen.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if window.mode != mode {
        window.mode = mode;
    }
    let size = (
        window.resolution.physical_width(),
        window.resolution.physical_height(),
    );
    let ratio = RATIOS[screen.ratio];
    let dpi = window.resolution.scale_factor();
    let l = layout::compute(area(size, ratio), dpi, Vec2::new(view.w, view.h), hud.0);
    if placed.0 != l {
        placed.0 = l;
    }
    let (projection, cam) = &mut *camera;
    let (pos, along) = (l.game.min.max(Vec2::ZERO), l.game.size().max(Vec2::ONE));
    let want = bevy::camera::Viewport {
        physical_position: UVec2::new(pos.x as u32, pos.y as u32)
            .min(UVec2::new(size.0, size.1).saturating_sub(UVec2::ONE)),
        physical_size: UVec2::new(
            (along.x as u32).min(size.0.saturating_sub(pos.x as u32)).max(1),
            (along.y as u32).min(size.1.saturating_sub(pos.y as u32)).max(1),
        ),
        ..default()
    };
    if cam
        .viewport
        .as_ref()
        .map(|v| (v.physical_position, v.physical_size))
        != Some((want.physical_position, want.physical_size))
    {
        cam.viewport = Some(want);
    }
    if let Projection::Orthographic(o) = &mut **projection {
        // One world unit is `scale` physical pixels.
        o.scaling_mode = ScalingMode::WindowSize;
        o.scale = dpi / l.scale;
        o.viewport_origin = Vec2::splat(0.5);
    }
    let base = screen.title.get_or_insert_with(|| window.title.clone()).clone();
    let title = format!(
        "{base} [{}x{} px, x{:.2}{}, {}{}, UI {:.0}%, blur {:.0}%]",
        size.0,
        size.1,
        l.scale,
        if hud.0.integer { " integer" } else { "" },
        ratio_name(ratio),
        if screen.fullscreen { ", fullscreen" } else { "" },
        hud.0.ui_scale * 100.0,
        backdrop.blur * 100.0
    );
    if window.title != title {
        window.title = title;
    }
}

/// Sets the background once: the game's key art (`[key_art]`, else the first ranked room's
/// background, else its colour, else the client's own), blurred by the player's setting and
/// dimmed for gameplay. The backdrop system draws it over the whole window, so the game never
/// has black bars (docs/product.md, Look and screen).
fn game_backdrop(
    files: Res<ArtFiles>,
    room: Res<RoomSize>,
    mut images: ResMut<Assets<Image>>,
    mut backdrop: ResMut<Backdrop>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    // The room size comes from the sim's first tick; until then there is nothing to size by.
    if room.0.x < 1.0 {
        return;
    }
    *done = true;
    let art = files
        .0
        .get("game.toml")
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|t| Manifest::parse(t).ok())
        .map_or(KeyArt::None, |m| key_art::resolve(&files.0, &m, None));
    let image = backdrop::key_art_image(&files.0, &art, (room.0.x as u32, room.0.y as u32), |bytes| {
        decode_png(bytes).map(|s| (s.rgba, s.size.0, s.size.1))
    });
    backdrop.art = Some(images.add(image));
    backdrop.dim = backdrop::dim::GAMEPLAY;
    backdrop.blur = settings::get().backdrop_blur as f32 / 100.0;
}

/// Spawns a black panel the size of the room under the game, so the backdrop never shows
/// through a room with no art of its own.
fn room_panel(mut commands: Commands, game: NonSend<Game>, mut state: ResMut<Letterbox>) {
    let room = game.sim.world().room;
    let Some(def) = game.sim.def().room(room) else {
        return;
    };
    if state.room != Some(room) {
        state.room = Some(room);
        if let Some(e) = state.panel.take() {
            commands.entity(e).despawn();
        }
        let room_size = Vec2::new(def.width as f32, def.height as f32);
        state.panel = Some(
            commands
                .spawn((
                    Sprite::from_color(Color::BLACK, room_size),
                    Transform::from_xyz(0.0, 0.0, -998.0),
                ))
                .id(),
        );
    }
}

/// Keeps the panel at the view's size, with the camera.
fn keep_panel(
    state: Res<Letterbox>,
    camera: Single<&Transform, With<GameCamera>>,
    view: Res<ViewRect>,
    mut sprites: Query<(&mut Sprite, &mut Transform), Without<GameCamera>>,
) {
    if let Some(e) = state.panel
        && let Ok((mut s, mut t)) = sprites.get_mut(e)
    {
        s.custom_size = Some(Vec2::new(view.w, view.h));
        t.translation.x = camera.translation.x;
        t.translation.y = camera.translation.y;
        t.rotation = camera.rotation;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ratio_limits_the_area_to_its_largest_centred_rectangle() {
        assert_eq!(area((1920, 1080), None), Rect::new(0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(
            area((1920, 1080), Some((4, 3))),
            Rect::new(240.0, 0.0, 1680.0, 1080.0)
        );
        assert_eq!(
            area((1920, 1080), Some((32, 9))),
            Rect::new(0.0, 270.0, 1920.0, 810.0)
        );
    }

    #[test]
    fn flags_override_the_saved_choices_and_the_rest_stay() {
        let saved = Settings {
            ratio: Some((16, 9)),
            window: Some((1280, 720)),
            ..Settings::default()
        };
        let none = ScreenArgs {
            fullscreen: false,
            ratio: None,
            window: None,
        };
        let mut s = saved.clone();
        none.apply(&mut s).unwrap();
        assert_eq!(s, saved, "no flag changes nothing");
        let some = ScreenArgs {
            fullscreen: true,
            ratio: Some("native".into()),
            window: Some((1600, 1216)),
        };
        some.apply(&mut s).unwrap();
        assert!(s.fullscreen);
        assert_eq!(s.ratio, None);
        assert_eq!(s.window, Some((1600, 1216)));
        let bad = ScreenArgs {
            ratio: Some("5:4".into()),
            ..none
        };
        assert!(bad.apply(&mut s).is_err());
    }

    #[test]
    fn ratios_and_sizes_parse() {
        assert_eq!(parse_ratio("16:9"), Ok(Some((16, 9))));
        assert_eq!(parse_ratio("Native"), Ok(None));
        assert!(parse_ratio("5:4").is_err());
        assert_eq!(parse_size("1600x1216"), Ok((1600, 1216)));
        assert!(parse_size("10x10").is_err());
    }
}
