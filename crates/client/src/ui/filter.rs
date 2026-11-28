//! A picture under a CSS colour filter: `filter: saturate(s) brightness(b)` (`filter.wgsl`), which
//! Bevy UI has no way to draw with an `ImageNode` tint (a tint only multiplies; it cannot take
//! colour out). Used for the results heatmap's room (#705) and, with `blur(r)` in front, the game
//! behind the pause menu (#1657). The filter runs on the GPU each frame, so it works on pictures a
//! camera renders into (render targets) as well as on loaded images.

use bevy::asset::uuid_handle;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::ui_render::prelude::{MaterialNode, UiMaterial, UiMaterialPlugin};

const SHADER: Handle<Shader> = uuid_handle!("8c3b51e2-7f4a-4d06-9b1e-2a6f0d9c4e17");

pub(super) fn plugin(app: &mut App) {
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            &SHADER,
            Shader::from_wgsl(include_str!("filter.wgsl"), "iwbtg_client/ui/filter.wgsl"),
        )
        .expect("the filter shader handle is free");
    app.add_plugins(UiMaterialPlugin::<FilterMaterial>::default());
}

/// The material behind [`filtered`].
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub struct FilterMaterial {
    #[uniform(0)]
    u: FilterUniform,
    #[texture(1)]
    #[sampler(2)]
    picture: Option<Handle<Image>>,
}

#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct FilterUniform {
    /// x saturate, y brightness, z the blur's standard deviation in the picture's texels (0: none).
    p: Vec4,
}

impl UiMaterial for FilterMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SHADER)
    }
}

/// `picture` stretched over the node, drawn as CSS `filter: saturate(saturate)
/// brightness(brightness)` draws it (1 and 1 leave it as it is). Spawn it with a `Node`.
pub fn filtered(
    materials: &mut Assets<FilterMaterial>,
    picture: Handle<Image>,
    saturate: f32,
    brightness: f32,
) -> MaterialNode<FilterMaterial> {
    MaterialNode(materials.add(FilterMaterial {
        u: FilterUniform {
            p: Vec4::new(saturate, brightness, 0.0, 0.0),
        },
        picture: Some(picture),
    }))
}

/// `picture` as CSS `filter: blur(r) saturate(s) brightness(b)` draws it, with `blur` the
/// Gaussian's standard deviation in the picture's own texels (CSS `blur(r)` is that deviation in
/// CSS px). The shader samples at most 8 texels out each way, so keep `blur` at 4 or less.
pub fn blurred(
    materials: &mut Assets<FilterMaterial>,
    picture: Handle<Image>,
    blur: f32,
    saturate: f32,
    brightness: f32,
) -> MaterialNode<FilterMaterial> {
    MaterialNode(materials.add(FilterMaterial {
        u: FilterUniform {
            p: Vec4::new(saturate, brightness, blur.clamp(0.0, MAX_BLUR), 0.0),
        },
        picture: Some(picture),
    }))
}

/// The most blur [`blurred`] draws, in texels: the shader's 8-texel reach is two deviations.
pub const MAX_BLUR: f32 = 4.0;

impl FilterMaterial {
    /// The blur, in texels (0: none).
    pub fn blur(&self) -> f32 {
        self.u.p.z
    }

    /// Sets the blur, clamped to [`MAX_BLUR`].
    pub fn set_blur(&mut self, blur: f32) {
        self.u.p.z = blur.clamp(0.0, MAX_BLUR);
    }
}

/// What the shader does to one sRGB colour, for the tests and for anyone checking a screenshot.
#[cfg(test)]
pub fn apply(srgb: [f32; 3], saturate: f32, brightness: f32) -> [f32; 3] {
    let luma = 0.213 * srgb[0] + 0.715 * srgb[1] + 0.072 * srgb[2];
    srgb.map(|c| ((luma + (c - luma) * saturate) * brightness).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturate_matches_the_filter_effects_matrix() {
        // The CSS matrix for saturate(.4), row by row, on a pure red then brightness(.7).
        let s = 0.4;
        let got = apply([1.0, 0.0, 0.0], s, 0.7);
        let want = [0.213 + 0.787 * s, 0.213 - 0.213 * s, 0.213 - 0.213 * s].map(|v| v * 0.7);
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-6, "{got:?} vs {want:?}");
        }
        // Grey has no colour to take out; only brightness changes it.
        for g in apply([0.5, 0.5, 0.5], s, 0.7) {
            assert!((g - 0.35).abs() < 1e-6, "{g}");
        }
        // 1 and 1 leave a colour alone.
        let c = [0.2, 0.6, 0.9];
        let same = apply(c, 1.0, 1.0);
        for (a, b) in same.iter().zip(c) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
