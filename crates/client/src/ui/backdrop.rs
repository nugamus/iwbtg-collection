//! The one background system behind every screen (select, loading, gameplay, pause, results;
//! docs/product.md, Look and screen; artboard `Backdrop.dc.html`).
//!
//! Screens never draw a background of their own. They set the [`Backdrop`] resource: the art
//! (a handle from [`art_image`]), the dim for that screen ([`dim`]) and the player's blur. The art
//! covers the whole window, so there are no black bars at any ratio; the game is drawn over it.
//!
//! How it is drawn: a camera of its own (order -100) renders one full-window UI node per layer
//! with [`BackdropMaterial`] (`backdrop.wgsl`). The art carries its mip chain and the shader blurs
//! by sampling the level that matches the blur radius, so the slider is smooth and costs nine
//! texture reads whatever the radius. Changing the art cross-fades (OutQuint, 400 ms; at once with
//! reduced motion). Cameras that draw over the backdrop must not clear (`ClearColorConfig::None`).

use bevy::asset::{RenderAssetUsages, uuid_handle};
use bevy::camera::visibility::RenderLayers;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::ui::UiTargetCamera;
use bevy::ui_render::prelude::{MaterialNode, UiMaterial, UiMaterialPlugin};
use iwbtg_format::files::PackageFiles;
use iwbtg_format::key_art::{KeyArt, RoomLayer};

use super::motion::{Ease, ReducedMotion};
use super::tokens;
use crate::art::{Segment, axis};

const SHADER: Handle<Shader> = uuid_handle!("2d7e4c1a-6b0f-4a39-8e52-9c1d3f7a5b66");

/// Dim per screen (`Backdrop.dc.html`, the mockups): how much of #05060a covers the art.
pub mod dim {
    pub const SELECT: f32 = 0.38;
    pub const LOADING: f32 = 0.55;
    pub const GAMEPLAY: f32 = 0.45;
    pub const RESULTS: f32 = 0.62;
}

/// The blur the player starts with in the game (0 to 1; the setting itself lives in the settings
/// file once #509 adds it).
pub const DEFAULT_BLUR: f32 = 0.6;

/// How long the art takes to cross-fade, seconds.
const FADE: f32 = 0.4;

/// Adds the material, the camera and the systems. `UiKitPlugin` adds it; the game screen adds it
/// itself when the kit is not loaded.
pub struct BackdropPlugin;

impl Plugin for BackdropPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<Assets<Shader>>()
            .insert(
                &SHADER,
                Shader::from_wgsl(include_str!("backdrop.wgsl"), "iwbtg_client/ui/backdrop.wgsl"),
            )
            .expect("the backdrop shader handle is free");
        app.add_plugins(UiMaterialPlugin::<BackdropMaterial>::default())
            .init_resource::<Backdrop>()
            .init_resource::<ReducedMotion>()
            .add_systems(Startup, spawn)
            .add_systems(PostUpdate, drive);
    }
}

/// Marks the backdrop's own camera, so a query for the game's camera can leave it out.
#[derive(Component)]
pub struct BackdropCamera;

/// What the background shows. Screens set these; `drive` does the rest.
#[derive(Resource, Clone)]
pub struct Backdrop {
    /// The art, from [`art_image`] or [`fallback_image`]. `None` shows the fallback.
    pub art: Option<Handle<Image>>,
    /// 0 (art kept sharp) to 1 (the mockups' 40 px blur at 1600 px wide).
    pub blur: f32,
    /// 0 to 1, from [`dim`].
    pub dim: f32,
}

impl Default for Backdrop {
    fn default() -> Self {
        Backdrop {
            art: None,
            blur: DEFAULT_BLUR,
            dim: dim::SELECT,
        }
    }
}

/// The material behind one backdrop layer.
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub struct BackdropMaterial {
    #[uniform(0)]
    u: BackdropUniform,
    #[texture(1)]
    #[sampler(2)]
    art: Option<Handle<Image>>,
}

#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct BackdropUniform {
    /// x blur, y dim, z opacity, w highest mip level.
    p: Vec4,
}

impl UiMaterial for BackdropMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SHADER)
    }
}

#[derive(Resource)]
/// The two stacked layers (the one underneath keeps the previous art during a fade) and the art
/// shown when a screen sets none.
struct Layers {
    back: Handle<BackdropMaterial>,
    front: Handle<BackdropMaterial>,
    fallback: Handle<Image>,
}

/// The state of the cross-fade and what each layer shows.
#[derive(Resource, Default)]
struct Fade {
    shown: Option<Handle<Image>>,
    previous: Option<Handle<Image>>,
    /// Seconds into the fade; `None` when finished.
    t: Option<f32>,
}

