//! The game blurred behind the pause menu (m5 D13: `filter: blur(5px) saturate(.6)
//! brightness(.5)`, #1657).
//!
//! While the menu shows, a capture camera draws what the game camera sees into a picture at half
//! the game's size, and the game camera rests. The picture goes under the page at the game's
//! rectangle through the filter material (`ui::filter::blurred`): blurred by 5 artboard px at the
//! page's scale, saturation .6. The page's own black at 50 % is the brightness. Closing the menu
//! removes both and wakes the game camera. The sim and the game's frame are not touched: the sim
//! does not step while the menu shows, so the picture is the frame the player paused on.

use bevy::camera::RenderTarget;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureFormat};
use bevy::ui_render::prelude::MaterialNode;
use bevy::window::PrimaryWindow;

use crate::hud::HudLayout;
use crate::shell::RunScreen;
use crate::ui::filter::{self, FilterMaterial};
use crate::view::GameCamera;

/// The picture is the game's size over this: the blur hides the lost detail and costs a quarter.
const DOWN: f32 = 2.0;
/// The artboard's blur, in artboard px.
const BLUR: f32 = 5.0;
/// The artboard's saturation.
const SATURATE: f32 = 0.6;

/// The camera that draws the game into the picture.
#[derive(Component)]
pub struct Capture;

/// The picture's node, under the page.
#[derive(Component)]
pub struct Picture;

/// The size of the picture for a game rectangle `game` physical px big: whole texels, at least 1.
pub fn picture_size(game: Vec2) -> UVec2 {
    (game / DOWN).ceil().max(Vec2::ONE).as_uvec2()
}

/// The blur in the picture's texels for a page at UI scale `ui` on a window of DPI factor `dpi`:
/// 5 artboard px are `5 * ui * dpi` physical px, `DOWN` of which make a texel.
pub fn blur_texels(ui: f32, dpi: f32) -> f32 {
    BLUR * ui * dpi / DOWN
}

/// Opens, follows and closes the picture with the menu.
pub fn follow(world: &mut World) {
    let paused = crate::shell::run_screen(world) == Some(RunScreen::Paused);
    let capture = world.query_filtered::<Entity, With<Capture>>().iter(world).next();
    if !paused {
        if let Some(capture) = capture {
            close(world, capture);
        }
        return;
    }
    let Some(game) = world.get_resource::<HudLayout>().map(|l| l.0.game) else {
        return;
    };
    let Some((dpi, _)) = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .iter(world)
        .next()
        .map(|w| (w.resolution.scale_factor(), w.size()))
    else {
        return;
    };
    let ui = world.resource::<UiScale>().0;
    let size = picture_size(game.size());
    let Some((transform, projection)) = world
        .query_filtered::<(&Transform, &Projection), With<GameCamera>>()
        .iter(world)
        .next()
        .map(|(t, p)| (*t, p.clone()))
    else {
        return;
    };
    // The capture camera sees what the game camera sees: its transform, its scale over `DOWN`.
    let projection = match projection {
        Projection::Orthographic(mut o) => {
            o.scale *= DOWN;
            Projection::Orthographic(o)
        }
        other => other,
    };
    let capture = match capture {
        Some(c) => c,
        None => open(world, size, transform, projection.clone(), blur_texels(ui, dpi)),
    };
    // Follow a resize (the page is rebuilt for it too) and the scale.
    let image = world.get::<RenderTarget>(capture).and_then(|t| match t {
        RenderTarget::Image(i) => Some(i.handle.clone()),
        _ => None,
    });
    if let Some(image) = image
        && let Some(mut images) = world.get_resource_mut::<Assets<Image>>()
        && images.get(&image).is_some_and(|i| i.size() != size)
        && let Some(mut i) = images.get_mut(&image)
    {
        i.resize(Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        });
    }
    if let Some(mut t) = world.get_mut::<Transform>(capture)
        && *t != transform
    {
        *t = transform;
    }
    if let Some(mut p) = world.get_mut::<Projection>(capture) {
        *p = projection;
    }
    let mut nodes = world.query_filtered::<(&mut Node, &MaterialNode<FilterMaterial>), With<Picture>>();
    let at = game.min / (ui * dpi);
    let along = game.size() / (ui * dpi);
    let mut material = None;
    for (mut node, m) in nodes.iter_mut(world) {
        let want = (Val::Px(at.x), Val::Px(at.y), Val::Px(along.x), Val::Px(along.y));
        if (node.left, node.top, node.width, node.height) != want {
            (node.left, node.top, node.width, node.height) = want;
        }
        material = Some(m.0.clone());
    }
    if let Some(material) = material
        && let Some(mut materials) = world.get_resource_mut::<Assets<FilterMaterial>>()
    {
        let blur = blur_texels(ui, dpi).min(filter::MAX_BLUR);
        // Only when it changed: a write marks the material for upload.
        if materials.get(&material).is_some_and(|m| (m.blur() - blur).abs() > 1e-3)
            && let Some(mut m) = materials.get_mut(&material)
        {
            m.set_blur(blur);
        }
    }
}

