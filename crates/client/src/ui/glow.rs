//! Glow: a soft blurred light under an icon or a line of text, the mockups'
//! `filter: drop-shadow(0 0 6px ...)` on an icon and `text-shadow: 0 0 8px ...` on a word
//! (`RunInfo.dc.html`, the held keys of the input display).
//!
//! [`glowing`] wraps an icon ([`icon`](super::icons::icon)) or a text node in a node that
//! carries [`Glow`]. `sync_glows` (PostUpdate, after layout) gives that node a `GlowLayer` child
//! under the content, grown past the box by three blur deviations each side, and draws into the
//! layer's image the content's own shape, blurred: the icon's paths (tiny-skia) or the text's
//! glyphs as the font atlas holds them, at physical pixels, so it works the same on native and
//! wasm32 with no shader. The image is redrawn only when the content's size, glyphs or scale
//! change. Lighting and dimming the glow is a colour change ([`Glow::color`], `Color::NONE` for
//! off), so a display that changes every frame (the HUD's keys) costs no rasterising. The layer's
//! colour goes through `Ink`, so `UiOpacity` and `FadeMask` apply to it as to the content.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::text::TextLayoutInfo;
use bevy::ui::UiSystems;
use tiny_skia::Pixmap;

use super::icons::{self, Icon, UiIcon};
use super::shape::Ink;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<GlowImages>().add_systems(
        PostUpdate,
        sync_glows
            .after(UiSystems::PostLayout)
            .before(super::shape::sync_shapes),
    );
}

/// A soft glow under the node's icon or text child: CSS `0 0 <blur> <color>` (the blur radius as
/// CSS gives it; the deviation is half of it). Set `color` to `Color::NONE` to turn it off.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    pub color: Color,
    pub blur: f32,
}

impl Glow {
    pub fn new(blur: f32, color: Color) -> Glow {
        Glow { color, blur }
    }

    /// How far the layer reaches past the box, logical px: three deviations.
    fn reach(self) -> f32 {
        (1.5 * self.blur).ceil()
    }
}

/// `content` (an icon or a text node) with `glow` under it. The wrapper takes the content's size;
/// put a rotation (`UiTransform`) on the wrapper so the glow turns with it.
pub fn glowing(glow: Glow, content: impl Bundle) -> impl Bundle {
    (
        Node {
            flex_shrink: 0.0,
            ..default()
        },
        glow,
        children![content],
    )
}

/// The child that draws a [`Glow`]; what it last drew.
#[derive(Component, Default)]
struct GlowLayer {
    drawn: Option<u64>,
}

/// Glow images of icons by everything that shapes them (one image serves every node alike).
#[derive(Resource, Default)]
struct GlowImages(HashMap<u64, Handle<Image>>);

