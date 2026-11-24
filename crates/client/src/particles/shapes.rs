//! Our own pictures of GM's built-in particle shapes (`pt_shape_*`), drawn in code: white with
//! the shape in the alpha, tinted by the particle's color. research#197, "Particles (GM8
//! runner)", PART-001 confirms 14 built-in sprites, 64x64 except `pt_shape_pixel` (1x1), and
//! `pt_shape_pixel` as the default; the GM8 manual's "15 built-in sprites" is a documentation
//! error (it lists the same 14 constants as the modern manual). The exact pixels of each shape
//! are OPEN (PART-018): GM's images are not in the spec and are not copied, so each picture
//! follows the modern manual's description of the shape, on its own.

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// The side of every shape but `pixel`, in pixels (Confirmed, research#197 PART-001).
pub(crate) const SIZE: u32 = 64;

/// The 14 `pt_shape_*` names the format accepts, in the order of their images (Confirmed,
/// research#197 PART-001; `pixel` is the default).
pub(crate) const NAMES: [&str; 14] = [
    "pixel",
    "disk",
    "square",
    "line",
    "star",
    "circle",
    "ring",
    "sphere",
    "flare",
    "spark",
    "explosion",
    "cloud",
    "smoke",
    "snow",
];

/// The image index of a shape word (`pixel` for an unknown one; `pixel` is the manual's default,
/// research#197 PART-001).
pub(crate) fn index(word: &str) -> usize {
    NAMES.iter().position(|n| *n == word).unwrap_or(0)
}

/// The side of shape `i`: `pt_shape_pixel` is 1x1, every other shape 64x64 (Confirmed,
/// research#197 PART-001).
pub(crate) fn side(i: usize) -> u32 {
    if i == 0 { 1 } else { SIZE }
}

/// A small fixed hash noise in 0..1, for the soft shapes.
fn noise(x: i32, y: i32, salt: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x27d4_eb2d) ^ (y as u32).wrapping_mul(0x1656_67b1) ^ salt;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

/// Blocky value noise: the hash on an 8 px grid, blended.
fn smooth_noise(x: f32, y: f32, salt: u32) -> f32 {
    let (gx, gy) = (x / 8.0, y / 8.0);
    let (ix, iy) = (gx.floor() as i32, gy.floor() as i32);
    let (fx, fy) = (gx - gx.floor(), gy - gy.floor());
    let a = noise(ix, iy, salt) + (noise(ix + 1, iy, salt) - noise(ix, iy, salt)) * fx;
    let b = noise(ix, iy + 1, salt) + (noise(ix + 1, iy + 1, salt) - noise(ix, iy + 1, salt)) * fx;
    a + (b - a) * fy
}

/// The alpha of shape `i` at a pixel, from its offset to the centre (`dx`, `dy` in -1..1). The
/// drawings follow the modern manual's one-line descriptions (disk a filled circle, circle a
/// 3 px outline, ring an inward glow, and so on); the exact pixels are OPEN (research#197
/// PART-018).
fn alpha(i: usize, dx: f32, dy: f32, px: f32, py: f32) -> f32 {
    let r = dx.hypot(dy);
    let a = dy.atan2(dx);
    let edge = |inner: f32, outer: f32| ((outer - r) / (outer - inner)).clamp(0.0, 1.0);
    match NAMES[i] {
        "disk" => edge(0.9, 1.0),
        "square" => 1.0,
        "line" => {
            if dy.abs() < 0.08 {
                1.0
            } else {
                0.0
            }
        }
        "star" => {
            // Five points: the radius swings between 0.4 and 1 around the circle.
            let k = (a * 5.0 / 2.0 + std::f32::consts::FRAC_PI_2 * 5.0 / 2.0)
                .cos()
                .abs();
            let reach = 0.4 + 0.6 * k.powf(6.0);
            if r <= reach { 1.0 } else { 0.0 }
        }
        "circle" => {
            if (0.88..=1.0).contains(&r) {
                1.0
            } else {
                0.0
            }
        }
        "ring" => (1.0 - ((r - 0.75) / 0.25).abs()).clamp(0.0, 1.0),
        "sphere" => (1.0 - r * r).clamp(0.0, 1.0).sqrt(),
        "flare" => {
            let core = (1.0 - r).clamp(0.0, 1.0).powf(4.0);
            let rays = (1.0 - dx.abs().min(dy.abs()) * 12.0).clamp(0.0, 1.0) * (1.0 - r).clamp(0.0, 1.0);
            (core + rays * 0.8).min(1.0)
        }
        "spark" => {
            let thin = (1.0 - dx.abs().min(dy.abs()) * 10.0).clamp(0.0, 1.0);
            (thin * (1.0 - r).clamp(0.0, 1.0) * 1.5).min(1.0)
        }
        "explosion" => {
            let n = smooth_noise(px, py, 11);
            ((1.0 - r) * 1.6 - (1.0 - n) * 0.7).clamp(0.0, 1.0)
        }
        "cloud" => {
            let n = smooth_noise(px, py, 23);
            ((1.0 - r) * (0.6 + 0.6 * n)).clamp(0.0, 1.0)
        }
        "smoke" => {
            let n = smooth_noise(px, py, 37);
            ((1.0 - r) * (0.3 + 0.5 * n)).clamp(0.0, 1.0)
        }
        "snow" => {
            // Six arms.
            let arm = ((a * 3.0).sin()).abs();
            let width = (1.0 - arm * r * 6.0).clamp(0.0, 1.0);
            if r < 0.95 { width.max(edge(0.1, 0.2)) } else { 0.0 }
        }
        _ => 1.0,
    }
}

/// The image of shape `i`.
pub(crate) fn image(i: usize) -> Image {
    let side = side(i);
    let mut data = Vec::with_capacity((side * side * 4) as usize);
    let half = side as f32 / 2.0;
    for y in 0..side {
        for x in 0..side {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let a = if side == 1 {
                1.0
            } else {
                alpha(i, (px - half) / half, (py - half) / half, px, py)
            };
            data.extend_from_slice(&[255, 255, 255, (a * 255.0).round() as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_has_an_image_with_something_in_it() {
        for (i, name) in NAMES.iter().enumerate() {
            let img = image(i);
            let data = img.data.as_ref().unwrap();
            assert_eq!(data.len() as u32, side(i) * side(i) * 4);
            assert!(data.chunks(4).any(|p| p[3] > 0), "{name} is empty");
            assert_eq!(index(name), i);
        }
        assert_eq!(index("nonsense"), 0);
    }
}
