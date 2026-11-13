//! Sheared shapes: a `Shape` on a UI node draws it as a parallelogram with rounded corners
//! (0.2 shear, 10 px radius) while the node's children (its text) stay upright, the way the
//! mockups use `skewX(-11.31deg)` with a counter-skewed child.
//!
//! The node keeps its plain layout box, so flex layout places it like the mockup's unskewed box.
//! A child layer (`ShapeLayer`), grown past the box by the shear overhang, the ring and the shadow,
//! draws the shape with `ShapeMaterial` (`shape.wgsl`). The layer is added and kept in step by
//! `sync_shapes`; screens only set `Shape`.
//!
//! The same pass applies `UiOpacity` (multiplied down the tree) and `FadeMask` (a vertical fade at
//! the top and bottom of a list) to shapes and to text spawned with an `Ink` colour.

use bevy::asset::uuid_handle;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::ui::UiSystems;
use bevy::ui_render::prelude::{MaterialNode, UiMaterial, UiMaterialPlugin};

use super::tokens::{RADIUS, SHEAR};

const SHADER: Handle<Shader> = uuid_handle!("6f0c2a9e-3c55-4d2b-9a43-2f1f6b8f0d11");

pub(super) fn plugin(app: &mut App) {
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            &SHADER,
            Shader::from_wgsl(include_str!("shape.wgsl"), "iwbtg_client/ui/shape.wgsl"),
        )
        .expect("the shape shader handle is free");
    app.add_plugins(UiMaterialPlugin::<ShapeMaterial>::default())
        .add_systems(PostUpdate, sync_shapes.after(UiSystems::PostLayout));
}

/// A horizontal or vertical gradient over the fill, three stops (start, `mid`, end).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Veil {
    pub stops: [Color; 3],
    /// Where the middle stop sits, 0 to 1.
    pub mid: f32,
    /// Top to bottom instead of left to right.
    pub vertical: bool,
}

impl Veil {
    /// Left to right through three colours, the middle one at `mid`.
    pub fn across(a: Color, b: Color, c: Color, mid: f32) -> Veil {
        Veil {
            stops: [a, b, c],
            mid,
            vertical: false,
        }
    }
}

/// A drop shadow below the shape (CSS `box-shadow: 0 <offset_y> <blur> <color>`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub offset_y: f32,
    pub blur: f32,
    pub color: Color,
}

/// Which edge a band runs along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Bottom,
}

/// A strip along one edge, sheared with the shape: the star colour on list entries, the accent
/// line under the title wedge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub side: Side,
    pub width: f32,
    pub color: Color,
    /// For a bottom band, the share of the width where it has faded out (0: solid).
    pub fade_to: f32,
}

/// What a sheared node looks like. Colours are sRGB with alpha, as the mockups write them.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Shape {
    pub fill: Color,
    /// Art drawn upright over the fill, covering the shape and clipped to it.
    pub image: Option<Handle<Image>>,
    pub veil: Option<Veil>,
    pub band: Option<Band>,
    /// An outline outside the shape (CSS `box-shadow: 0 0 0 <width> <color>`).
    pub ring: Option<(f32, Color)>,
    pub shadow: Option<Shadow>,
    /// Shear and radius; the tokens unless a part needs otherwise.
    pub shear: f32,
    pub radius: f32,
}

impl Shape {
    /// A plain sheared shape in one colour.
    pub fn fill(fill: Color) -> Shape {
        Shape {
            fill,
            image: None,
            veil: None,
            band: None,
            ring: None,
            shadow: None,
            shear: SHEAR,
            radius: RADIUS,
        }
    }

    pub fn with_image(mut self, image: Handle<Image>) -> Shape {
        self.image = Some(image);
        self
    }

    pub fn with_veil(mut self, veil: Veil) -> Shape {
        self.veil = Some(veil);
        self
    }

    pub fn with_band(mut self, band: Band) -> Shape {
        self.band = Some(band);
        self
    }

    pub fn with_ring(mut self, width: f32, color: Color) -> Shape {
        self.ring = Some((width, color));
        self
    }

    pub fn with_shadow(mut self, offset_y: f32, blur: f32, color: Color) -> Shape {
        self.shadow = Some(Shadow {
            offset_y,
            blur,
            color,
        });
        self
    }

    /// How far the layer reaches past the box: (left and right, top, bottom), for a box `height` tall.
    fn margins(&self, height: f32) -> (f32, f32, f32) {
        let ring = self.ring.map_or(0.0, |(w, _)| w);
        let (spread, dy) = self.shadow.map_or((0.0, 0.0), |s| (s.blur, s.offset_y.abs()));
        let around = ring.max(spread + dy) + 2.0;
        (self.shear * height / 2.0 + around, around, around)
    }
}