/// Adds missing glow layers, keeps their reach and colour, and redraws their image when the
/// content changed.
#[allow(clippy::type_complexity, reason = "one pass over glows and their parts")]
fn sync_glows(
    mut commands: Commands,
    glows: Query<(Entity, Ref<Glow>, &Children)>,
    mut layers: Query<(
        &mut GlowLayer,
        &mut Node,
        &mut Ink,
        &mut ImageNode,
        &ComputedNode,
        &UiGlobalTransform,
    )>,
    sources: Query<(
        &ComputedNode,
        &UiGlobalTransform,
        Option<&UiIcon>,
        Option<&TextLayoutInfo>,
    )>,
    mut cache: ResMut<GlowImages>,
    mut images: ResMut<Assets<Image>>,
) {
    for (entity, glow, children) in &glows {
        let Some(layer) = children.iter().find(|c| layers.contains(*c)) else {
            commands.spawn((
                GlowLayer::default(),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ImageNode::default(),
                Ink(glow.color),
                Pickable::IGNORE,
                // Under the content, never over it.
                ZIndex(-1),
                ChildOf(entity),
            ));
            continue;
        };
        let Some(source) = children.iter().find(|c| sources.contains(*c) && *c != layer) else {
            continue;
        };
        let Ok((mut drawn, mut node, mut ink, mut image, at, layer_tf)) = layers.get_mut(layer) else {
            continue;
        };
        if glow.is_changed() && ink.0 != glow.color {
            ink.0 = glow.color;
        }
        let reach = Val::Px(-glow.reach());
        if node.left != reach {
            node.left = reach;
            node.right = reach;
            node.top = reach;
            node.bottom = reach;
        }
        let Ok((src, src_tf, kind, text)) = sources.get(source) else {
            continue;
        };
        let size = at.size().round().as_uvec2();
        if size.x == 0 || size.y == 0 || src.size().x == 0.0 {
            continue;
        }
        // Where the content's box starts in the layer's image, in physical px.
        let content_min = src_tf.transform_point2(src.content_box().min);
        let origin = layer_tf.inverse().transform_point2(content_min) + at.size() / 2.0;
        let sigma = glow.blur / 2.0 / at.inverse_scale_factor();
        let mut key = std::collections::hash_map::DefaultHasher::new();
        (size, origin.round().as_ivec2(), sigma.to_bits()).hash(&mut key);
        let mask = match (kind, text) {
            (Some(UiIcon(icon)), _) => {
                let box_size = src.content_box().size().round();
                (icon, box_size.as_uvec2()).hash(&mut key);
                let key = key.finish();
                if drawn.drawn == Some(key) {
                    continue;
                }
                let handle = match cache.0.get(&key) {
                    Some(h) => h.clone(),
                    None => {
                        let Some(alpha) = icon_mask(*icon, size, origin.round(), box_size) else {
                            continue;
                        };
                        let h = images.add(icons::white_image(size.x, size.y, &blur(&alpha, size, sigma)));
                        cache.0.insert(key, h.clone());
                        h
                    }
                };
                Some((key, handle))
            }
            (None, Some(text)) => {
                for g in &text.glyphs {
                    (
                        g.position.to_array().map(f32::to_bits),
                        g.atlas_info.texture,
                        g.atlas_info.rect.min.to_array().map(f32::to_bits),
                    )
                        .hash(&mut key);
                }
                let key = key.finish();
                if drawn.drawn == Some(key) {
                    continue;
                }
                let alpha = text_mask(text, size, origin.round(), &images);
                Some((
                    key,
                    images.add(icons::white_image(size.x, size.y, &blur(&alpha, size, sigma))),
                ))
            }
            _ => None,
        };
        if let Some((key, handle)) = mask {
            drawn.drawn = Some(key);
            image.image = handle;
        }
    }
}

/// The icon's coverage (0 to 1) in a `size` image, its box at `at` of `box_size` px.
fn icon_mask(icon: Icon, size: UVec2, at: Vec2, box_size: Vec2) -> Option<Vec<f32>> {
    let mut pixmap = Pixmap::new(size.x, size.y)?;
    icons::draw(icon, &mut pixmap, at, box_size)?;
    Some(
        pixmap
            .pixels()
            .iter()
            .map(|p| f32::from(p.alpha()) / 255.0)
            .collect(),
    )
}

/// The text's glyph coverage (0 to 1) in a `size` image, the text's box starting at `at`.
fn text_mask(text: &TextLayoutInfo, size: UVec2, at: Vec2, images: &Assets<Image>) -> Vec<f32> {
    let (w, h) = (size.x as i32, size.y as i32);
    let mut alpha = vec![0.0f32; (w * h) as usize];
    for g in &text.glyphs {
        let Some(atlas) = images.get(g.atlas_info.texture) else {
            continue;
        };
        let Some(data) = atlas.data.as_ref() else {
            continue;
        };
        let stride = atlas.width() as i32;
        let rect = g.atlas_info.rect;
        let (rw, rh) = (rect.width() as i32, rect.height() as i32);
        // The glyph's position is its centre in the text's box.
        let top_left = (at + g.position - rect.size() / 2.0).round().as_ivec2();
        for y in 0..rh {
            for x in 0..rw {
                let (dx, dy) = (top_left.x + x, top_left.y + y);
                if dx < 0 || dy < 0 || dx >= w || dy >= h {
                    continue;
                }
                let src = ((rect.min.y as i32 + y) * stride + rect.min.x as i32 + x) as usize;
                let Some(&a) = data.get(src * 4 + 3) else {
                    continue;
                };
                let d = &mut alpha[(dy * w + dx) as usize];
                *d = (*d + f32::from(a) / 255.0).min(1.0);
            }
        }
    }
    alpha
}

