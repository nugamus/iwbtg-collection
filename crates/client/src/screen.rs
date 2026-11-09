//! The screen: integer scale, fullscreen ratios, windowed presets and the blurred letterbox
//! (docs/product.md, Look and screen). The game keeps its native size; this only decides how big
//! one game pixel is on the monitor and what fills the rest.
//!
//! There is no settings screen yet (#53), so the choices are flags (`--fullscreen`, `--ratio`,
//! `--window`) and keys: F11 fullscreen, F10 next ratio, F9 next windowed size. Each choice is
//! saved in the settings file (`settings.rs`), so the next start keeps it.

use std::sync::OnceLock;

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use iwbtg_sim::def::RoomId;

use crate::art::{ArtFiles, parse_room_art};
use crate::package::decode_png;
use crate::settings::{self, Settings};
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

/// The blurred letterbox sprite and the room it shows the background of.
#[derive(Resource, Default)]
struct Letterbox {
    room: Option<RoomId>,
    sprite: Option<Entity>,
    panel: Option<Entity>,
}

pub struct ScreenPlugin;

impl Plugin for ScreenPlugin {
    fn build(&self, app: &mut App) {
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
            (keys, apply, letterbox, keep_letterbox)
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
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    if let Some((w, h)) = start.0.take() {
        window.resolution.set_physical_resolution(w, h);
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
        "{base} [{}x{} px, x{scale}, {}{}]",
        size.0,
        size.1,
        ratio_name(ratio),
        if screen.fullscreen { ", fullscreen" } else { "" }
    );
    if window.title != title {
        window.title = title;
    }
}

/// A box blur on RGBA8, `radius` pixels, run `passes` times (three approximate a Gaussian).
fn blur(rgba: &[u8], w: usize, h: usize, radius: usize, passes: usize) -> Vec<u8> {
    let mut cur = rgba.to_vec();
    let mut tmp = vec![0u8; cur.len()];
    for _ in 0..passes {
        for horizontal in [true, false] {
            let (len, lines) = if horizontal { (w, h) } else { (h, w) };
            for line in 0..lines {
                let at = |i: usize| {
                    let i = i.min(len - 1);
                    if horizontal {
                        (line * w + i) * 4
                    } else {
                        (i * w + line) * 4
                    }
                };
                for i in 0..len {
                    let (lo, hi) = (i.saturating_sub(radius), (i + radius).min(len - 1));
                    for c in 0..4 {
                        let sum: u32 = (lo..=hi).map(|j| u32::from(cur[at(j) + c])).sum();
                        tmp[at(i) + c] = (sum / (hi - lo + 1) as u32) as u8;
                    }
                }
            }
            std::mem::swap(&mut cur, &mut tmp);
        }
    }
    cur
}

/// Downscales to at most `max` pixels wide by averaging, so the blur is cheap and wide.
fn shrink(rgba: &[u8], w: usize, h: usize, max: usize) -> (Vec<u8>, usize, usize) {
    let f = w.div_ceil(max).max(1);
    let (nw, nh) = (w.div_ceil(f), h.div_ceil(f));
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        for x in 0..nw {
            let mut sum = [0u32; 4];
            let mut n = 0;
            for sy in y * f..((y + 1) * f).min(h) {
                for sx in x * f..((x + 1) * f).min(w) {
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += u32::from(rgba[(sy * w + sx) * 4 + c]);
                    }
                    n += 1;
                }
            }
            for c in 0..4 {
                out[(y * nw + x) * 4 + c] = (sum[c] / n.max(1)) as u8;
            }
        }
    }
    (out, nw, nh)
}

/// A dim, blurred copy of the room's background, or a flat colour when it has none.
fn letterbox_image(files: &iwbtg_format::files::PackageFiles, room: &str) -> Image {
    let text = |p: String| {
        files
            .get(&p)
            .and_then(|b| std::str::from_utf8(b).ok().map(String::from))
    };
    let art = parse_room_art(&text(format!("rooms/{room}.toml")).unwrap_or_default(), None);
    let decoded = art
        .background
        .as_deref()
        .and_then(|id| files.get(&format!("backgrounds/{id}.png")))
        .and_then(decode_png);
    let (rgba, w, h) = match decoded {
        Some(s) => {
            let (small, w, h) = shrink(&s.rgba, s.size.0 as usize, s.size.1 as usize, 48);
            (blur(&small, w, h, 3, 3), w, h)
        }
        None => {
            let [r, g, b] = art.color.unwrap_or([0.06, 0.06, 0.1]);
            let px = [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, 255];
            (px.repeat(4), 2, 2)
        }
    };
    // Dimmed, so the game stays the brightest thing on screen.
    let rgba = rgba
        .chunks(4)
        .flat_map(|p| [p[0] / 2, p[1] / 2, p[2] / 2, 255])
        .collect();
    let mut image = Image::new(
        bevy::render::render_resource::Extent3d {
            width: w as u32,
            height: h as u32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        rgba,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    // Smooth when stretched: the blur is the point.
    image.sampler = bevy::image::ImageSampler::linear();
    image
}

/// Fills everything outside the game with the blurred background, and keeps a black panel the
/// size of the view above it, so the blur never shows through a room with no art. Both follow
/// the camera.
fn letterbox(
    mut commands: Commands,
    game: NonSend<Game>,
    files: Res<ArtFiles>,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<Letterbox>,
) {
    let room = game.sim.world().room;
    let Some(def) = game.sim.def().room(room) else {
        return;
    };
    let room_size = Vec2::new(def.width as f32, def.height as f32);
    if state.room != Some(room) {
        state.room = Some(room);
        for e in [state.sprite.take(), state.panel.take()].into_iter().flatten() {
            commands.entity(e).despawn();
        }
        let handle = images.add(letterbox_image(&files.0, &def.name));
        state.sprite = Some(
            commands
                .spawn((Sprite::from_image(handle), Transform::from_xyz(0.0, 0.0, -999.0)))
                .id(),
        );
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

/// Keeps the letterbox sprite over the whole visible area, whatever its shape, and the panel at
/// the view's size, both with the camera.
fn keep_letterbox(
    state: Res<Letterbox>,
    camera: Single<(&Projection, &Transform), With<GameCamera>>,
    view: Res<ViewRect>,
    mut sprites: Query<(&mut Sprite, &mut Transform), Without<GameCamera>>,
) {
    let (projection, camera) = *camera;
    let Projection::Orthographic(o) = projection else {
        return;
    };
    let area = o.area.size().max(Vec2::new(view.w, view.h)) * 1.05;
    for (entity, size) in [(state.sprite, area), (state.panel, Vec2::new(view.w, view.h))] {
        if let Some(e) = entity
            && let Ok((mut s, mut t)) = sprites.get_mut(e)
        {
            s.custom_size = Some(size);
            t.translation.x = camera.translation.x;
            t.translation.y = camera.translation.y;
            t.rotation = camera.rotation;
        }
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

    #[test]
    fn the_blur_spreads_a_bright_pixel_and_keeps_the_average() {
        let mut img = vec![0u8; 9 * 9 * 4];
        for p in img.chunks_mut(4) {
            p[3] = 255;
        }
        let c = (4 * 9 + 4) * 4;
        img[c] = 255;
        let out = blur(&img, 9, 9, 1, 1);
        assert!(out[c] < 255 && out[c] > 0);
        assert!(out[(4 * 9 + 3) * 4] > 0);
        assert_eq!(out[3], 255);
    }
}