/// Multiplies the opacity of a node and everything under it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct UiOpacity(pub f32);

/// Fades whatever is drawn under this node out over its top and bottom `top`/`bottom` logical px
/// (the list fades of the mockups). Works over any background, art included.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FadeMask {
    pub top: f32,
    pub bottom: f32,
}

/// The colour of a text node before opacity and fades. Text spawned through the kit has it; an
/// icon given one (its `ImageNode` tint) fades the same way.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Ink(pub Color);

/// The child that draws a `Shape`.
#[derive(Component)]
pub struct ShapeLayer;

/// The material behind `ShapeLayer` (`shape.wgsl` documents each field).
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub struct ShapeMaterial {
    #[uniform(0)]
    pub u: ShapeUniform,
    #[texture(1)]
    #[sampler(2)]
    pub image: Option<Handle<Image>>,
}

#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct ShapeUniform {
    pub bg: Vec4,
    pub veil0: Vec4,
    pub veil1: Vec4,
    pub veil2: Vec4,
    pub ring: Vec4,
    pub shadow: Vec4,
    pub band: Vec4,
    pub geom: Vec4,
    pub margin: Vec4,
    pub mode: Vec4,
    pub extra: Vec4,
    pub fade: Vec4,
}

impl UiMaterial for ShapeMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SHADER)
    }
}

fn srgba(c: Color) -> Vec4 {
    Vec4::from_array(c.to_srgba().to_f32_array())
}

impl ShapeUniform {
    /// The uniform for `shape` on a box `height` logical px tall, at `scale` physical px per
    /// logical px, faded by `opacity` and the window-space `fade` band.
    fn new(shape: &Shape, height: f32, scale: f32, opacity: f32, fade: Vec4) -> ShapeUniform {
        let (mx, mt, mb) = shape.margins(height);
        let clear = Vec4::ZERO;
        let veil = shape.veil;
        let band = shape.band;
        let shadow = shape.shadow;
        ShapeUniform {
            bg: srgba(shape.fill),
            veil0: veil.map_or(clear, |v| srgba(v.stops[0])),
            veil1: veil.map_or(clear, |v| srgba(v.stops[1])),
            veil2: veil.map_or(clear, |v| srgba(v.stops[2])),
            ring: shape.ring.map_or(clear, |(_, c)| srgba(c)),
            shadow: shadow.map_or(clear, |s| srgba(s.color)),
            band: band.map_or(clear, |b| srgba(b.color)),
            geom: Vec4::new(shape.shear, shape.radius, scale, opacity),
            margin: Vec4::new(mx, mt, mb, shape.ring.map_or(0.0, |(w, _)| w)),
            mode: Vec4::new(
                veil.map_or(0.5, |v| v.mid),
                if veil.is_some_and(|v| v.vertical) {
                    1.0
                } else {
                    0.0
                },
                if shape.image.is_some() { 1.0 } else { 0.0 },
                match band.map(|b| b.side) {
                    None => 0.0,
                    Some(Side::Left) => 1.0,
                    Some(Side::Bottom) => 2.0,
                },
            ),
            extra: Vec4::new(
                shadow.map_or(0.0, |s| s.offset_y),
                shadow.map_or(0.0, |s| s.blur),
                band.map_or(0.0, |b| b.width),
                band.map_or(0.0, |b| b.fade_to),
            ),
            fade,
        }
    }
}

/// The window-space band (physical px) a `FadeMask` node fades over.
fn fade_band(mask: &FadeMask, node: &ComputedNode, at: &UiGlobalTransform) -> Vec4 {
    let scale = 1.0 / node.inverse_scale_factor().max(f32::EPSILON);
    let centre = at.translation.y;
    let (top, bottom) = (centre - node.size().y / 2.0, centre + node.size().y / 2.0);
    Vec4::new(top, top + mask.top * scale, bottom - mask.bottom * scale, bottom)
}

/// How visible a point at window y is inside `band`, as the shader computes it.
fn fade_at(band: Vec4, y: f32) -> f32 {
    if band.w <= band.x {
        return 1.0;
    }
    let smooth = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a).max(f32::EPSILON)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    smooth(band.x, band.y, y) * (1.0 - smooth(band.z, band.w, y))
}

/// Opacity multiplied down from every `UiOpacity` above, and the nearest `FadeMask` band.
fn inherited(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    opacities: &Query<&UiOpacity>,
    masks: &Query<(&FadeMask, &ComputedNode, &UiGlobalTransform)>,
) -> (f32, Vec4) {
    let mut opacity = 1.0;
    let mut band = Vec4::ZERO;
    loop {
        if let Ok(o) = opacities.get(entity) {
            opacity *= o.0;
        }
        if band == Vec4::ZERO
            && let Ok((mask, node, at)) = masks.get(entity)
        {
            band = fade_band(mask, node, at);
        }
        match parents.get(entity) {
            Ok(parent) => entity = parent.parent(),
            Err(_) => return (opacity, band),
        }
    }
}

