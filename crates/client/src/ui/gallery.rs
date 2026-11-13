//! `iwbtg-client --ui-gallery`: a dev-only screen with every token and component of the kit,
//! laid out at the mockups' 1920x1080 and scaled to the window, so reviewers can hold it next to
//! the canvas's shared parts. Hover a game panel to see it slide, click Pop and Fade for the
//! OutElastic and OutQuint tweens, Tab through the buttons for the focus ring, M toggles reduced
//! motion. The art in the panels is drawn here at run time (no game art).

use bevy::asset::RenderAssetUsages;
use bevy::input_focus::tab_navigation::TabGroup;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::IsDefaultUiCamera;
use bevy::ui_widgets::Activate;

use super::UiKitPlugin;
use super::backdrop::{self, Backdrop, dim};
use super::fonts::{Family, Type};
use super::icons::{Icon, icon};
use super::motion::{Ease, OffsetX, Opacity, ReducedMotion, Tween, Width};
use super::shape::{Band, FadeMask, Shape, Side, UiOpacity, Veil};
use super::tokens::{self, star_color};
use super::widgets::{
    ButtonKind, button, control, ellipsis_text, mod_pill, panel, ranked_pill, star_pill, stat, tag_pill,
    text, unranked_pill,
};

/// The size the mockups are drawn at.
const DESIGN: Vec2 = Vec2::new(1920.0, 1080.0);

/// Opens the gallery window and runs until it closes.
///
/// `window` (`--window`) sets the size in physical pixels; `ratio` (`--ratio`) without a window
/// makes a 1600-wide window of that ratio. Either one makes the window exactly that many pixels
/// whatever the monitor's scaling, so `--screenshot` images come out at the size asked for.
pub fn run(
    screenshot: Option<std::path::PathBuf>,
    reduced_motion: bool,
    window: Option<(u32, u32)>,
    ratio: Option<(u32, u32)>,
) {
    let size = window.or_else(|| ratio.map(|(w, h)| (1600, 1600 * h / w)));
    let resolution = match size {
        Some((w, h)) => bevy::window::WindowResolution::new(w, h).with_scale_factor_override(1.0),
        None => (1600, 900).into(),
    };
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "IWBTG Collection: UI gallery".to_string(),
            resolution,
            #[cfg(target_arch = "wasm32")]
            fit_canvas_to_parent: true,
            ..default()
        }),
        ..default()
    }))
    .add_plugins((crate::render_fix::plugin, UiKitPlugin))
    .insert_resource(ClearColor(tokens::GROUND))
    .insert_resource(ReducedMotion(reduced_motion))
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (
            fit_to_window,
            slide_on_hover,
            drift_list,
            motion_switch,
            backdrop_keys,
        ),
    )
    .add_observer(on_activate);
    if let Some(path) = screenshot {
        app.insert_resource(crate::view::ScreenshotTo(path))
            .add_systems(Update, crate::view::take_screenshot);
    }
    app.run();
}

/// Scales the 1920x1080 layout to fit the window.
fn fit_to_window(window: Single<&Window, Changed<Window>>, mut scale: ResMut<UiScale>) {
    let size = window.size();
    let fit = (size.x / DESIGN.x).min(size.y / DESIGN.y);
    if fit > 0.0 && (scale.0 - fit).abs() > 1e-4 {
        scale.0 = fit;
    }
}

/// Placeholder scenery for the panels: a sky in `hue`, two rows of hills and, with `tower`, a
/// tower. Drawn here, so the gallery needs no art files.
fn scenery(hue: f32, tower: bool) -> Image {
    let (w, h) = (320u32, 96u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            let sky = Color::hsl(hue, 0.45, 0.42 - 0.22 * fy);
            let far = 0.55 + 0.08 * (fx * 9.0).sin() + 0.05 * (fx * 23.0).cos();
            let near = 0.75 + 0.07 * (fx * 5.0 + 1.0).sin();
            let tower = tower && (fx - 0.72).abs() < 0.035 && fy > 0.22;
            let c = if fy > near {
                Color::hsl(hue + 30.0, 0.35, 0.10)
            } else if tower || fy > far {
                Color::hsl(hue + 15.0, 0.30, 0.20)
            } else {
                sky
            };
            rgba.extend_from_slice(&c.to_srgba().to_u8_array());
        }
    }
    Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// What a gallery button does.
