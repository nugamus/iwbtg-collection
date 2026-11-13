# 0003: UI toolkit

Status: Accepted by Claude on the user's behalf, 2026-11-04 (user may revisit). Issue #490.

Related: [architecture, UI](../architecture/client/ui.md), [product](../product.md) (UI decisions), the M4 mockups in [`docs/design/mockups/m4/`](../design/mockups/m4/README.md).

## Context

Every M4 and M6 screen (game select, mod select, loading, HUD, pause, results) uses the same shared design language (mockup README, Shared design language): rows, panels, pills and buttons sheared by 0.2 (`skewX(-11.31deg)`) with upright text and a 10 px corner radius; game art clipped to sheared panels; gradients over art; lists that fade out at their edges; Barlow with tabular numerals; OutQuint and OutElastic motion. The client is Bevy 0.20 and must run natively and in a browser (wasm32, WebGL2).

The toolkit has to support:

1. Horizontal shear 0.2 on a panel while its text stays upright.
2. A 10 px corner radius on the sheared shape.
3. An image clipped to a sheared panel.
4. Gradients.
5. Masks for list fades.
6. Tabular numerals.
7. Native and wasm32 from one code path.

## Options considered

| Option | Verdict | Reason |
|---|---|---|
| **bevy_ui with one custom `UiMaterial` for sheared shapes** | **Chosen** | Ships with Bevy 0.20, so no extra dependency or version lag. Flex layout, text (Parley, OpenType features), picking, focus and the headless widgets (`bevy_ui_widgets`: buttons with keyboard activation, Tab navigation) come with it. A `UiMaterial` is a fragment shader on a node's quad, so one WGSL shader draws the sheared rounded shape, its image, gradient, band, ring and shadow. It runs on WebGL2: the shader uses only `fwidth`, `textureSample` and `textureDimensions`, all in GLSL ES 3.0. |
| bevy_ui alone (no custom material) | Rejected | `BorderRadius`, `BackgroundGradient` and `BoxShadow` exist, but every node is an axis-aligned rectangle. `UiTransform` can rotate and scale, not shear, and would shear the text with it. |
| Sheared nodes with a sheared `UiTransform` plus counter-sheared children | Rejected | `UiTransform` has no shear (translation, scale, rotation only); faking one with rotation and non-uniform scale distorts the radius and the text. |
| Sprites and meshes in world space (`bevy_sprite`, `Mesh2d`) | Rejected | Shear is easy on a mesh, but layout, text wrapping, focus, picking and accessibility would all be ours to write. |
| egui (`bevy_egui`) | Rejected | Immediate-mode UI built for tools. No shear, no per-widget shaders; matching the mockups would mean painting every shape by hand. Fine later for the editor. |
| Third-party retained UI crates (`bevy_lunex`, `sickle_ui`, `bevy_cobweb_ui`) | Rejected | Each lags Bevy releases, and none draws sheared shapes; we would still need the material. |
| HTML/CSS overlay on the web build | Rejected | Matches the mockups exactly in a browser, but not on native, and splits the UI into two implementations. |

## Decision

The client draws its UI with **bevy_ui** and one material, **`ShapeMaterial`** (`crates/client/src/ui/shape.rs`, `shape.wgsl`). All of it lives in `crates/client/src/ui/` (architecture, UI). How each requirement is met:

