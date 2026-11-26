//! Icons: the mockups' small line icons (the six stat marks, search, back, random, the music
//! preview bars), drawn from the same SVG path data the mockups use.
//!
//! Each icon is a few strokes and fills over a view box. When an icon node gets its size from
//! layout, a system rasterises it with tiny-skia at exactly that many physical pixels (white with
//! the shape's coverage as alpha) and the node's `ImageNode` colour tints it. So an icon is crisp
//! at every window size and `UiScale`, on native and wasm32 alike, and needs no asset files.
//! Images are cached per icon and pixel size.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use svgtypes::{SimplePathSegment, SimplifyingPathParser};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<IconImages>()
        .add_systems(PostUpdate, rasterize.after(bevy::ui::UiSystems::PostLayout));
}

/// Every icon the mockups draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Flag,
    Clock,
    Check,
    Bolt,
    Skull,
    Crown,
    Search,
    /// The chevron of the Back button.
    Back,
    Random,
    /// The four bars of the music preview.
    MusicBars,
    /// The input display's arrow, pointing up (`RunInfo.dc.html`; turned for the other ways).
    Arrow,
    /// A player with no avatar (`Leaderboard.dc.html`).
    Person,
    /// Time Attack's stopwatch in the toolbar's mode switch (`Toolbar.dc.html`, 20 px).
    TimeAttack,
    /// Classic's floppy disk in the toolbar's mode switch (20 px).
    Classic,
    /// The toolbar's notifications bell (22 px).
    Bell,
    /// An update is ready (`E8_Icons.dc.html`, `18 11a7 7 0 1 1-2-4.9M18 3v4h-4`): the update
    /// notice's mark and the main-latest dialog (#1606).
    Update,
    /// The toolbar's settings gear (22 px).
    Gear,
    /// The outlined person of the toolbar's Sign in button (16 px).
    SignIn,
    /// The main menu's buttons (`MainMenu.dc.html`, 32 px): Play's triangle.
    MenuPlay,
    /// Browse: an arrow down onto a line.
    MenuBrowse,
    /// Replays: a clock turning back.
    MenuReplays,
    /// Settings: a sun-like gear.
    MenuSettings,
    /// Exit: a door with an arrow out.
    MenuExit,
    /// Back: a chevron.
    MenuBack,
    /// Time Attack's stopwatch, at the menu's size.
    MenuTimeAttack,
    /// Classic's floppy disk, at the menu's size.
    MenuClassic,
    /// Race: a flag.
    MenuRace,
}

impl Icon {
    /// Every icon, for the gallery.
    pub const ALL: [Icon; 27] = [
        Icon::Flag,
        Icon::Clock,
        Icon::Check,
        Icon::Bolt,
        Icon::Skull,
        Icon::Crown,
        Icon::Search,
        Icon::Back,
        Icon::Random,
        Icon::MusicBars,
        Icon::Arrow,
        Icon::Person,
        Icon::TimeAttack,
        Icon::Classic,
        Icon::Bell,
        Icon::Update,
        Icon::Gear,
        Icon::SignIn,
        Icon::MenuPlay,
        Icon::MenuBrowse,
        Icon::MenuReplays,
        Icon::MenuSettings,
        Icon::MenuExit,
        Icon::MenuBack,
        Icon::MenuTimeAttack,
        Icon::MenuClassic,
        Icon::MenuRace,
    ];

    /// The size of the view box the shapes are drawn in.
    fn view(self) -> (f32, f32) {
        match self {
            Icon::Search => (18.0, 18.0),
            Icon::Random => (16.0, 16.0),
            Icon::MusicBars => (21.0, 18.0),
            Icon::Arrow => (26.0, 26.0),
            Icon::Person | Icon::SignIn => (16.0, 16.0),
            Icon::TimeAttack | Icon::Classic => (20.0, 20.0),
            Icon::Bell | Icon::Gear => (22.0, 22.0),
            Icon::Update => (20.0, 20.0),
            Icon::MenuPlay
            | Icon::MenuBrowse
            | Icon::MenuReplays
            | Icon::MenuSettings
            | Icon::MenuExit
            | Icon::MenuBack
            | Icon::MenuTimeAttack
            | Icon::MenuClassic
            | Icon::MenuRace => (32.0, 32.0),
            _ => (14.0, 14.0),
        }
    }