#[derive(Component, Clone, Copy)]
enum Action {
    Pop,
    Fade,
    ReducedMotion,
}

/// The row that pops wider.
#[derive(Component)]
struct PopRow {
    wide: bool,
}

/// The panel that fades.
#[derive(Component)]
struct FadePanel {
    shown: bool,
}

/// Game panels slide 25 px left while hovered.
#[derive(Component)]
struct SlideOnHover;

/// The list under the fade mask, drifting up and down.
#[derive(Component)]
struct Drift;

/// Shows whether reduced motion is on.
#[derive(Component)]
struct MotionState;

fn heading(label: &str) -> impl Bundle + use<> {
    text(label, Type::LABEL, tokens::MUTED)
}

fn row(gap: f32) -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(gap),
        flex_wrap: FlexWrap::Wrap,
        row_gap: Val::Px(gap),
        ..default()
    }
}

/// Every icon at the mockups' size and at 2.5 times that, the small ones muted, the big ones in ink.
fn icon_row() -> impl Bundle + use<> {
    let small = Icon::ALL.map(|kind| (kind, kind.default_height(), tokens::MUTED));
    let big = Icon::ALL.map(|kind| (kind, kind.default_height() * 2.5, tokens::INK));
    (
        row(14.0),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            for (kind, height, color) in small.into_iter().chain(big) {
                c.spawn(icon(kind, height, color));
            }
        })),
    )
}

fn column(gap: f32) -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(gap),
        ..default()
    }
}

/// A colour chip with its token name under it.
fn swatch(name: &str, color: Color) -> impl Bundle + use<> {
    (
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(6.0),
            ..default()
        },
        children![
            (
                Node {
                    width: Val::Px(64.0),
                    height: Val::Px(34.0),
                    ..default()
                },
                Shape::fill(color).with_ring(1.0, tokens::white(0.18)),
            ),
            text(name, Type::barlow(12.0, 600), tokens::SOFT),
        ],
    )
}

/// A list entry (difficulty and category) as on game select: star band, star pill, name, PB.
fn entry(name: &str, stars: f32, pb: &str, width: f32) -> impl Bundle + use<> {
    let shape = Shape::fill(tokens::dark(tokens::CARD_SOLID, 0.82)).with_band(Band {
        side: Side::Left,
        width: 6.0,
        color: star_color(stars),
        fade_to: 0.0,
    });
    let pb_color = if pb == "No PB" { tokens::MUTED } else { tokens::INK };
    (
        panel(width, 45.0, shape),
        children![
            star_pill(stars),
            (
                Node {
                    width: Val::Px(260.0),
                    ..default()
                },
                children![text(name, Type::BODY, tokens::INK)],
            ),
            text(pb, Type::barlow(15.0, 600), pb_color),
        ],
    )
}