fn spawn(
    mut commands: Commands,
    mut materials: ResMut<Assets<BackdropMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let camera = commands
        .spawn((
            Camera2d,
            BackdropCamera,
            Camera {
                order: -100,
                clear_color: ClearColorConfig::Custom(tokens::GROUND),
                ..default()
            },
            // Draws nothing but the backdrop's UI.
            RenderLayers::none(),
        ))
        .id();
    let art = images.add(fallback_image());
    let make = |opacity: f32| BackdropMaterial {
        u: BackdropUniform {
            p: Vec4::new(0.0, 0.0, opacity, 0.0),
        },
        art: Some(art.clone()),
    };
    let back = materials.add(make(1.0));
    let front = materials.add(make(1.0));
    for handle in [back.clone(), front.clone()] {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            MaterialNode(handle),
            UiTargetCamera(camera),
        ));
    }
    commands.insert_resource(Layers {
        back,
        front,
        fallback: art,
    });
    commands.insert_resource(Fade::default());
}

/// Moves the art, blur and dim from [`Backdrop`] into the layers and runs the cross-fade.
fn drive(
    backdrop: Res<Backdrop>,
    layers: Option<Res<Layers>>,
    mut fade: Option<ResMut<Fade>>,
    mut materials: ResMut<Assets<BackdropMaterial>>,
    images: Res<Assets<Image>>,
    reduced: Res<ReducedMotion>,
    time: Res<Time>,
) {
    let (Some(layers), Some(fade)) = (layers, fade.as_deref_mut()) else {
        return;
    };
    let wanted = match &backdrop.art {
        Some(h) => h.clone(),
        None => layers.fallback.clone(),
    };
    if fade.shown.as_ref() != Some(&wanted) {
        fade.previous = fade.shown.take();
        fade.shown = Some(wanted);
        // The first art appears at once; later changes cross-fade.
        fade.t = if fade.previous.is_some() && !reduced.0 {
            Some(0.0)
        } else {
            None
        };
    }
    if let Some(t) = fade.t {
        let t = t + time.delta_secs();
        fade.t = (t < FADE).then_some(t);
    }
    let opacity = fade.t.map_or(1.0, |t| Ease::OutQuint.at(t / FADE));
    let levels = |h: &Option<Handle<Image>>| {
        h.as_ref()
            .and_then(|h| images.get(h))
            .map_or(0.0, |i| (i.texture_descriptor.mip_level_count.max(1) - 1) as f32)
    };
    let front_levels = levels(&fade.shown);
    let back_levels = levels(&fade.previous.clone().or(fade.shown.clone()));
    let back_art = fade.previous.clone().or(fade.shown.clone());
    for (handle, art, opacity, top) in [
        (&layers.back, back_art, 1.0, back_levels),
        (&layers.front, fade.shown.clone(), opacity, front_levels),
    ] {
        let Some(mut mat) = materials.get_mut(handle) else {
            continue;
        };
        let u = BackdropUniform {
            p: Vec4::new(
                backdrop.blur.clamp(0.0, 1.0),
                backdrop.dim.clamp(0.0, 1.0),
                opacity,
                top,
            ),
        };
        if mat.u != u {
            mat.u = u;
        }
        if mat.art != art {
            mat.art = art;
        }
    }
}

/// An image with its whole mip chain, filtered smoothly, for the backdrop shader.
pub fn art_image(rgba: &[u8], w: u32, h: u32) -> Image {
    let (data, levels) = mip_chain(rgba, w, h);
    let mut image = Image::new_fill(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::linear();
    image
}

/// The mip chain of an RGBA8 image, level 0 first, each level a 2x2 average of the one before
/// (sizes `max(1, size >> level)`, as the GPU counts them), and the number of levels.
pub fn mip_chain(rgba: &[u8], w: u32, h: u32) -> (Vec<u8>, u32) {
    let (mut cw, mut ch) = (w.max(1) as usize, h.max(1) as usize);
    let mut level = rgba.to_vec();
    let mut all = level.clone();
    let mut levels = 1;
    while cw > 1 || ch > 1 {
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                let (x0, y0) = ((x * 2).min(cw - 1), (y * 2).min(ch - 1));
                let (x1, y1) = ((x * 2 + 1).min(cw - 1), (y * 2 + 1).min(ch - 1));
                for c in 0..4 {
                    let at = |xx: usize, yy: usize| u32::from(level[(yy * cw + xx) * 4 + c]);
                    next[(y * nw + x) * 4 + c] =
                        ((at(x0, y0) + at(x1, y0) + at(x0, y1) + at(x1, y1) + 2) / 4) as u8;
                }
            }
        }
        all.extend_from_slice(&next);
        level = next;
        (cw, ch) = (nw, nh);
        levels += 1;
    }
    (all, levels)
}