1. **Shear with upright text.** A node with a `Shape` component keeps its plain layout box, so flex layout places it exactly like the mockup's unskewed `div`. A child layer (`ShapeLayer`, absolutely positioned, behind the node's other children) is grown past the box by the shear overhang (0.1 × height on each side) plus room for the ring and shadow. Its fragment shader undoes the shear (`x + 0.2·y` about the box centre) and tests the point against the unsheared rounded box. The node's children (text, pills) are ordinary upright nodes.
2. **Radius.** The rounded-box distance is taken before the shear, so corners are sheared ellipses, like CSS `border-radius` under `skewX`. Edges are antialiased over one physical pixel with `fwidth`.
3. **Image clipped to the shape.** `Shape::image` is sampled upright, `cover`-fitted to the sheared shape's bounding box, and kept only inside the shape.
4. **Gradients.** `Shape::veil` is a three-stop gradient over the fill and image (left to right in the shape's own space, or top to bottom), mixed in premultiplied sRGB as CSS does. `Shape::band` is a strip along the left or bottom edge (the star colour on list entries, the accent line under the title wedge), optionally fading out along its length. Unsheared gradients (screen vignettes) use bevy_ui's own `BackgroundGradient`.
5. **List fades.** `FadeMask { top, bottom }` on a list container fades everything under it over its top and bottom edges. Shapes fade per pixel in the shader (a window-space band in the uniform); text fades per line by its centre (the `Ink` colour times the fade). This works over any background, art included, which a gradient overlay in the ground colour would not. A true per-pixel mask for text would need render-to-texture; not needed for the mockups.
6. **Tabular numerals.** Every kit text style turns on the OpenType `tnum` feature (`TextFont::font_features`); Barlow has it.
7. **Native and wasm32.** Fonts are compiled into the binary (`include_bytes!`) and the shader is inserted under a fixed handle at startup, so nothing is loaded from an asset folder or over the network. The wasm32 build opens the gallery (dev-setup, UI gallery in a browser).

Also part of the kit:

- **Tokens** (`ui/tokens.rs`): the README's colours by name, the star ramp from `Main.dc.html`, shear, radius and motion durations.
- **Fonts** (`ui/fonts.rs`): Barlow 400–700, Barlow Condensed 500–700 with italic 600/700, Pixelify Sans (pause box only), all SIL OFL 1.1, and a subset of Noto Sans Symbols 2 (OFL) as the fallback for `★ ☆ ▾ ▸ ✓` and heavy arrows, which Barlow lacks, a subset of Noto Sans Math (OFL, U+2190–2199, 7 KB, `assets/fonts/make-arrows-subset.py`) for the basic arrows `← ↑ → ↓` that neither Barlow nor Symbols 2 has (#655), and a subset of Noto Sans JP (OFL) as the Japanese fallback (see Japanese text below). `Type` holds the mockups' text styles (family, size, weight, italic, tracking, capitals).
- **Components** (`ui/widgets.rs`): sheared panel, pill, button (Primary, Secondary, Control; rest, hover, pressed, focus-visible), label/value stat, star pill, mod pill, Ranked and Unranked pills, tag pill, one-line text cut with `…` (`ellipsis_text`, #636). Buttons are `bevy_ui_widgets::Button`: a click, Enter or Space sends `Activate`; Tab moves focus.
- **Button states.** The mockups draw only rest. Hover mixes 12% white into the fill (glass also gets 8% more opaque), press mixes 18% black and scales to 0.96, both eased with OutQuint over 150 ms. The focus ring appears only for keyboard focus (CSS `:focus-visible`): 2 px of white at 85%, the same kind of ring as the selected game panel.
- **Backdrop** (`ui/backdrop.rs`): one background system for every screen. The art covers the window and is blurred in the shader by sampling its mip chain (one 3x3 tap on the level that matches the radius), not by a CPU blur or a render-to-texture pass: the slider is continuous, there is no per-change cost and it works the same on wasm32. It has its own camera under everything, so the game's sprites and the UI draw over it without a UI node covering the game. Costs: art needs a mip chain (a third more memory), and the blur is a mip approximation (slightly boxy at the top of the slider, hidden by the dim). The blur is the `display.backdrop_blur` setting (percent); `[` and `]` change and save it until the settings slider (#53) exists.
- **Icons** (`ui/icons.rs`): the mockups' line icons (flag, clock, check, bolt, skull, crown, search, back chevron, random dice, music bars, the input arrow, person) are kept as the mockups' own SVG path data in Rust. `icons::icon(Icon, height, colour)` spawns an image node; once layout gives it a size, a system rasterises the paths with tiny-skia (path strings parsed by svgtypes, arcs turned into curves) at exactly that many physical pixels, as white with coverage in alpha, and the node's colour tints it. Chosen over an icon font (the mockups' icons are strokes, so a font would need an extra build step and a design tool) and over an SDF material (a shader per shape kind). Rasterising at the laid-out size keeps them crisp at every window size and `UiScale`, works the same on native and wasm32 (both crates are pure Rust), and needs no asset files. Images are cached per icon and size, so a resize costs one raster per icon. `stat` takes an optional icon.
- **Motion** (`ui/motion.rs`): `Ease::OutQuint` and `Ease::OutElastic`, and `Tween<P>` components for x and y offset (`UiTransform`, so layout does not move), scale, width and opacity (`UiOpacity`). `ReducedMotion` makes every tween land at once; `--reduced-motion` turns it on, and a settings entry will later.
- **Gallery** (`ui/gallery.rs`, `--ui-gallery`): every token and component, at the mockups' 1920×1080 scaled to the window.

## Consequences

- Each sheared node costs one extra UI node and one material instance; a material is only re-uploaded when its shape, opacity or fade changes. Fine for lists of tens of rows; revisit if a screen draws hundreds.
- The pointer hit area of a sheared node is its unsheared layout box (the overhang corners do not count). The mockups' shapes are wide and short, so the difference is a few pixels at the slanted ends.
- A shadow drawn outside the node can overlap the node before it in the same row; the mockups' shadows are soft enough that this does not show.
- Text inside a fading list fades per line, not per pixel.
- **Japanese text (#502).** `NotoSansJP-subset.ttf` is Noto Sans JP instanced at weight 500, cut to kana, CJK and full-width punctuation, and the 2965 JIS X 0208 level 1 kanji (3675 characters, about 1 MB). It is the last family in every list, so Latin text still uses Barlow and symbols and arrows still use Noto Sans Symbols 2 and Noto Sans Math. One weight serves all weights (bold Japanese is not synthesized). The full font is about 9.6 MB, level 1 + 2 would be 2.2 MB; level 1 covers the port list and ordinary names. Rare kanji, Korean and Chinese-only characters still show a missing glyph: widen the subset (`assets/fonts/make-jp-subset.py`) or load a font on demand if players ask for it. It adds 1,049,600 bytes to the wasm32 binary (about 640 KB gzipped). Line breaking needs no extra data: bevy_text's default build (no `complex_script_segmentation`) breaks Japanese between any two characters by the UAX #14 rules, as browsers do. The log line `ICU4X data error: No segmentation model for complex script: Chinese/Japanese` comes from the word segmenter, which then treats a Japanese run as one word; that only affects word-wise cursor moves and selection, which the client does not have.