    /// Width over height.
    pub fn aspect(self) -> f32 {
        let (w, h) = self.view();
        w / h
    }

    /// The mockups' own height in logical px (the width follows `aspect`).
    pub fn default_height(self) -> f32 {
        self.view().1
    }

    fn ops(self) -> Vec<Op> {
        use Op::{Dot, Fill, Stroke};
        match self {
            Icon::Flag => vec![Stroke("M3 12.5V1.5M3 2h7l-1.5 2.5L10 7H3", 1.6)],
            Icon::Clock => vec![Stroke(
                "M7 1.5a5.5 5.5 0 1 0 0 11a5.5 5.5 0 1 0 0-11M7 4v3.2l2 1.3",
                1.6,
            )],
            Icon::Check => vec![Stroke(
                "M7 1.5a5.5 5.5 0 1 0 0 11a5.5 5.5 0 1 0 0-11M4.6 7.2l1.7 1.7l3.2-3.4",
                1.6,
            )],
            Icon::Bolt => vec![Stroke("M8 1.5L3.5 8H7l-1 4.5L10.5 6H7z", 1.6)],
            // The mockup draws the eyes as zero-length round-capped strokes: dots.
            Icon::Skull => vec![
                Stroke(
                    "M7 1.5c-3 0-5 2-5 4.6c0 1.6.8 2.7 2 3.3v2.1h6V9.4c1.2-.6 2-1.7 2-3.3c0-2.6-2-4.6-5-4.6",
                    1.6,
                ),
                Dot(5.0, 6.5, 0.8),
                Dot(9.0, 6.5, 0.8),
            ],
            Icon::Crown => vec![Stroke("M2 11.5h10M2.5 10L2 4l3 2.5L7 2.5l2 4L12 4l-.5 6z", 1.6)],
            Icon::Search => vec![
                Stroke("M2 7.5a5.5 5.5 0 1 0 11 0a5.5 5.5 0 1 0-11 0", 2.0),
                Stroke("M12 12l4.5 4.5", 2.0),
            ],
            Icon::Back => vec![Stroke("M9 2L4 7l5 5", 2.0)],
            Icon::Random => vec![
                Stroke(
                    "M4.5 1.5h7a3 3 0 0 1 3 3v7a3 3 0 0 1-3 3h-7a3 3 0 0 1-3-3v-7a3 3 0 0 1 3-3z",
                    1.6,
                ),
                Dot(5.0, 5.0, 1.3),
                Dot(11.0, 11.0, 1.3),
                Dot(8.0, 8.0, 1.3),
            ],
            // Bars 3 px wide, 3 px apart, 1 px corners, standing on the bottom edge.
            Icon::MusicBars => [(0.0, 10.0), (6.0, 18.0), (12.0, 7.0), (18.0, 14.0)]
                .into_iter()
                .map(|(x, h)| Fill(bar(x, h)))
                .collect(),
            Icon::Arrow => vec![Fill("M13 4L22 14H16.5V22H9.5V14H4Z".into())],
            Icon::Person => vec![
                Dot(8.0, 5.5, 3.0),
                Fill("M2.5 15c.6-3.4 2.8-5 5.5-5s4.9 1.6 5.5 5z".into()),
            ],
            Icon::TimeAttack => vec![Stroke(
                "M 10 4.5a6.5 6.5 0 1 0 0 13a6.5 6.5 0 1 0 0-13M 10 7.5V11l2.2 1.4M 8 2h4M 15 4.5l1.3-1.3",
                1.8,
            )],
            Icon::Classic => vec![Stroke("M 4 3h9.5L16 5.5V17H4zM 7 3v4.5h6V3M 7 17v-5h6v5", 1.8)],
            Icon::Bell => vec![Stroke(
                "M 6 15.5V10a5 5 0 0 1 10 0v5.5l1.5 1.5h-13zM 9.2 19a2 2 0 0 0 3.6 0",
                1.8,
            )],
            Icon::Update => vec![Stroke("M18 11a7 7 0 1 1-2-4.9M18 3v4h-4", 1.8)],
            Icon::Gear => vec![
                Stroke("M 11 8a3 3 0 1 0 0 6a3 3 0 1 0 0-6", 1.8),
                Stroke(
                    "M 11 2.5v2.4M 11 17.1v2.4M 2.5 11h2.4M 17.1 11h2.4M 5 5l1.7 1.7M 15.3 15.3L17 17M 5 17l1.7-1.7M 15.3 6.7L17 5",
                    1.8,
                ),
            ],
            Icon::SignIn => vec![
                Stroke("M 8 2.5a3 3 0 1 0 0 6a3 3 0 1 0 0-6", 1.6),
                Stroke("M 2.5 14c.8-3 3-4.5 5.5-4.5s4.7 1.5 5.5 4.5", 1.6),
            ],
            Icon::MenuPlay => vec![Stroke("M 11 7l14 9-14 9z", 2.2)],
            Icon::MenuBrowse => vec![Stroke("M 16 5v13M 10.5 12.5L16 18l5.5-5.5M 6 25h20", 2.2)],
            Icon::MenuReplays => vec![Stroke(
                "M 7.5 16a8.5 8.5 0 1 0 2.8-6.3M 7 6v4.5h4.5M 16 11.5v5l3.5 2",
                2.2,
            )],
            Icon::MenuSettings => vec![Stroke(
                "M 16 12a4 4 0 1 0 0 8a4 4 0 1 0 0-8M 16 4v3M 16 25v3M 4 16h3M 25 16h3M 7.5 7.5l2.1 2.1M 22.4 22.4l2.1 2.1M 7.5 24.5l2.1-2.1M 22.4 9.6l2.1-2.1",
                2.2,
            )],
            Icon::MenuExit => vec![Stroke("M 14 6H7v20h7M 19.5 10.5L25 16l-5.5 5.5M 25 16H13", 2.2)],
            Icon::MenuBack => vec![Stroke("M 19 8l-8 8 8 8", 2.2)],
            Icon::MenuTimeAttack => vec![Stroke(
                "M 16 9a9 9 0 1 0 0 18a9 9 0 1 0 0-18M 16 13v5l3 2M 13 4.5h6M 23 9l2-2",
                2.2,
            )],
            Icon::MenuClassic => vec![Stroke("M 6 5h15.5L26 9.5V27H6zM 10 5v7h10V5M 10 27v-8h12v8", 2.2)],
            Icon::MenuRace => vec![Stroke("M 8 27V5M 8 6h15l-3.5 5 3.5 5H8", 2.2)],
        }
    }
}

