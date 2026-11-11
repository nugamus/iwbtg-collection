//! The screen: integer scale, fullscreen ratios, windowed presets and the backdrop behind the game
//! (docs/product.md, Look and screen). The game keeps its native size; this only decides how big
//! one game pixel is on the monitor and what fills the rest.
//!
//! There is no settings screen yet (#53), so the choices are flags (`--fullscreen`, `--ratio`,
//! `--window`) and keys: F11 fullscreen, F10 next ratio, F9 next windowed size. Each choice is
//! saved in the settings file (`settings.rs`), so the next start keeps it. `[` and `]` blur the
//! backdrop less and more (not saved yet: the settings file has no blur entry, #509).

use std::sync::OnceLock;

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use iwbtg_format::key_art::{self, KeyArt};
use iwbtg_format::manifest::Manifest;
use iwbtg_sim::def::RoomId;

use crate::art::ArtFiles;
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
    /// The game is scaled by whole steps inside it; the rest is a blurred copy of the background.
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

/// The scale of one game pixel in screen pixels, and whether the picture needs the half-pixel
/// shift that keeps it centred on a whole pixel. `ratio` limits the area the game may use to the
/// largest rectangle of that ratio that fits the window. At least 1 when the window is big
/// enough; a window smaller than the game shrinks it (not crisp, but not cut off).
pub fn layout(window: (u32, u32), ratio: Option<(u32, u32)>, room: (u32, u32)) -> (f32, (bool, bool)) {
    let (ww, wh) = (window.0.max(1) as f32, window.1.max(1) as f32);
    let (sw, sh) = match ratio {
        Some((rw, rh)) if ww * rh as f32 >= wh * rw as f32 => (wh * rw as f32 / rh as f32, wh),
        Some((rw, rh)) => (ww, ww * rh as f32 / rw as f32),
        None => (ww, wh),
    };
    let fit = (sw / room.0 as f32).min(sh / room.1 as f32);
    let scale = if fit >= 1.0 { fit.floor() } else { fit };
    let odd = |win: u32, size: u32| (win as f32 - scale * size as f32).rem_euclid(2.0) > 0.5;
    (scale, (odd(window.0, room.0), odd(window.1, room.1)))
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
) {
    if let Some((w, h)) = start.0.take() {
        window.resolution.set_physical_resolution(w, h);
    }
    // Stand-in for the blur slider of the settings screen (#53).
    if keys.just_pressed(KeyCode::BracketRight) {
        backdrop.blur = (backdrop.blur + 0.1).min(1.0);
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        backdrop.blur = (backdrop.blur - 0.1).max(0.0);
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

/// Sets the window mode and the camera: whole-pixel scale, centred. The game is scaled to fit the
/// view (the whole room when it has none). When the view shows less than the room, or is turned,
/// the camera is clipped to the view's rectangle so nothing of the room shows beside it.
fn apply(
    mut screen: ResMut<Screen>,
    room: Res<RoomSize>,
    view: Res<ViewRect>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut camera: Single<(&mut Projection, &mut Camera), With<GameCamera>>,
    backdrop: Res<Backdrop>,
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
    let (scale, (odd_x, odd_y)) = layout(size, ratio, (view.w as u32, view.h as u32));
    let clip = view.clips(room.0);
    let (projection, cam) = &mut *camera;
    cam.viewport = clip.then(|| {
        let (w, h) = (
            ((view.w * scale).round() as u32).max(1),
            ((view.h * scale).round() as u32).max(1),
        );
        bevy::camera::Viewport {
            physical_position: UVec2::new(size.0.saturating_sub(w) / 2, size.1.saturating_sub(h) / 2),
            physical_size: UVec2::new(w.min(size.0), h.min(size.1)),
            ..default()
        }
    });
    if let Projection::Orthographic(o) = &mut **projection {
        // One world unit is `scale` physical pixels.
        o.scaling_mode = ScalingMode::WindowSize;
        o.scale = window.resolution.scale_factor() / scale;
        // Half a pixel when the leftover is odd, so game pixels sit on screen pixels.
        let shift = |odd: bool, px: u32| if odd { 0.5 / px.max(1) as f32 } else { 0.0 };
        // A clipped camera already sits on whole pixels.
        o.viewport_origin = if clip {
            Vec2::splat(0.5)
        } else {
            Vec2::new(0.5 + shift(odd_x, size.0), 0.5 - shift(odd_y, size.1))
        };
    }
    let base = screen.title.get_or_insert_with(|| window.title.clone()).clone();
    let title = format!(
        "{base} [{}x{} px, x{scale}, {}{}, blur {:.0}%]",
        size.0,
        size.1,
        ratio_name(ratio),
        if screen.fullscreen { ", fullscreen" } else { "" },
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
    backdrop.blur = backdrop::DEFAULT_BLUR;
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

    const ROOM: (u32, u32) = (800, 608);

    #[test]
    fn the_game_scales_by_whole_steps_and_never_past_the_window() {
        assert_eq!(layout((800, 608), None, ROOM).0, 1.0);
        assert_eq!(layout((1600, 1216), None, ROOM).0, 2.0);
        // 1080p: 608 * 1 fits, 608 * 2 does not.
        assert_eq!(layout((1920, 1080), None, ROOM).0, 1.0);
        assert_eq!(layout((2560, 1440), None, ROOM).0, 2.0);
    }

    #[test]
    fn a_ratio_limits_the_area_the_game_may_use() {
        // 32:9 on a 16:9 screen is a strip 1920 x 540, shorter than the game.
        assert!(layout((1920, 1080), Some((32, 9)), ROOM).0 < 1.0);
        // 4:3 on 2560 x 1440 is 1920 x 1440: two game widths do not fit (1600), one does.
        assert_eq!(layout((2560, 1440), Some((4, 3)), ROOM).0, 2.0);
        assert_eq!(layout((3840, 2160), Some((4, 3)), ROOM).0, 3.0);
    }

    #[test]
    fn an_odd_leftover_asks_for_the_half_pixel_shift() {
        assert_eq!(layout((801, 608), None, ROOM).1, (true, false));
        assert_eq!(layout((802, 609), None, ROOM).1, (false, true));
        assert_eq!(layout((800, 608), None, ROOM).1, (false, false));
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
