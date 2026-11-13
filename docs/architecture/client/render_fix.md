# The client: Render fixes

Part of [the client](../client.md).

`render_fix.rs` holds workarounds for bugs in Bevy or wgpu that break the picture on one backend. Each app (`view::run`, the UI gallery, the player card preview) adds `render_fix::plugin` right after `DefaultPlugins`. Each fix names its upstream issue and the follow-up issue that removes it.

- **Sprites on DirectX 12** (#571, bevyengine/bevy#26509, removal #727). Bevy 0.20 draws every `Sprite` as a 2D mesh; its shader (`bevy_sprite_render/sprite_mesh/sprite_material.wesl`) copies the `instance_index` built-in into a `let`. On DX12, naga adds the draw's first instance to the built-in and adds it again where the copy is read (gfx-rs/wgpu#4564), so every sprite batch after the first read another sprite's data: sprites drawn with the wrong texture, in the wrong place, or not at all. The plugin takes the shader out of Bevy's embedded assets before anything loads it, rewrites it to read `vertex.instance_index` each time (as Bevy's own `mesh2d.wesl` does), and puts it back. If a Bevy update changes the shader so the copy is gone, it logs a warning and leaves it alone. The rewrite means the same on every backend, so it runs on all of them.
