//! The visual mods (#550; docs/product.md, Mods): Hitboxes (`HB`) and No background (`NB`),
//! picked with `--mods` (mods.rs). Both only change the picture: they read the sim's state and
//! never write it, so the replay and every hash stay the same.
//!
//! - Hitboxes draws the collision mask of the kid (green) and of every hazard (red: the `killer`
//!   class, and killers that kill in their own collision) where the last tick placed it, pixel
//!   by pixel as the sim tests it (`collide::instance_box`, `overlap::point_in`), so a precise
//!   spike shows its triangle. Invisible hazards are drawn too. A mask's picture is cached by
//!   its shape, scale and angle, so a room of spikes builds one image.
//! - No background draws a plain grey backdrop instead of the room's background colour and
//!   background layers (art.rs reads [`PlainBackdrop`]). Tiles, instances and foreground layers
//!   stay.

use std::collections::{BTreeMap, HashMap};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;
use iwbtg_sim::collide::instance_box;
use iwbtg_sim::collide::overlap::point_in;
use iwbtg_sim::collide::PlacedMask;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef};
use iwbtg_sim::mods::{Mod, ModSet};
use iwbtg_sim::world::{InstId, Instance};

use crate::view::{Game, RoomSize};

/// No background: the colour the room is drawn on instead of its background art.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlainBackdrop(pub Color);

/// No background's backdrop: a neutral grey that the kid's dark outline and light spikes both
/// stand out on.
pub(crate) const PLAIN: Color = Color::srgb_u8(0x3a, 0x3a, 0x3a);

/// In front of every instance, tile and foreground layer (995 and up), behind the overlay (1000).
const HITBOX_Z: f32 = 998.0;
/// The kid's hitbox.
const KID: Color = Color::srgb_u8(0x45, 0xd4, 0x83);
/// A hazard's hitbox.
const HAZARD: Color = Color::srgb_u8(0xff, 0x3d, 0x55);
/// How opaque a mask's inside is; its edge pixels are opaque.
const FILL_ALPHA: u8 = 0x60;
/// Masks larger than this many pixels are not drawn (a full-room killer would cost a large image
/// for a box that covers the screen anyway).
const MAX_PIXELS: i64 = 1 << 22;

/// Adds the run's visual mods.
pub struct VisualPlugin(pub ModSet);

impl Plugin for VisualPlugin {
    fn build(&self, app: &mut App) {
        if self.0.contains(Mod::NoBackground) {
            app.insert_resource(PlainBackdrop(PLAIN));
        }
        if self.0.contains(Mod::Hitboxes) {
            app.init_resource::<Hitboxes>()
                .add_systems(Update, draw_hitboxes.after(crate::view::follow_view));
        }
    }
}

/// What a hitbox shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Kid,
    Hazard,
}

/// The role of an instance's hitbox, or `None` when Hitboxes leaves it out.
fn role(def: &GameDef, inst: &Instance) -> Option<Role> {
    if inst.classes.intersects(ClassMask::KID | ClassMask::KID_FLIPPED) {
        return Some(Role::Kid);
    }
    let killer_kind = def.kind(inst.kind).is_some_and(|k| k.base == Builtin::Killer);
    (inst.classes.intersects(ClassMask::KILLER) || killer_kind).then_some(Role::Hazard)
}

/// A mask's picture does not depend on where it stands: the mask, its transform and where its
/// box lies relative to the transform's origin.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Shape {
    mask: usize,
    origin: (i32, i32),
    scale: (u64, u64),
    angle: u64,
    offset: (i64, i64),
    size: (i64, i64),
}

fn shape(placed: &PlacedMask) -> Shape {
    let b = placed.bbox;
    let xf = &placed.xf;
    Shape {
        mask: std::ptr::from_ref(placed.mask) as usize,
        origin: xf.origin,
        scale: (xf.xscale.get().to_bits(), xf.yscale.get().to_bits()),
        angle: xf.angle.get().to_bits(),
        offset: (b.left - xf.at.0, b.top - xf.at.1),
        size: (b.right - b.left + 1, b.bottom - b.top + 1),
    }
}

/// The mask's pixels as an RGBA image, white: solid pixels at [`FILL_ALPHA`], solid pixels with
/// an empty neighbour (or on the box's edge) opaque, the rest clear. `None` for a mask too large
/// to draw.
fn rasterize(placed: &PlacedMask) -> Option<(u32, u32, Vec<u8>)> {
    let b = placed.bbox;
    let (w, h) = (b.right - b.left + 1, b.bottom - b.top + 1);
    if w <= 0 || h <= 0 || w.saturating_mul(h) > MAX_PIXELS {
        return None;
    }
    let solid = |x: i64, y: i64| point_in(placed, b.left + x, b.top + y).unwrap_or(false);
    let inside: Vec<bool> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| solid(x, y)).collect();
    let at = |x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h && inside[(y * w + x) as usize];
    let mut rgba = Vec::with_capacity(inside.len() * 4);
    for y in 0..h {
        for x in 0..w {
            let a = if !at(x, y) {
                0
            } else if at(x - 1, y) && at(x + 1, y) && at(x, y - 1) && at(x, y + 1) {
                FILL_ALPHA
            } else {
                0xff
            };
            rgba.extend_from_slice(&[0xff, 0xff, 0xff, a]);
        }
    }
    Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?, rgba))
}