fn open(world: &mut World, size: UVec2, transform: Transform, projection: Projection, blur: f32) -> Entity {
    let mut target = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    target.sampler = ImageSampler::linear();
    let image = world.resource_mut::<Assets<Image>>().add(target);
    let capture = world
        .spawn((
            Camera2d,
            Camera {
                // Before the cameras that show the picture.
                order: -40,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::Image(image.clone().into()),
            projection,
            transform,
            Capture,
            crate::shell::RUN_ONLY,
        ))
        .id();
    let mut cameras = world.query_filtered::<&mut Camera, With<GameCamera>>();
    for mut camera in cameras.iter_mut(world) {
        camera.is_active = false;
    }
    let node = filter::blurred(
        &mut world.resource_mut::<Assets<FilterMaterial>>(),
        image,
        blur,
        SATURATE,
        1.0,
    );
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        node,
        // Under the page (20) and its black at 50 %, over the backdrop.
        GlobalZIndex(19),
        Pickable::IGNORE,
        Picture,
        crate::shell::RUN_ONLY,
    ));
    capture
}

fn close(world: &mut World, capture: Entity) {
    world.entity_mut(capture).despawn();
    let pictures: Vec<Entity> = world.query_filtered::<Entity, With<Picture>>().iter(world).collect();
    for p in pictures {
        world.entity_mut(p).despawn();
    }
    let mut cameras = world.query_filtered::<&mut Camera, With<GameCamera>>();
    for mut camera in cameras.iter_mut(world) {
        camera.is_active = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Half the game's size, whole texels; 5 artboard px of blur at the page's scale.
    #[test]
    fn the_picture_is_half_size_and_blurs_five_artboard_px() {
        assert_eq!(picture_size(Vec2::new(1440.0, 1080.0)), UVec2::new(720, 540));
        assert_eq!(picture_size(Vec2::new(801.0, 1.0)), UVec2::new(401, 1));
        assert_eq!(picture_size(Vec2::ZERO), UVec2::ONE);
        // 1920x1080 at DPI 1: the page's scale is 1, 5 px are 2.5 texels.
        assert_eq!(blur_texels(1.0, 1.0), 2.5);
        assert_eq!(blur_texels(0.5, 2.0), 2.5);
    }

    /// Opening the menu starts the capture over the game's rectangle and rests the game camera;
    /// closing it takes both away and wakes the camera. The picture follows the game camera.
    #[test]
    fn the_picture_comes_and_goes_with_the_menu() {
        use bevy::window::WindowResolution;
        let mut world = World::new();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<Assets<FilterMaterial>>();
        world.insert_resource(UiScale(0.5));
        let layout = crate::hud::layout::Layout {
            game: Rect::new(240.0, 0.0, 1680.0, 1080.0),
            ..default()
        };
        world.insert_resource(HudLayout(layout));
        world.spawn((
            Window {
                resolution: WindowResolution::new(1920, 1080),
                ..default()
            },
            PrimaryWindow,
        ));
        let game = world
            .spawn((Camera::default(), Transform::from_xyz(400.0, 300.0, 0.0), Projection::from(OrthographicProjection::default_2d()), GameCamera))
            .id();
        world.insert_resource(State::new(RunScreen::Playing));
        follow(&mut world);
        assert_eq!(world.query::<&Capture>().iter(&world).count(), 0, "nothing while playing");

        world.insert_resource(State::new(RunScreen::Paused));
        follow(&mut world);
        let (capture, at, p) = world
            .query_filtered::<(Entity, &Transform, &Projection), With<Capture>>()
            .single(&world)
            .map(|(e, t, p)| (e, *t, p.clone()))
            .unwrap();
        assert_eq!(at.translation, Vec3::new(400.0, 300.0, 0.0), "it sees what the game sees");
        let Projection::Orthographic(o) = p else { panic!("orthographic") };
        assert_eq!(o.scale, DOWN, "at half the size");
        assert!(!world.get::<Camera>(game).unwrap().is_active, "the game camera rests");
        let node = world.query_filtered::<&Node, With<Picture>>().single(&world).unwrap();
        // 240 physical px at DPI 1 and UI scale .5 are 480 page px.
        assert_eq!((node.left, node.width), (Val::Px(480.0), Val::Px(2880.0)));
        let RenderTarget::Image(target) = world.get::<RenderTarget>(capture).unwrap() else {
            panic!("an image target")
        };
        let size = world.resource::<Assets<Image>>().get(&target.handle).unwrap().size();
        assert_eq!(size, UVec2::new(720, 540));

        world.insert_resource(State::new(RunScreen::Playing));
        follow(&mut world);
        assert_eq!(world.query::<&Capture>().iter(&world).count(), 0);
        assert_eq!(world.query::<&Picture>().iter(&world).count(), 0);
        assert!(world.get::<Camera>(game).unwrap().is_active, "the game camera is back");
    }
}
