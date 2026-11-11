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
}

impl Icon {
    /// Every icon, for the gallery.
    pub const ALL: [Icon; 12] = [
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
    ];

    /// The size of the view box the shapes are drawn in.
    fn view(self) -> (f32, f32) {
        match self {
            Icon::Search => (18.0, 18.0),
            Icon::Random => (16.0, 16.0),
            Icon::MusicBars => (21.0, 18.0),
            Icon::Arrow => (26.0, 26.0),
            Icon::Person => (16.0, 16.0),
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
    let (vw, vh) = icon.view();
    let t = Transform::from_scale(w as f32 / vw, h as f32 / vh);
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
    // White shapes: the premultiplied alpha is every channel, so the straight pixel is white
    // with that alpha.
    let rgba: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| [255, 255, 255, p.alpha()])
        .collect();
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
    Some(image)
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