fn game_panel(
    title: &str,
    creator: &str,
    art: Handle<Image>,
    stars: &[f32],
    selected: bool,
) -> impl Bundle + use<> {
    let veil = if selected {
        Veil::across(
            Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, 0.82),
            Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, 0.45),
            Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, 0.0),
            0.45,
        )
    } else {
        Veil::across(
            Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, 0.9),
            Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, 0.6),
            Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, 0.2),
            0.45,
        )
    };
    let mut shape = Shape::fill(tokens::GROUND).with_image(art).with_veil(veil);
    shape = if selected {
        shape
            .with_ring(2.0, tokens::white(0.55))
            .with_shadow(10.0, 30.0, tokens::black(0.5))
    } else {
        shape.with_shadow(4.0, 12.0, tokens::black(0.25))
    };
    let dots: Vec<Color> = stars.iter().map(|s| star_color(*s)).collect();
    (
        panel(700.0, 72.0, shape),
        SlideOnHover,
        Hovered::default(),
        UiTransform::default(),
        Tween::<OffsetX>::at_rest(0.0, tokens::motion::STANDARD, Ease::OutQuint),
        children![
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    width: Val::Px(420.0),
                    ..default()
                },
                children![
                    text(title, Type::barlow(23.0, 700), tokens::INK),
                    text(creator, Type::barlow(14.0, 400), Color::srgb_u8(0xdf, 0xe2, 0xec)),
                ],
            ),
            (
                Node {
                    flex_wrap: FlexWrap::NoWrap,
                    ..row(5.0)
                },
                Children::spawn(SpawnIter(dots.into_iter().map(|c| {
                    (
                        Node {
                            width: Val::Px(10.0),
                            height: Val::Px(10.0),
                            border_radius: BorderRadius::all(Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(c),
                    )
                }))),
            ),
        ],
    )
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, reduced: Res<ReducedMotion>) {
    // The backdrop has its own camera; this one draws over it and must not clear.
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::None,
            ..default()
        },
        IsDefaultUiCamera,
    ));
    commands.insert_resource(BackdropArts(
        [245.0, 140.0, 330.0, 190.0]
            .map(|hue| images.add(backdrop::scene_image(hue)))
            .to_vec(),
    ));
    let castle = images.add(scenery(250.0, true));
    let forest = images.add(scenery(140.0, true));
    let city = images.add(scenery(300.0, true));
    // Under the controls: hills only, as a tower cut by the short strip reads as a stray block.
    let hills = images.add(scenery(140.0, false));

    let left = commands
        .spawn(column(26.0))
        .with_children(|c| {
            c.spawn((
                column(4.0),
                children![
                    text("UI gallery", Type::TITLE, tokens::INK),
                    text(
                        "Shared parts of docs/design/mockups/m4 · Tab moves focus · M toggles reduced motion",
                        Type::barlow(15.0, 500),
                        tokens::MUTED,
                    ),
                ],
            ));
            c.spawn((
                column(10.0),
                children![
                    heading("Colour tokens"),
                    (
                        row(14.0),
                        children![
                            swatch("ground", tokens::GROUND),
                            swatch("ink", tokens::INK),
                            swatch("muted", tokens::MUTED),
                            swatch("soft", tokens::SOFT),
                            swatch("accent", tokens::ACCENT),
                            swatch("ahead", tokens::AHEAD),
                            swatch("behind", tokens::BEHIND),
                            swatch("gold", tokens::GOLD),
                            swatch("amber", tokens::AMBER),
                            swatch("ranked", tokens::RANKED),
                        ],
                    ),
                ],
            ));
            c.spawn((column(10.0), children![heading("Star ramp")]))
                .with_children(|c| {
                    c.spawn(row(8.0)).with_children(|r| {
                        for s in [0.5, 2.4, 3.12, 4.87, 5.63, 6.84, 7.3, 8.21] {
                            r.spawn(star_pill(s));
                        }
                    });
                });
            c.spawn((
                column(10.0),
                children![
                    heading("Type"),
                    (
                        row(22.0),
                        children![
                            text("Barlow 400", Type::barlow(20.0, 400), tokens::INK),
                            text("Barlow 500", Type::barlow(20.0, 500), tokens::INK),
                            text("Barlow 600", Type::barlow(20.0, 600), tokens::INK),
                            text("Barlow 700", Type::barlow(20.0, 700), tokens::INK),
                            text("Condensed 700", Type::condensed(22.0, 700), tokens::INK),
                            text(
                                "PAUSE",
                                Type {
                                    family: Family::Pixel,
                                    ..Type::barlow(22.0, 400)
                                },
                                tokens::INK,
                            ),
                        ],
                    ),
                    (
                        row(40.0),
                        children![
                            (
                                Node {
                                    flex_direction: FlexDirection::Column,
                                    align_items: AlignItems::FlexEnd,
                                    ..default()
                                },
                                children![
                                    text("38:12.46", Type::barlow(18.0, 700), tokens::INK),
                                    text("11:11.11", Type::barlow(18.0, 700), tokens::INK),
                                    text("1:04:20.98", Type::barlow(18.0, 700), tokens::INK),
                                ],
                            ),
                            text(
                                "Tabular numerals: the digits line up",
                                Type::barlow(15.0, 500),
                                tokens::MUTED
                            ),
                            text("−1.24  +0.80  ★", Type::barlow(18.0, 700), tokens::AHEAD,),
                            text(
                                "39:40.12 → 38:12.46  ← ↑ ↓ ▸",
                                Type::barlow(18.0, 700),
                                tokens::INK
                            ),
                        ],
                    ),
                ],
            ));
            c.spawn((
                column(10.0),
                children![
                    heading("Pills"),
                    (
                        row(8.0),
                        children![
                            ranked_pill(),
                            unranked_pill(),
                            star_pill(6.84),
                            mod_pill("SD"),
                            mod_pill("SL"),
                            tag_pill("Needle"),
                            tag_pill("Avoidance"),
                        ],
                    ),
                ],
            ));
            c.spawn((
                column(10.0),
                children![heading("Ellipsis (one line cut at its box)")],
            ))
            .with_children(|c| {
                for width in [320.0, 200.0, 120.0, 40.0] {
                    c.spawn(Node {
                        width: Val::Px(width),
                        flex_direction: FlexDirection::Column,
                        ..default()
                    })
                    .with_child(ellipsis_text(
                        "I wanna be the Kaenbyou · ばけばけ",
                        Type::barlow(15.0, 600),
                        tokens::ROW_NAME,
                    ));
                }
            });
            c.spawn((column(10.0), children![heading("Buttons")]))
                .with_children(|c| {
                    c.spawn(row(12.0)).with_children(|r| {
                        r.spawn(button("Back", ButtonKind::Secondary));
                        r.spawn(button("Options", ButtonKind::Secondary));
                        r.spawn(button("Random", ButtonKind::Secondary));
                        r.spawn(button("Mods", ButtonKind::Primary));
                    });
                    // Controls sit on art in the screens, so they are shown on art here, as an
                    // upright art shape: an `ImageNode` draws its image inside the padding, so the
                    // padding never showed (#546). It keeps its size when the column overflows.
                    c.spawn((
                        Node {
                            align_self: AlignSelf::FlexStart,
                            padding: UiRect::all(Val::Px(14.0)),
                            column_gap: Val::Px(8.0),
                            align_items: AlignItems::Center,
                            flex_shrink: 0.0,
                            ..default()
                        },
                        Shape {
                            shear: 0.0,
                            ..Shape::fill(tokens::GROUND).with_image(hills)
                        },
                    ))
                    .with_children(|r| {
                        r.spawn(control("Sort", "Stars ▾"));
                        r.spawn((button("Pop", ButtonKind::Control), Action::Pop));
                        r.spawn((button("Fade", ButtonKind::Control), Action::Fade));
                        r.spawn((
                            button("Reduced motion", ButtonKind::Control),
                            Action::ReducedMotion,
                        ));
                    });
                    c.spawn((
                        text(&motion_label(reduced.0), Type::barlow(14.0, 500), tokens::MUTED),
                        MotionState,
                    ));
                    c.spawn((text("", Type::barlow(14.0, 500), tokens::MUTED), BackdropState));
                });
            c.spawn((column(10.0), children![heading("Icons"), icon_row()]));
            c.spawn((
                column(10.0),
                children![
                    heading("Stats"),
                    (
                        row(36.0),
                        children![
                            stat(Some(Icon::Flag), "Stages", "7", tokens::INK),
                            stat(Some(Icon::Clock), "Avg clear", "54:20.36", tokens::INK),
                            stat(Some(Icon::Check), "Clear rate", "3.8%", tokens::INK),
                            stat(Some(Icon::Bolt), "Your PB", "38:12.46", tokens::INK),
                            stat(Some(Icon::Skull), "Your deaths", "9,412", tokens::INK),
                            stat(Some(Icon::Crown), "World record", "31:41.02", tokens::GOLD),
                        ],
                    ),
                ],
            ));
        })
        .id();

    let wedge = Shape::fill(tokens::GROUND)
        .with_image(castle.clone())
        .with_veil(Veil::across(
            Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, 0.9),
            Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, 0.62),
            Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, 0.12),
            0.55,
        ))
        .with_band(Band {
            side: Side::Bottom,
            width: 4.0,
            color: tokens::ACCENT,
            fade_to: 0.7,
        })
        .with_shadow(24.0, 60.0, tokens::black(0.45));

    let right = commands
        .spawn(column(26.0))
        .with_children(|c| {
            c.spawn((
                column(10.0),
                children![
                    heading("Panel with art, veil and accent band"),
                    (
                        wedge,
                        Node {
                            width: Val::Px(780.0),
                            height: Val::Px(150.0),
                            padding: UiRect::new(Val::Px(56.0), Val::Px(30.0), Val::Px(0.0), Val::Px(0.0)),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::Center,
                            row_gap: Val::Px(10.0),
                            ..default()
                        },
                        children![
                            (
                                row(8.0),
                                children![
                                    ranked_pill(),
                                    star_pill(6.84),
                                    text("Very Hard · Any%", Type::barlow(16.0, 600), tokens::SOFT)
                                ]
                            ),
                            text(
                                "I wanna be the Kaenbyou",
                                Type::condensed(52.0, 700).italic(),
                                tokens::INK
                            ),
                        ],
                    ),
                ],
            ));
            c.spawn((
                column(10.0),
                children![heading("Game panels (hover slides 25 px, OutQuint 400 ms)")],
            ))
            .with_children(|c| {
                c.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(3.0),
                        padding: UiRect::left(Val::Px(40.0)),
                        ..default()
                    },
                    children![
                        game_panel(
                            "I wanna eat the lemon",
                            "AutumnWater",
                            forest,
                            &[2.4, 3.9, 5.1],
                            false
                        ),
                        game_panel(
                            "I wanna be the Kaenbyou",
                            "Kira☆~Kira☆~",
                            castle,
                            &[3.12, 4.87, 6.84, 8.21, 5.63],
                            true
                        ),
                        game_panel(
                            "I wanna help the Fairy tale",
                            "れおちゅー",
                            city,
                            &[3.8, 5.2, 6.1],
                            false
                        ),
                    ],
                ));
            });
            c.spawn((
                column(10.0),
                children![heading("Pop (OutElastic) and fade (OutQuint)")],
            ))
            .with_children(|c| {
                c.spawn((
                    Shape::fill(tokens::ROW_YOU).with_shadow(6.0, 18.0, tokens::black(0.35)),
                    Node {
                        width: Val::Px(600.0),
                        height: Val::Px(50.0),
                        padding: UiRect::horizontal(Val::Px(20.0)),
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(16.0),
                        margin: UiRect::left(Val::Px(40.0)),
                        ..default()
                    },
                    PopRow { wide: false },
                    Tween::<Width>::at_rest(600.0, tokens::motion::POP, Ease::OutElastic),
                    children![
                        text("#25", Type::condensed(18.0, 700), tokens::INK),
                        text("nugamus", Type::barlow(17.0, 700), tokens::INK),
                        text("Your PB", Type::PILL_CAPS, Color::srgb_u8(0xe3, 0xe5, 0xec)),
                        text("38:12.46", Type::barlow(18.0, 700), tokens::INK),
                    ],
                ));
                c.spawn((
                    Shape::fill(tokens::SECONDARY),
                    Node {
                        width: Val::Px(600.0),
                        height: Val::Px(45.0),
                        padding: UiRect::horizontal(Val::Px(20.0)),
                        align_items: AlignItems::Center,
                        margin: UiRect::left(Val::Px(40.0)),
                        ..default()
                    },
                    UiOpacity(1.0),
                    FadePanel { shown: true },
                    Tween::<Opacity>::at_rest(1.0, tokens::motion::QUICK, Ease::OutQuint),
                    children![text("Fades out and back in", Type::BODY, tokens::INK)],
                ));
            });
            c.spawn((
                column(10.0),
                children![heading("List fade mask (top and bottom 48 px)")],
            ))
            .with_children(|c| {
                c.spawn((
                    Node {
                        height: Val::Px(230.0),
                        width: Val::Px(760.0),
                        overflow: Overflow::clip(),
                        padding: UiRect::horizontal(Val::Px(30.0)),
                        ..default()
                    },
                    FadeMask {
                        top: 48.0,
                        bottom: 48.0,
                    },
                ))
                .with_children(|m| {
                    m.spawn((column(3.0), Drift, UiTransform::default()))
                        .with_children(|l| {
                            let entries = [
                                ("Medium · Any%", 3.12, "24:51.30"),
                                ("Hard · Any%", 4.87, "31:08.94"),
                                ("Very Hard · Any%", 6.84, "38:12.46"),
                                ("Impossible · Any%", 8.21, "No PB"),
                                ("Hard · 100%", 5.63, "No PB"),
                                ("Easy · Any%", 2.40, "12:03.77"),
                                ("Normal · Any%", 2.95, "17:40.12"),
                                ("Very Hard · 100%", 7.30, "No PB"),
                            ];
                            for (name, stars, pb) in entries {
                                l.spawn(entry(name, stars, pb, 680.0));
                            }
                        });
                });
            });
        })
        .id();

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(64.0), Val::Px(40.0)),
                column_gap: Val::Px(70.0),
                ..default()
            },
            TabGroup::default(),
        ))
        .add_children(&[left, right]);
}