enum Op {
    /// A path stroked with round caps and joins, at this width in view-box units.
    Stroke(&'static str, f32),
    /// A filled path.
    Fill(String),
    /// A filled circle: x, y, radius.
    Dot(f32, f32, f32),
}

/// A 3 px wide bar with 1 px corners, `x` from the left and `h` high, standing on the bottom of
/// an 18-high view box.
fn bar(x: f32, h: f32) -> String {
    let y = 18.0 - h;
    format!(
        "M{} {}h1a1 1 0 0 1 1 1v{}a1 1 0 0 1-1 1h-1a1 1 0 0 1-1-1v-{}a1 1 0 0 1 1-1z",
        x + 1.0,
        y,
        h - 2.0,
        h - 2.0,
    )
}

/// Parses SVG path data (arcs become curves).
fn parse(d: &str) -> Option<Path> {
    let mut pb = PathBuilder::new();
    for seg in SimplifyingPathParser::from(d) {
        match seg.ok()? {
            SimplePathSegment::MoveTo { x, y } => pb.move_to(x as f32, y as f32),
            SimplePathSegment::LineTo { x, y } => pb.line_to(x as f32, y as f32),
            SimplePathSegment::Quadratic { x1, y1, x, y } => {
                pb.quad_to(x1 as f32, y1 as f32, x as f32, y as f32)
            }
            SimplePathSegment::CurveTo { x1, y1, x2, y2, x, y } => {
                pb.cubic_to(x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32)
            }
            SimplePathSegment::ClosePath => pb.close(),
        }
    }
    pb.finish()
}

/// Draws `icon` into a `w` x `h` pixel RGBA image: white, with the coverage as alpha.
pub fn render(icon: Icon, w: u32, h: u32) -> Option<Image> {
    let (w, h) = (w.max(1), h.max(1));
    let mut pixmap = Pixmap::new(w, h)?;
    draw(icon, &mut pixmap, Vec2::ZERO, Vec2::new(w as f32, h as f32))?;
    // White shapes: the premultiplied alpha is every channel, so the straight pixel is white
    // with that alpha.
    let alpha: Vec<u8> = pixmap.pixels().iter().map(|p| p.alpha()).collect();
    Some(white_image(w, h, &alpha))
}

/// A `w` x `h` white image with `alpha` (one byte per pixel, rows from the top) as its alpha,
/// sampled linearly: what the icons and glows (`glow.rs`) are tinted from.
pub(super) fn white_image(w: u32, h: u32, alpha: &[u8]) -> Image {
    let rgba: Vec<u8> = alpha.iter().flat_map(|&a| [255, 255, 255, a]).collect();
    let mut image = Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

/// Draws `icon`'s shapes in white into `pixmap`, filling the box at `at` of `size` pixels.
pub(super) fn draw(icon: Icon, pixmap: &mut Pixmap, at: Vec2, size: Vec2) -> Option<()> {
    let (vw, vh) = icon.view();
    let t = Transform::from_row(size.x / vw, 0.0, 0.0, size.y / vh, at.x, at.y);
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    for op in icon.ops() {
        match op {
            Op::Stroke(d, width) => {
                let stroke = Stroke {
                    width,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Stroke::default()
                };
                pixmap.stroke_path(&parse(d)?, &paint, &stroke, t, None);
            }
            Op::Fill(d) => pixmap.fill_path(&parse(&d)?, &paint, FillRule::Winding, t, None),
            Op::Dot(x, y, r) => {
                pixmap.fill_path(
                    &PathBuilder::from_circle(x, y, r)?,
                    &paint,
                    FillRule::Winding,
                    t,
                    None,
                );
            }
        }
    }
    Some(())
}

/// Marks a node as an icon; `rasterize` keeps its `ImageNode` image at the node's pixel size.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiIcon(pub Icon);

/// The rasterised images by icon and pixel size.
#[derive(Resource, Default)]
struct IconImages(HashMap<(Icon, u32, u32), Handle<Image>>);

/// A line icon of `height` logical px in `color`.
pub fn icon(icon: Icon, height: f32, color: Color) -> impl Bundle {
    (
        UiIcon(icon),
        Node {
            width: Val::Px(height * icon.aspect()),
            height: Val::Px(height),
            flex_shrink: 0.0,
            ..default()
        },
        ImageNode { color, ..default() },
    )
}

/// Gives each icon the image for its laid-out pixel size.
fn rasterize(
    mut icons: Query<(&UiIcon, &ComputedNode, &mut ImageNode), Changed<ComputedNode>>,
    mut cache: ResMut<IconImages>,
    mut images: ResMut<Assets<Image>>,
) {
    for (UiIcon(kind), node, mut image) in &mut icons {
        let size = node.size();
        let (w, h) = (size.x.round() as u32, size.y.round() as u32);
        if w == 0 || h == 0 {
            continue;
        }
        let handle = match cache.0.get(&(*kind, w, h)) {
            Some(handle) => handle.clone(),
            None => {
                let Some(img) = render(*kind, w, h) else {
                    continue;
                };
                let handle = images.add(img);
                cache.0.insert((*kind, w, h), handle.clone());
                handle
            }
        };
        if image.image != handle {
            image.image = handle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_renders_and_leaves_room_around_the_shape() {
        for kind in Icon::ALL {
            let (vw, vh) = kind.view();
            let img = render(kind, vw as u32 * 4, vh as u32 * 4).expect("renders");
            let data = img.data.expect("pixels");
            assert!(data.chunks(4).any(|p| p[3] > 200), "{kind:?} draws opaque pixels");
            assert!(data.chunks(4).any(|p| p[3] == 0), "{kind:?} is not a solid block");
        }
    }

    #[test]
    fn the_bars_stand_on_the_bottom_edge() {
        let img = render(Icon::MusicBars, 21, 18).expect("renders");
        let data = img.data.expect("pixels");
        let at = |x: usize, y: usize| data[(y * 21 + x) * 4 + 3];
        // The first bar is 10 high: solid at its foot, empty above it.
        assert!(at(2, 16) > 200);
        assert_eq!(at(2, 3), 0);
        // The second is 18 high: solid at the top.
        assert!(at(8, 2) > 200);
    }
}