/// The hitbox sprites on screen and the mask pictures built so far.
#[derive(Resource, Default)]
struct Hitboxes {
    drawn: BTreeMap<InstId, Entity>,
    images: HashMap<Shape, Option<Handle<Image>>>,
}

/// Each frame: one sprite per kid and hazard mask, where the last tick placed it.
fn draw_hitboxes(
    mut commands: Commands,
    game: NonSend<Game>,
    room: Res<RoomSize>,
    mut images: ResMut<Assets<Image>>,
    mut boxes: ResMut<Hitboxes>,
    mut query: Query<(&mut Sprite, &mut Transform)>,
) {
    let def = game.sim.def();
    let rules = &game.sim.rules().collide;
    let boxes = &mut *boxes;
    let mut alive = BTreeMap::new();
    for inst in game.sim.world().instances() {
        if !inst.alive {
            continue;
        }
        let Some(role) = role(def, inst) else { continue };
        let Ok(Some(placed)) = instance_box(def, inst, (inst.m.x(), inst.m.y()), rules) else {
            continue;
        };
        let handle = boxes
            .images
            .entry(shape(&placed))
            .or_insert_with(|| {
                let (w, h, rgba) = rasterize(&placed)?;
                Some(images.add(Image::new(
                    Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    rgba,
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::RENDER_WORLD,
                )))
            })
            .clone();
        let Some(handle) = handle else { continue };
        let color = match role {
            Role::Kid => KID,
            Role::Hazard => HAZARD,
        };
        let (left, top) = (placed.bbox.left as f32, placed.bbox.top as f32);
        // The kid's box over the hazards'.
        let z = HITBOX_Z + if role == Role::Kid { 0.5 } else { 0.0 } + (inst.id.0 % 1000) as f32 * 1e-4;
        let t = Transform::from_xyz(left - room.0.x / 2.0, room.0.y / 2.0 - top, z);
        let entity = match boxes.drawn.get(&inst.id).copied() {
            Some(e) => {
                if let Ok((mut sprite, mut tr)) = query.get_mut(e) {
                    sprite.image = handle;
                    sprite.color = color;
                    *tr = t;
                }
                e
            }
            None => commands
                .spawn((
                    Sprite {
                        image: handle,
                        color,
                        ..default()
                    },
                    Anchor::TOP_LEFT,
                    t,
                ))
                .id(),
        };
        alive.insert(inst.id, entity);
    }
    for (id, e) in &boxes.drawn {
        if !alive.contains_key(id) {
            commands.entity(*e).despawn();
        }
    }
    boxes.drawn = alive;
}

#[cfg(test)]
mod tests {
    use iwbtg_sim::collide::Xform;
    use iwbtg_sim::def::{BitMask, IRect, Mask, MaskShape};
    use iwbtg_sim::num::Real;

    use super::*;

    fn placed(mask: &Mask, at: (i64, i64)) -> PlacedMask<'_> {
        let b = mask.bbox;
        PlacedMask {
            bbox: IRect {
                left: b.left + at.0,
                top: b.top + at.1,
                right: b.right + at.0,
                bottom: b.bottom + at.1,
            },
            mask,
            xf: Xform {
                at,
                origin: (0, 0),
                xscale: Real::ONE,
                yscale: Real::ONE,
                angle: Real::ZERO,
            },
        }
    }

    fn alpha(img: &(u32, u32, Vec<u8>)) -> Vec<Vec<u8>> {
        let (w, _, rgba) = img;
        rgba.chunks(*w as usize * 4)
            .map(|row| row.chunks(4).map(|p| p[3]).collect())
            .collect()
    }

    #[test]
    fn a_box_mask_is_an_opaque_frame_round_a_lighter_inside() {
        let mask = Mask {
            bbox: IRect {
                left: 0,
                top: 0,
                right: 2,
                bottom: 2,
            },
            shape: MaskShape::Rect,
        };
        let img = rasterize(&placed(&mask, (10, 20))).unwrap();
        assert_eq!((img.0, img.1), (3, 3));
        let a = alpha(&img);
        assert_eq!(a[0], vec![0xff, 0xff, 0xff]);
        assert_eq!(a[1], vec![0xff, FILL_ALPHA, 0xff]);
        assert_eq!(a[2], vec![0xff, 0xff, 0xff]);
    }

    #[test]
    fn a_precise_mask_shows_only_its_solid_pixels() {
        let mut bits = BitMask::new(2, 2).unwrap();
        bits.set(0, 0, true);
        bits.set(1, 1, true);
        let mask = Mask {
            bbox: IRect {
                left: 0,
                top: 0,
                right: 1,
                bottom: 1,
            },
            shape: MaskShape::Bits(bits),
        };
        let a = alpha(&rasterize(&placed(&mask, (0, 0))).unwrap());
        assert_eq!(a, vec![vec![0xff, 0], vec![0, 0xff]]);
    }

    #[test]
    fn the_same_mask_elsewhere_reuses_its_picture() {
        let mask = Mask {
            bbox: IRect {
                left: 1,
                top: 1,
                right: 4,
                bottom: 4,
            },
            shape: MaskShape::Rect,
        };
        assert_eq!(shape(&placed(&mask, (0, 0))), shape(&placed(&mask, (96, 32))));
        let other = mask.clone();
        assert_ne!(shape(&placed(&mask, (0, 0))), shape(&placed(&other, (0, 0))));
    }
}
