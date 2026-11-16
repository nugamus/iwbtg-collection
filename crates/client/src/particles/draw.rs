//! Drawing the particles: one 2D mesh per system, picture and blend, rebuilt when the systems
//! change. Each quad is one particle, tinted by its color and alpha; additive types use an
//! additive blend (`part_type_blend`). The meshes sit at the system's depth among the
//! instances and tile layers (`order::Ladder::depth_z`).

use std::collections::BTreeMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState,
};
use bevy::material::descriptor::RenderPipelineDescriptor;
use bevy::material::specialize::SpecializedMeshPipelineError;
use bevy::sprite_render::{
    ColorMaterial, ExtendedMaterial2d, Material2dKey, Material2dPipeline, Material2dPlugin,
    MaterialExtension2d,
};

use super::{Tracker, shapes};
use crate::shell::{InRun, RUN_ONLY};
use crate::view::{Game, RoomSize, Textures, order};

/// `bm_add`: the source times its alpha added to what is there.
#[derive(AsBindGroup, Asset, TypePath, Clone, Debug, Default)]
pub(crate) struct Additive {}

impl MaterialExtension2d for Additive {
    fn specialize(
        _pipeline: &Material2dPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(fragment) = descriptor.fragment.as_mut() {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::SrcAlpha,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent {
                        src_factor: BlendFactor::Zero,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                });
            }
        }
        Ok(())
    }
}

type AddMaterial = ExtendedMaterial2d<ColorMaterial, Additive>;

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(Material2dPlugin::<AddMaterial>::default());
    crate::shell::run_resource(app, Pictures::new);
    app.add_systems(
        Update,
        draw.after(crate::view::DrawInstances)
            .after(crate::view::follow_view)
            .in_set(InRun),
    );
}

/// What a quad shows: one of our shapes or a package sprite.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Picture {
    Shape(usize),
    Sprite(usize),
}

/// One mesh: a system, a picture, a blend.
type Key = (u32, Picture, bool);

/// The run's shape images, materials and meshes.
#[derive(Resource)]
pub(crate) struct Pictures {
    shapes: Vec<Handle<Image>>,
    plain: BTreeMap<Picture, Handle<ColorMaterial>>,
    added: BTreeMap<Picture, Handle<AddMaterial>>,
    meshes: BTreeMap<Key, (Entity, Handle<Mesh>)>,
    /// The tracker's generation the meshes show.
    shown: Option<u64>,
}

impl Pictures {
    fn new(world: &mut World) -> Pictures {
        let mut images = world.resource_mut::<Assets<Image>>();
        let shapes = (0..shapes::NAMES.len())
            .map(|i| images.add(shapes::image(i)))
            .collect();
        Pictures {
            shapes,
            plain: BTreeMap::new(),
            added: BTreeMap::new(),
            meshes: BTreeMap::new(),
            shown: None,
        }
    }
}

/// One mesh's vertices.
#[derive(Default)]
struct Quads {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    color: Vec<[f32; 4]>,
    index: Vec<u32>,
}

impl Quads {
    /// A quad of `size` (w, h) whose `pivot` (offset from its top-left, in its own pixels,
    /// y down) is at `at` (y up), turned `angle` degrees counter-clockwise.
    fn push(&mut self, at: Vec2, size: Vec2, pivot: Vec2, angle: f32, uv: Rect, rgba: [f32; 4]) {
        let base = self.pos.len() as u32;
        let rot = Vec2::from_angle(angle.to_radians());
        // Corners from the pivot, y up: top-left, top-right, bottom-right, bottom-left.
        let corners = [
            (Vec2::new(-pivot.x, pivot.y), Vec2::new(uv.min.x, uv.min.y)),
            (Vec2::new(size.x - pivot.x, pivot.y), Vec2::new(uv.max.x, uv.min.y)),
            (Vec2::new(size.x - pivot.x, pivot.y - size.y), Vec2::new(uv.max.x, uv.max.y)),
            (Vec2::new(-pivot.x, pivot.y - size.y), Vec2::new(uv.min.x, uv.max.y)),
        ];
        for (c, t) in corners {
            let p = at + rot.rotate(c);
            self.pos.push([p.x, p.y, 0.0]);
            self.uv.push([t.x, t.y]);
            self.color.push(rgba);
        }
        self.index
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn mesh(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.color)
            .with_inserted_indices(Indices::U32(self.index))
    }
}

