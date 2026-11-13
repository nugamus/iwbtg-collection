//! Works around a Bevy 0.20 sprite bug on DirectX 12 (#571, bevyengine/bevy#26509).
//!
//! Bevy 0.20 draws every `Sprite` as a 2D mesh whose shader (`sprite_material.wesl`) copies the
//! `instance_index` built-in into a `let` and indexes the per-sprite data with the copy. On DX12,
//! naga's HLSL output adds the draw's first instance to that built-in (D3D's `SV_InstanceID` starts
//! at 0 in every draw), and adds it again each time the copy is read (gfx-rs/wgpu#4564). So the
//! sprites of every draw but the first read the data of the instance at twice their index: they land
//! on another sprite's place or are not drawn at all. Bevy's own `mesh2d.wesl` avoids the copy and
//! reads `vertex.instance_index` each time; [`patch`] makes `sprite_material.wesl` do the same before
//! anything loads it. The shader means the same on every backend, so the patch is applied on all.
//!
//! Remove this module once the Bevy release we use draws sprites right on DX12 (#727).

use std::path::Path;

use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::prelude::*;

/// Where Bevy 0.20 embeds the sprite mesh shader.
const SPRITE_SHADER: &str = "bevy_sprite_render/sprite_mesh/sprite_material.wesl";

/// The copy that triggers the double offset.
const COPY: &str = "let instance_index = vertex.instance_index;";

/// Replaces Bevy's embedded sprite shader with the patched one. Add it after `DefaultPlugins`.
pub fn plugin(app: &mut App) {
    let Some(registry) = app.world().get_resource::<EmbeddedAssetRegistry>() else {
        return;
    };
    let path = Path::new(SPRITE_SHADER);
    let Some(original) = registry.remove_asset(path) else {
        warn!("render_fix: Bevy's {SPRITE_SHADER} is gone; sprites may draw wrong on DX12 (#571)");
        return;
    };
    let source = String::from_utf8_lossy(original.value()).into_owned();
    let fixed = patch(&source).unwrap_or_else(|| {
        warn!("render_fix: {SPRITE_SHADER} changed upstream; left as it is (#571)");
        source
    });
    registry.insert_asset(path.to_path_buf(), path, fixed.into_bytes());
}

/// `source` with the `let` copy of `instance_index` removed and every read of the copy turned into
/// `vertex.instance_index`. `None` when the copy is not there (Bevy changed the shader).
pub fn patch(source: &str) -> Option<String> {
    let at = source.find(COPY)?;
    let mut rest = String::with_capacity(source.len());
    rest.push_str(&source[..at]);
    rest.push_str(&source[at + COPY.len()..]);
    // Every `instance_index` that is not a field (`x.instance_index`) or a longer name is the copy.
    let mut out = String::with_capacity(rest.len() + 64);
    let mut tail = rest.as_str();
    while let Some(i) = tail.find("instance_index") {
        let (before, after) = tail.split_at(i);
        let after_word = &after["instance_index".len()..];
        let field = before.ends_with('.');
        let longer = before.chars().next_back().is_some_and(is_ident)
            || after_word.chars().next().is_some_and(is_ident);
        out.push_str(before);
        if !field && !longer {
            out.push_str("vertex.");
        }
        out.push_str("instance_index");
        tail = after_word;
    }
    out.push_str(tail);
    Some(out)
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parts of Bevy 0.20.1's `sprite_material.wesl` that name `instance_index`.
    const BEVY: &str = "\
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    let instance_index = vertex.instance_index;
    let slot = mesh2d_bindings::mesh[instance_index].material_bind_group_slot;
    let uncompressed_vertex = decompress_vertex(vertex, instance_index);
    var world_from_local = mesh_functions::get_world_from_local(instance_index);
    out.world_normal = mesh_functions::mesh2d_normal_local_to_world(uncompressed_vertex.normal, vertex.instance_index);
    out.instance_index = instance_index;
}
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    var output_color = sprite_functions::sample_final_color(mesh.uv, mesh.instance_index);
}
";

    #[test]
    fn the_patch_reads_the_built_in_each_time() {
        let fixed = patch(BEVY).unwrap();
        assert!(!fixed.contains("let instance_index"));
        assert!(fixed.contains("mesh2d_bindings::mesh[vertex.instance_index]"));
        assert!(fixed.contains("decompress_vertex(vertex, vertex.instance_index)"));
        assert!(fixed.contains("get_world_from_local(vertex.instance_index)"));
        assert!(fixed.contains("out.instance_index = vertex.instance_index;"));
        // Fields stay fields.
        assert!(fixed.contains("uncompressed_vertex.normal, vertex.instance_index)"));
        assert!(fixed.contains("sample_final_color(mesh.uv, mesh.instance_index)"));
        assert!(!fixed.contains("vertex.vertex."));
    }

    #[test]
    fn a_changed_shader_is_left_alone() {
        assert_eq!(
            patch("fn vertex(vertex: Vertex) { let i = vertex.instance_index; }"),
            None
        );
    }
}