/// The stand-in art the A key cycles through.
#[derive(Resource)]
struct BackdropArts(Vec<Handle<Image>>);

/// The line that reports the backdrop's state.
#[derive(Component)]
struct BackdropState;

/// A cycles the art (it cross-fades), [ and ] move the blur, D cycles the dim of each screen.
fn backdrop_keys(
    keys: Res<ButtonInput<KeyCode>>,
    arts: Res<BackdropArts>,
    mut backdrop: ResMut<Backdrop>,
    mut label: Query<&mut Text, With<BackdropState>>,
    mut index: Local<(usize, usize)>,
) {
    const DIMS: [(&str, f32); 4] = [
        ("select", dim::SELECT),
        ("loading", dim::LOADING),
        ("gameplay", dim::GAMEPLAY),
        ("results", dim::RESULTS),
    ];
    let first = backdrop.art.is_none();
    if keys.just_pressed(KeyCode::KeyA) || first {
        if !first {
            index.0 = (index.0 + 1) % arts.0.len();
        }
        backdrop.art = Some(arts.0[index.0].clone());
    }
    if keys.just_pressed(KeyCode::KeyD) {
        index.1 = (index.1 + 1) % DIMS.len();
    }
    backdrop.dim = DIMS[index.1].1;
    if keys.just_pressed(KeyCode::BracketRight) {
        backdrop.blur = (backdrop.blur + 0.1).min(1.0);
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        backdrop.blur = (backdrop.blur - 0.1).max(0.0);
    }
    for mut text in &mut label {
        text.0 = format!(
            "Backdrop: art {} of {} (A) · blur {:.0}% ([ ]) · dim {} {:.2} (D)",
            index.0 + 1,
            arts.0.len(),
            backdrop.blur * 100.0,
            DIMS[index.1].0,
            backdrop.dim
        );
    }
}