/// Rebuilds the meshes when the systems changed, and keeps each at its system's depth.
#[allow(
    clippy::too_many_arguments,
    reason = "a Bevy system: one parameter per resource"
)]
fn draw(
    mut commands: Commands,
    game: NonSend<Game>,
    tracker: Res<Tracker>,
    mut pictures: ResMut<Pictures>,
    textures: Option<Res<Textures>>,
    room: Res<RoomSize>,
    ladder: Option<Res<order::Ladder>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut plain: ResMut<Assets<ColorMaterial>>,
    mut added: ResMut<Assets<AddMaterial>>,
    mut transforms: Query<&mut Transform>,
) {
    let def = game.sim.def();
    let world = game.sim.world();
    let none = order::Ladder::default();
    let ladder = ladder.as_deref().unwrap_or(&none);
    let placed: Vec<(f64, f32)> = if tracker.systems.is_empty() {
        Vec::new()
    } else {
        order::instance_z(world, ladder, |i| i.img.sprite().is_some())
            .into_iter()
            .filter_map(|(id, z)| world.get(id).map(|i| (i.depth.get(), z)))
            .collect()
    };
    let z_of = |depth: f64| ladder.depth_z(&placed, depth);
    if pictures.shown == Some(tracker.generation) {
        // Only the depth can move (instances come and go around it).
        for (&(s, _, _), (entity, _)) in &pictures.meshes {
            let depth = tracker
                .systems
                .get(&s)
                .and_then(|d| def.particle_systems.get(d.def))
                .map_or(0.0, |d| d.depth.get());
            if let Ok(mut t) = transforms.get_mut(*entity) {
                t.translation.z = z_of(depth);
            }
        }
        return;
    }
    pictures.shown = Some(tracker.generation);
    let half = room.0 / 2.0;
    let mut built: BTreeMap<Key, Quads> = BTreeMap::new();
    let mut depth_of: BTreeMap<u32, f64> = BTreeMap::new();
    for (&s, sys) in &tracker.systems {
        let Some(pd) = def.particle_systems.get(sys.def) else { continue };
        depth_of.insert(s, pd.depth.get());
        for p in &sys.particles {
            let Some(t) = pd.types.get(p.ty) else { continue };
            let (picture, frames, frame_size, pivot, strip) = match t.look.sprite {
                Some((id, _, _)) => {
                    let i = id.0 as usize;
                    let (Some(sd), Some(Some((_, strip)))) =
                        (def.sprites.get(i), textures.as_ref().and_then(|t| t.0.get(i)))
                    else {
                        continue;
                    };
                    let size = Vec2::new(sd.size.0 as f32, sd.size.1 as f32);
                    (
                        Picture::Sprite(i),
                        u32::from(sd.frames),
                        size,
                        Vec2::new(sd.origin.0 as f32, sd.origin.1 as f32),
                        strip.as_vec2(),
                    )
                }
                None => {
                    let i = shapes::index(t.look.shape.as_deref().unwrap_or("pixel"));
                    let side = shapes::side(i) as f32;
                    (
                        Picture::Shape(i),
                        1,
                        Vec2::splat(side),
                        Vec2::splat(side / 2.0),
                        Vec2::splat(side),
                    )
                }
            };
            let look = p.look(t, frames);
            if look.rgba[3] <= 0.0 || look.size <= 0.0 {
                continue;
            }
            let columns = ((strip.x / frame_size.x).round() as u32).max(1);
            let (fx, fy) = (
                (look.frame % columns) as f32 * frame_size.x,
                (look.frame / columns) as f32 * frame_size.y,
            );
            let uv = Rect::new(
                fx / strip.x,
                fy / strip.y,
                (fx + frame_size.x) / strip.x,
                (fy + frame_size.y) / strip.y,
            );
            let scale = Vec2::new(t.look.scale[0].get() as f32, t.look.scale[1].get() as f32)
                * look.size;
            let at = Vec2::new(p.x as f32 - half.x, half.y - p.y as f32);
            let [r, g, b, a] = look.rgba;
            let lin = Color::srgba(r, g, b, a).to_linear().to_f32_array();
            built
                .entry((s, picture, t.look.additive))
                .or_default()
                .push(at, frame_size * scale, pivot * scale, look.angle, uv, lin);
        }
    }
    // Meshes no longer drawn go.
    let stale: Vec<Key> = pictures
        .meshes
        .keys()
        .filter(|k| !built.contains_key(k))
        .copied()
        .collect();
    for k in stale {
        if let Some((entity, mesh)) = pictures.meshes.remove(&k) {
            commands.entity(entity).despawn();
            meshes.remove(&mesh);
        }
    }
    for (key, quads) in built {
        let (s, picture, additive) = key;
        let z = z_of(depth_of.get(&s).copied().unwrap_or(0.0));
        if let Some((entity, mesh)) = pictures.meshes.get(&key) {
            if let Some(mut m) = meshes.get_mut(mesh) {
                *m = quads.mesh();
            }
            if let Ok(mut t) = transforms.get_mut(*entity) {
                t.translation.z = z;
            }
            continue;
        }
        let image = match picture {
            Picture::Shape(i) => pictures.shapes[i].clone(),
            Picture::Sprite(i) => match textures.as_ref().and_then(|t| t.0.get(i)) {
                Some(Some((h, _))) => h.clone(),
                _ => continue,
            },
        };
        let mesh = meshes.add(quads.mesh());
        let base = ColorMaterial {
            texture: Some(image),
            ..ColorMaterial::default()
        };
        let mut entity = commands.spawn((Mesh2d(mesh.clone()), Transform::from_xyz(0.0, 0.0, z), RUN_ONLY));
        if additive {
            let m = pictures
                .added
                .entry(picture)
                .or_insert_with(|| {
                    added.add(AddMaterial {
                        base,
                        extension: Additive {},
                    })
                })
                .clone();
            entity.insert(MeshMaterial2d(m));
        } else {
            let m = pictures
                .plain
                .entry(picture)
                .or_insert_with(|| plain.add(base))
                .clone();
            entity.insert(MeshMaterial2d(m));
        }
        let id = entity.id();
        pictures.meshes.insert(key, (id, mesh));
    }
}