/// A flat colour as art (a game with no art but a background colour).
pub fn colour_image(rgb: [u8; 3]) -> Image {
    art_image(&[rgb[0], rgb[1], rgb[2], 255].repeat(4), 2, 2)
}

/// The client's own backdrop for a game with no art at all.
pub fn fallback_image() -> Image {
    scene_image(245.0)
}

/// A dusk scene in `hue` (degrees): sky, two ridges and a row of spikes, the one motif every
/// fangame shares. Drawn here, no files; the fallback art and the gallery's stand-in art.
pub fn scene_image(hue: f32) -> Image {
    let (w, h) = (480u32, 270u32);
    let rgb = |s: f32, l: f32| {
        let c = Color::hsl(hue, s, l).to_srgba();
        [
            (c.red * 255.0) as u8,
            (c.green * 255.0) as u8,
            (c.blue * 255.0) as u8,
        ]
    };
    let (far_c, near_c) = (rgb(0.30, 0.11), rgb(0.30, 0.055));
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let fy = y as f32 / h as f32;
        let sky = rgb(0.38, 0.22 - 0.15 * fy);
        for x in 0..w {
            let fx = x as f32 / w as f32;
            let far = 0.62 + 0.05 * (fx * 7.0).sin() + 0.03 * (fx * 19.0).cos();
            let near = 0.80 + 0.04 * (fx * 11.0 + 1.0).sin();
            // Spikes: triangles 24 px wide and 14 px tall standing on the bottom edge.
            let tri = 12.0 - ((x % 24) as f32 - 12.0).abs();
            let spike_top = h as f32 - tri * 14.0 / 12.0;
            let px = if y as f32 >= spike_top && fy > 0.9 {
                [0x05, 0x06, 0x0a]
            } else if fy > near {
                near_c
            } else if fy > far {
                far_c
            } else {
                sky
            };
            rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    art_image(&rgba, w, h)
}

/// The image to show for a game's key art ([`iwbtg_format::key_art::resolve`]): the package's PNG,
/// the first ranked room's background image or colour, or the client's own fallback. A room
/// background is composed over `room` (its size in px) as the room draws its first back layer
/// ([`compose_layer`]), so a small tile is not stretched into a few giant squares.
pub fn key_art_image(
    files: &PackageFiles,
    art: &KeyArt,
    room: (u32, u32),
    decode: impl Fn(&[u8]) -> Option<(Vec<u8>, u32, u32)>,
) -> Image {
    let png = |path: &str| files.get(path).and_then(&decode);
    match art {
        KeyArt::Image(path) => png(path).map_or_else(fallback_image, |(rgba, w, h)| art_image(&rgba, w, h)),
        KeyArt::RoomBackground(layer) => png(&layer.path).map_or_else(fallback_image, |(rgba, w, h)| {
            let (cw, ch) = (room.0.max(1), room.1.max(1));
            art_image(&compose_layer(&rgba, w, h, layer, cw, ch), cw, ch)
        }),
        KeyArt::Colour(rgb) => colour_image(*rgb),
        KeyArt::None => fallback_image(),
    }
}

/// Draws a back layer over a `cw` x `ch` room the way `art.rs` does ([`axis`] cuts each axis):
/// stretched to the room or at its own size, repeated only on the tiled axes, from the layer's
/// offset. Where the layer does not reach, the room colour shows (black without one).
pub fn compose_layer(rgba: &[u8], w: u32, h: u32, layer: &RoomLayer, cw: u32, ch: u32) -> Vec<u8> {
    let (iw, ih) = (w.max(1) as f32, h.max(1) as f32);
    let (rw, rh) = (cw as f32, ch as f32);
    let (cell_x, cell_y) = if layer.stretch { (rw, rh) } else { (iw, ih) };
    let xs = axis(rw, iw, cell_x, layer.offset.0 as f32, layer.tile.0);
    let ys = axis(rh, ih, cell_y, layer.offset.1 as f32, layer.tile.1);
    // The image pixel under room pixel `p` on one axis; `None` outside every segment.
    let lookup = |segs: &[Segment], p: u32, scale: f32, image: f32| {
        let p = p as f32 + 0.5;
        segs.iter().find(|s| p >= s.at && p < s.at + s.len).map(|s| {
            let v = s.from + (p - s.at) / scale;
            let v = if s.repeat { v.rem_euclid(image) } else { v };
            (v as usize).min(image as usize - 1)
        })
    };
    let (sx, sy) = (cell_x / iw, cell_y / ih);
    let [r, g, b] = layer.color.unwrap_or([0; 3]);
    let mut out = Vec::with_capacity(cw as usize * ch as usize * 4);
    for y in 0..ch {
        let iy = lookup(&ys, y, sy, ih);
        for x in 0..cw {
            match (lookup(&xs, x, sx, iw), iy) {
                (Some(ix), Some(iy)) => {
                    let at = (iy * w as usize + ix) * 4;
                    let px = &rgba[at..at + 4];
                    // Over the room colour, like the room draws it.
                    let a = u32::from(px[3]);
                    let mix =
                        |c: u8, bg: u8| ((u32::from(c) * a + u32::from(bg) * (255 - a) + 127) / 255) as u8;
                    out.extend_from_slice(&[mix(px[0], r), mix(px[1], g), mix(px[2], b), 255]);
                }
                _ => out.extend_from_slice(&[r, g, b, 255]),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mip_chain_halves_down_to_one_pixel() {
        let rgba = vec![200u8; 8 * 4 * 4];
        let (data, levels) = mip_chain(&rgba, 8, 4);
        // 8x4, 4x2, 2x1, 1x1.
        assert_eq!(levels, 4);
        assert_eq!(data.len(), (32 + 8 + 2 + 1) * 4);
        assert!(data.iter().all(|&b| b == 200), "an even image stays even");
    }

    #[test]
    fn a_mip_averages_its_four_pixels() {
        // 2x2: black, white / white, black average to mid grey in the 1x1 level.
        let px = |v: u8| [v, v, v, 255];
        let rgba: Vec<u8> = [px(0), px(255), px(255), px(0)].concat();
        let (data, levels) = mip_chain(&rgba, 2, 2);
        assert_eq!(levels, 2);
        assert_eq!(&data[16..20], &[128, 128, 128, 255]);
    }

    #[test]
    fn art_images_carry_every_level() {
        let img = art_image(&vec![0u8; 16 * 16 * 4], 16, 16);
        assert_eq!(img.texture_descriptor.mip_level_count, 5);
    }

    fn layer(stretch: bool, tile: (bool, bool), offset: (i32, i32)) -> RoomLayer {
        RoomLayer {
            path: "backgrounds/a.png".into(),
            stretch,
            tile,
            offset,
            color: Some([9, 9, 9]),
        }
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const FLOOR: [u8; 4] = [9, 9, 9, 255];

    fn row(out: &[u8], cw: u32, y: u32) -> Vec<[u8; 4]> {
        (0..cw)
            .map(|x| {
                let i = ((y * cw + x) * 4) as usize;
                out[i..i + 4].try_into().unwrap()
            })
            .collect()
    }

    #[test]
    fn a_tiled_layer_repeats_from_the_corner() {
        // 2x1: red, blue. Tiled over 3x2: red blue red on both rows.
        let rgba = [RED, BLUE].concat();
        let out = compose_layer(&rgba, 2, 1, &layer(false, (true, true), (0, 0)), 3, 2);
        assert_eq!(out.len(), 3 * 2 * 4);
        assert_eq!(row(&out, 3, 0), [RED, BLUE, RED]);
        assert_eq!(row(&out, 3, 1), [RED, BLUE, RED]);
    }

    #[test]
    fn a_stretched_layer_fills_the_room() {
        // 2x1 stretched to 4x2: left half red, right half blue, no repeat.
        let rgba = [RED, BLUE].concat();
        let out = compose_layer(&rgba, 2, 1, &layer(true, (false, false), (0, 0)), 4, 2);
        assert_eq!(row(&out, 4, 0), [RED, RED, BLUE, BLUE]);
        assert_eq!(row(&out, 4, 1), [RED, RED, BLUE, BLUE]);
    }

    #[test]
    fn a_one_axis_tiled_layer_repeats_only_that_axis() {
        // 2x1 tiled horizontally only, one down, over 4x3: only row 1 is covered.
        let rgba = [RED, BLUE].concat();
        let out = compose_layer(&rgba, 2, 1, &layer(false, (true, false), (0, 1)), 4, 3);
        assert_eq!(row(&out, 4, 0), [FLOOR; 4]);
        assert_eq!(row(&out, 4, 1), [RED, BLUE, RED, BLUE]);
        assert_eq!(row(&out, 4, 2), [FLOOR; 4]);
    }

    #[test]
    fn an_offset_shifts_the_tiling() {
        let rgba = [RED, BLUE].concat();
        let out = compose_layer(&rgba, 2, 1, &layer(false, (true, true), (1, 0)), 4, 1);
        assert_eq!(row(&out, 4, 0), [BLUE, RED, BLUE, RED]);
    }

    #[test]
    fn the_fallback_and_colour_art_build() {
        let f = fallback_image();
        assert_eq!((f.width(), f.height()), (480, 270));
        assert_eq!(colour_image([10, 20, 30]).texture_descriptor.mip_level_count, 2);
    }
}