/// Adds missing shape layers, sizes them for their shape and keeps each material, text colour,
/// opacity and fade in step. Writes only what changed, so a still screen costs no uploads.
#[allow(clippy::too_many_arguments, reason = "one pass over shapes and text")]
pub(super) fn sync_shapes(
    mut commands: Commands,
    shapes: Query<(Entity, Ref<Shape>, &ComputedNode, Option<&Children>)>,
    mut layers: Query<(&mut Node, Option<&MaterialNode<ShapeMaterial>>), With<ShapeLayer>>,
    mut inks: Query<(Entity, &Ink, &mut TextColor, &UiGlobalTransform)>,
    mut tints: Query<(Entity, &Ink, &mut ImageNode, &UiGlobalTransform), Without<TextColor>>,
    parents: Query<&ChildOf>,
    opacities: Query<&UiOpacity>,
    masks: Query<(&FadeMask, &ComputedNode, &UiGlobalTransform)>,
    mut materials: ResMut<Assets<ShapeMaterial>>,
) {
    for (entity, shape, computed, children) in &shapes {
        let layer = children.and_then(|c| c.iter().find(|c| layers.contains(*c)));
        let Some(layer) = layer else {
            let layer = commands
                .spawn((
                    ShapeLayer,
                    Node {
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                    Pickable::IGNORE,
                    // Under the node's own children, never over them.
                    ZIndex(-1),
                ))
                .id();
            commands.entity(entity).insert_children(0, &[layer]);
            continue;
        };
        let inverse = computed.inverse_scale_factor().max(f32::EPSILON);
        let height = computed.size().y * inverse;
        let (opacity, band) = inherited(entity, &parents, &opacities, &masks);
        let uniform = ShapeUniform::new(&shape, height, 1.0 / inverse, opacity, band);
        let Ok((mut node, material)) = layers.get_mut(layer) else {
            continue;
        };
        let (mx, mt, mb) = shape.margins(height);
        let (left, top, bottom) = (Val::Px(-mx), Val::Px(-mt), Val::Px(-mb));
        if node.left != left || node.top != top || node.bottom != bottom {
            node.left = left;
            node.right = left;
            node.top = top;
            node.bottom = bottom;
        }
        match material.and_then(|m| materials.get(&m.0).map(|a| (m, a))) {
            Some((handle, current)) => {
                if current.u != uniform || current.image != shape.image {
                    let id = handle.0.id();
                    if let Some(mut m) = materials.get_mut(id) {
                        m.u = uniform;
                        m.image.clone_from(&shape.image);
                    }
                }
            }
            None => {
                let handle = materials.add(ShapeMaterial {
                    u: uniform,
                    image: shape.image.clone(),
                });
                commands.entity(layer).insert(MaterialNode(handle));
            }
        }
    }
    for (entity, ink, mut color, at) in &mut inks {
        let (opacity, band) = inherited(entity, &parents, &opacities, &masks);
        let alpha = ink.0.alpha() * opacity * fade_at(band, at.translation.y);
        let want = ink.0.with_alpha(alpha);
        if color.0 != want {
            color.0 = want;
        }
    }
    for (entity, ink, mut image, at) in &mut tints {
        let (opacity, band) = inherited(entity, &parents, &opacities, &masks);
        let want = ink
            .0
            .with_alpha(ink.0.alpha() * opacity * fade_at(band, at.translation.y));
        if image.color != want {
            image.color = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layer_reaches_past_the_shear_ring_and_shadow() {
        let plain = Shape::fill(Color::WHITE);
        // 0.2 shear on a 50 px box leans 5 px each way, plus 2 px for antialiasing.
        assert_eq!(plain.margins(50.0), (7.0, 2.0, 2.0));
        let lifted = plain
            .with_shadow(8.0, 24.0, Color::BLACK)
            .with_ring(2.0, Color::WHITE);
        assert_eq!(lifted.margins(52.0), (5.2 + 34.0, 34.0, 34.0));
    }

    #[test]
    fn the_fade_is_open_in_the_middle_and_shut_at_the_edges() {
        let band = Vec4::new(100.0, 140.0, 360.0, 400.0);
        assert_eq!(fade_at(band, 100.0), 0.0);
        assert_eq!(fade_at(band, 250.0), 1.0);
        assert_eq!(fade_at(band, 400.0), 0.0);
        assert!((fade_at(band, 120.0) - 0.5).abs() < 1e-6);
        assert_eq!(fade_at(Vec4::ZERO, 5.0), 1.0);
    }
}