/// Gaussian blur of a coverage mask (deviation `sigma` px, two passes), as bytes.
fn blur(alpha: &[f32], size: UVec2, sigma: f32) -> Vec<u8> {
    let (w, h) = (size.x as usize, size.y as usize);
    let radius = (3.0 * sigma).ceil().max(0.0) as usize;
    let kernel: Vec<f32> = {
        let k: Vec<f32> = (0..=2 * radius)
            .map(|i| {
                let d = i as f32 - radius as f32;
                if sigma > 0.0 {
                    (-d * d / (2.0 * sigma * sigma)).exp()
                } else {
                    1.0
                }
            })
            .collect();
        let sum: f32 = k.iter().sum();
        k.into_iter().map(|v| v / sum).collect()
    };
    let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
        let mut out = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let mut acc = 0.0;
                for (i, k) in kernel.iter().enumerate() {
                    let o = i as isize - radius as isize;
                    let (sx, sy) = if horizontal {
                        (x as isize + o, y as isize)
                    } else {
                        (x as isize, y as isize + o)
                    };
                    if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h {
                        acc += k * src[sy as usize * w + sx as usize];
                    }
                }
                out[y * w + x] = acc;
            }
        }
        out
    };
    let blurred = pass(&pass(alpha, true), false);
    blurred
        .into_iter()
        .map(|a| (a.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blur_spreads_a_dot_and_keeps_its_weight() {
        let size = UVec2::new(21, 21);
        let mut alpha = vec![0.0; 21 * 21];
        // A 5x5 solid square in the middle.
        for y in 8..13 {
            for x in 8..13 {
                alpha[y * 21 + x] = 1.0;
            }
        }
        let out = blur(&alpha, size, 2.0);
        let at = |x: usize, y: usize| out[y * 21 + x];
        assert!(at(10, 10) < 255 && at(10, 10) > 100, "the centre softens");
        assert!(at(5, 10) > 0, "light reaches past the square");
        assert_eq!(at(0, 0), 0, "but not to the corner");
        assert_eq!(at(10, 4), at(10, 16), "evenly both ways");
        let total: f32 = out.iter().map(|&a| f32::from(a) / 255.0).sum();
        assert!(
            (total - 25.0).abs() < 1.0,
            "the light is spread, not lost: {total}"
        );
    }

    #[test]
    fn the_arrow_glow_reaches_past_the_arrow() {
        let size = UVec2::new(26 + 18, 26 + 18);
        let mask = icon_mask(Icon::Arrow, size, Vec2::splat(9.0), Vec2::splat(26.0)).unwrap();
        let out = blur(&mask, size, 3.0);
        let at = |x: usize, y: usize| out[y * size.x as usize + x];
        // The tip of the arrow is at (13, 4) in its box: the glow shows above it.
        assert!(at(22, 11) > 0, "above the tip");
        assert!(at(22, 22) > at(22, 6), "brightest over the arrow");
        assert_eq!(at(0, 43), 0, "and gone at the far corner");
    }

    /// Glyphs are copied from the atlas at their place in the text box (the position is the
    /// glyph's centre).
    #[test]
    fn text_glyphs_come_from_the_atlas() {
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
        use bevy::text::{GlyphAtlasInfo, PositionedGlyph};
        // A 4x4 atlas whose right half is solid.
        let data: Vec<u8> = (0..16)
            .flat_map(|i| [255, 255, 255, if i % 4 >= 2 { 255 } else { 0 }])
            .collect();
        let atlas = Image::new(
            Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        );
        let mut images = Assets::<Image>::default();
        let texture = images.add(atlas).id();
        let text = TextLayoutInfo {
            glyphs: vec![PositionedGlyph {
                position: Vec2::new(3.0, 1.0),
                atlas_info: GlyphAtlasInfo {
                    texture,
                    rect: Rect::new(2.0, 0.0, 4.0, 2.0),
                    offset: Vec2::ZERO,
                    is_alpha_mask: true,
                },
                section_index: 0,
                line_index: 0,
            }],
            ..default()
        };
        let mask = text_mask(&text, UVec2::new(8, 8), Vec2::new(2.0, 2.0), &images);
        let lit: Vec<(usize, usize)> = (0..64)
            .filter(|i| mask[*i] > 0.5)
            .map(|i| (i % 8, i / 8))
            .collect();
        assert_eq!(lit, vec![(4, 2), (5, 2), (4, 3), (5, 3)]);
    }
}