fn motion_label(on: bool) -> String {
    format!("Reduced motion: {}", if on { "on" } else { "off" })
}

/// Game panels whose hover state just changed.
type HoverChanged = (With<SlideOnHover>, Changed<Hovered>);

/// Game panels slide 25 px left while hovered and back after.
fn slide_on_hover(mut panels: Query<(&Hovered, &mut Tween<OffsetX>), HoverChanged>) {
    for (hovered, mut tween) in &mut panels {
        tween.retarget(if hovered.get() { -25.0 } else { 0.0 });
    }
}

/// Moves the masked list up and down so its rows pass through the fades (still under reduced motion).
fn drift_list(time: Res<Time>, reduced: Res<ReducedMotion>, mut list: Query<&mut UiTransform, With<Drift>>) {
    let y = if reduced.0 {
        -40.0
    } else {
        -40.0 + 70.0 * (time.elapsed_secs() * 0.6).sin()
    };
    for mut t in &mut list {
        t.translation.y = Val::Px(y);
    }
}

fn motion_switch(
    keys: Res<ButtonInput<KeyCode>>,
    mut reduced: ResMut<ReducedMotion>,
    mut label: Query<&mut Text, With<MotionState>>,
) {
    if keys.just_pressed(KeyCode::KeyM) {
        reduced.0 = !reduced.0;
    }
    if reduced.is_changed() {
        for mut text in &mut label {
            text.0 = motion_label(reduced.0);
        }
    }
}

fn on_activate(
    activate: On<Activate>,
    actions: Query<&Action>,
    mut reduced: ResMut<ReducedMotion>,
    mut pop: Query<(&mut PopRow, &mut Tween<Width>)>,
    mut fade: Query<(&mut FadePanel, &mut Tween<Opacity>)>,
) {
    let Ok(action) = actions.get(activate.entity) else {
        return;
    };
    match action {
        Action::Pop => {
            for (mut row, mut tween) in &mut pop {
                row.wide = !row.wide;
                tween.retarget(if row.wide { 620.0 } else { 600.0 });
            }
        }
        Action::Fade => {
            for (mut panel, mut tween) in &mut fade {
                panel.shown = !panel.shown;
                tween.retarget(if panel.shown { 1.0 } else { 0.0 });
            }
        }
        Action::ReducedMotion => reduced.0 = !reduced.0,
    }
}
