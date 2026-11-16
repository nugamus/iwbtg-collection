//! The game select list (#149, `Main.dc.html`, right side): a sheared panel per game with its
//! own key art, and under the selected game a panel per map. The selected game and map slide
//! 25 px left; the list scrolls to keep the selection in view and dims while you work on the
//! left side ([`ListDim`], set by #148's left side).

use bevy::prelude::*;
use bevy::ui::Outline;

use super::library::Library;
use crate::ui::fonts::Type;
use crate::ui::motion::{Ease, OffsetX, OffsetY, Opacity, Tween};
use crate::ui::shape::{Band, FadeMask, Shape, Side, UiOpacity, Veil};
use crate::ui::tokens::{self, star_color};
use crate::ui::widgets::{ellipsis_text, panel, text};

/// Game panels are 72 px tall, map panels 45, 3 px apart (`Main.dc.html`).
pub const GAME_HEIGHT: f32 = 72.0;
pub const MAP_HEIGHT: f32 = 45.0;
pub const GAP: f32 = 3.0;
/// Panels run 1000 px wide, past the window's right edge.
const WIDTH: f32 = 1000.0;
/// A game panel starts 860 px from the window's right edge (1060 at 1920 wide); map panels
/// 30 px further right.
const FROM_RIGHT: f32 = 860.0;
const MAP_INDENT: f32 = 30.0;
/// The selected game and map slide this far left.
pub const SLIDE: f32 = 25.0;
/// The first panel's top.
pub const TOP: f32 = 150.0;
/// The list keeps the selection centred between [`TOP`] and the footer's top (1080 less the
/// footer's 26 px margin and the 108 px player card).
pub const BOTTOM: f32 = 1080.0 - 134.0;
/// Panels that scroll above [`TOP`] fade out over this many px.
const FADE: f32 = 30.0;
/// Room left of the panels for the slide, the shear and the shadows.
const LEFT_ROOM: f32 = 60.0;
/// The list's opacity while you work on the left side. The artboards do not show it (#149,
/// `revisit`).
pub const DIM: f32 = 0.5;

/// The game and map picked. #148's left side reads it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub game: usize,
    pub map: usize,
}

/// True while the player works on the left side (tabs, ranking); the list dims. Nothing sets it
/// until the left side exists (#148).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListDim(pub bool);

/// A game's panel, by index in the library.
#[derive(Component, Clone, Copy, Debug)]
pub struct GamePanel(pub usize);

/// A map's panel: game and map index.
#[derive(Component, Clone, Copy, Debug)]
pub struct MapPanel(pub usize, pub usize);

/// The holder of a game's map panels, shown only for the selected game.
#[derive(Component)]
pub(super) struct Maps(usize);

/// The name on a map panel (bold when selected).
#[derive(Component)]
pub(super) struct MapName(usize, usize);

/// The column that scrolls.
#[derive(Component)]
pub(super) struct Column;

/// The list's frame, which dims.
#[derive(Component)]
pub(super) struct Frame;

/// The veil over a game panel's art: darker on the left, where the title is.
fn veil(selected: bool) -> Veil {
    let ink = |a: f32| Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, a);
    if selected {
        Veil::across(ink(0.82), ink(0.45), ink(0.0), 0.45)
    } else {
        Veil::across(ink(0.9), ink(0.6), ink(0.2), 0.45)
    }
}

fn game_shape(art: Handle<Image>, selected: bool) -> Shape {
    let shape = Shape::fill(tokens::GROUND)
        .with_image(art)
        .with_veil(veil(selected));
    style_game(shape, selected)
}

/// The selected game's white outline and deeper shadow.
fn style_game(mut shape: Shape, selected: bool) -> Shape {
    shape.veil = Some(veil(selected));
    shape.ring = None;
    if selected {
        shape
            .with_ring(2.0, Color::srgba(1.0, 1.0, 1.0, 0.55))
            .with_shadow(10.0, 30.0, tokens::black(0.5))
    } else {
        shape.with_shadow(4.0, 12.0, tokens::black(0.25))
    }
}

/// A map panel's glass: dark, or light when selected, with its shadow.
fn style_map(mut shape: Shape, selected: bool) -> Shape {
    shape.shadow = None;
    if selected {
        shape.fill = tokens::white(0.26);
        shape.with_shadow(6.0, 18.0, tokens::black(0.4))
    } else {
        shape.fill = tokens::dark(tokens::CARD_SOLID, 0.82);
        shape
    }
}

/// The colour of a star rating; a map without one (#111) is muted.
fn stars_color(stars: Option<f32>) -> Color {
    stars.map_or(tokens::MUTED, star_color)
}

/// The colour dot of one map on a game panel.
fn dot(color: Color) -> impl Bundle {
    (
        Node {
            width: Val::Px(10.0),
            height: Val::Px(10.0),
            border_radius: BorderRadius::all(Val::Px(3.0)),
            ..default()
        },
        BackgroundColor(color),
        Outline::new(Val::Px(2.0), Val::ZERO, tokens::black(0.35)),
    )
}

/// The upright star pill of a map panel: 24 px, at least 64 wide, 8 px corners.
fn map_stars(stars: Option<f32>) -> impl Bundle {
    let label = stars.map_or_else(|| "★ –".to_owned(), |s| format!("★ {s:.2}"));
    (
        Node {
            height: Val::Px(24.0),
            min_width: Val::Px(64.0),
            padding: UiRect::horizontal(Val::Px(9.0)),
            border_radius: BorderRadius::all(Val::Px(8.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(stars_color(stars)),
        children![text(&label, Type::STARS, tokens::ON_LIGHT)],
    )
}

/// Spawns the list for `lib` with `sel` picked; its parts take their selected look in `looks`.
pub fn spawn(commands: &mut Commands, camera: Entity, lib: &Library, sel: Selection) {
    let column = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(GAP),
                padding: UiRect {
                    left: Val::Px(LEFT_ROOM),
                    top: Val::Px(FADE),
                    ..default()
                },
                ..default()
            },
            UiTransform::default(),
            Tween::<OffsetY>::at_rest(scroll(lib, sel), tokens::motion::STANDARD, Ease::OutQuint),
            Column,
        ))
        .id();
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(TOP - FADE),
                bottom: Val::Px(0.0),
                right: Val::Px(-(WIDTH - FROM_RIGHT)),
                width: Val::Px(WIDTH + LEFT_ROOM),
                overflow: Overflow::clip_y(),
                ..default()
            },
            FadeMask {
                top: FADE,
                bottom: 0.0,
            },
            UiOpacity(1.0),
            Tween::<Opacity>::at_rest(1.0, tokens::motion::STANDARD, Ease::OutQuint),
            Frame,
            UiTargetCamera(camera),
            DespawnOnExit(super::AppScreen::Select),
        ))
        .add_child(column);
    for (g, game) in lib.games.iter().enumerate() {
        let records = lib.records.get(g).cloned().unwrap_or_default();
        let group = commands
            .spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(GAP),
                ..default()
            })
            .id();
        commands.entity(column).add_child(group);
        let dots: Vec<Color> = game.maps.iter().map(|m| stars_color(m.stars)).collect();
        let fruit = lib.fruit[g].clone();
        let game_panel = commands
            .spawn((
                panel(WIDTH, GAME_HEIGHT, game_shape(lib.art[g].clone(), false)),
                GamePanel(g),
                UiTransform::default(),
                Tween::<OffsetX>::at_rest(0.0, tokens::motion::STANDARD, Ease::OutQuint),
                children![
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(2.0),
                            width: Val::Px(520.0),
                            min_width: Val::Px(0.0),
                            flex_shrink: 0.0,
                            ..default()
                        },
                        children![
                            ellipsis_text(&game.title, Type::barlow(23.0, 700), tokens::INK),
                            text(&game.creator, Type::barlow(14.0, 400), tokens::ON_ART),
                        ],
                    ),
                    (
                        Node {
                            column_gap: Val::Px(5.0),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        Children::spawn(SpawnIter(dots.into_iter().map(dot))),
                    ),
                ],
            ))
            .id();
        if records.cleared {
            let mark = commands
                .spawn((
                    Node {
                        column_gap: Val::Px(6.0),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    children![
                        (
                            Node {
                                width: Val::Px(14.0),
                                height: Val::Px(16.0),
                                ..default()
                            },
                            ImageNode::new(fruit),
                        ),
                        text("Cleared", Type::PILL_CAPS, Color::WHITE),
                    ],
                ))
                .id();
            commands.entity(game_panel).add_child(mark);
        }
        let maps = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(GAP),
                    display: Display::None,
                    ..default()
                },
                Maps(g),
            ))
            .id();
        commands.entity(group).add_children(&[game_panel, maps]);
        for (m, map) in game.maps.iter().enumerate() {
            let pb = records.pbs.get(m).cloned().flatten();
            let (pb_text, pb_color) = match pb {
                Some(t) => (t, tokens::INK),
                None => ("No PB".to_owned(), tokens::MUTED),
            };
            let shape = Shape::fill(Color::NONE).with_band(Band {
                side: Side::Left,
                width: 6.0,
                color: stars_color(map.stars),
                fade_to: 0.0,
            });
            let mut node = panel(WIDTH, MAP_HEIGHT, style_map(shape, false));
            node.0.margin = UiRect::left(Val::Px(MAP_INDENT));
            node.0.padding = UiRect {
                left: Val::Px(26.0),
                right: Val::Px(22.0),
                ..default()
            };
            let panel = commands
                .spawn((
                    node,
                    MapPanel(g, m),
                    UiTransform::default(),
                    Tween::<OffsetX>::at_rest(0.0, tokens::motion::STANDARD, Ease::OutQuint),
                    children![
                        map_stars(map.stars),
                        (
                            Node {
                                width: Val::Px(300.0),
                                flex_shrink: 0.0,
                                ..default()
                            },
                            children![(text(&map.name, Type::BODY, tokens::INK), MapName(g, m))],
                        ),
                        text(&pb_text, Type::barlow(15.0, 600), pb_color),
                    ],
                ))
                .id();
            commands.entity(maps).add_child(panel);
        }
    }
}

/// The selected item's centre below the first panel's top, and the list's height, with the
/// selected game's maps open (`maps[g]`: the number of maps of game `g`).
pub fn positions(maps: &[usize], sel: Selection) -> (f32, f32) {
    let open = maps.get(sel.game).copied().unwrap_or(0);
    let open_h = open as f32 * (MAP_HEIGHT + GAP);
    let game_top = sel.game as f32 * (GAME_HEIGHT + GAP);
    let centre = if open == 0 {
        game_top + GAME_HEIGHT / 2.0
    } else {
        game_top + GAME_HEIGHT + GAP + sel.map.min(open - 1) as f32 * (MAP_HEIGHT + GAP) + MAP_HEIGHT / 2.0
    };
    let height = (maps.len() as f32 * (GAME_HEIGHT + GAP) - GAP + open_h).max(0.0);
    (centre, height)
}

/// How far the list scrolls (0 or less) to put the selection in the middle of the room between
/// [`TOP`] and [`BOTTOM`], without leaving room above the first panel or below the last.
pub fn scroll_for(maps: &[usize], sel: Selection) -> f32 {
    let (centre, height) = positions(maps, sel);
    let middle = (BOTTOM - TOP) / 2.0;
    let lowest = ((BOTTOM - TOP) - height).min(0.0);
    (middle - centre).clamp(lowest, 0.0)
}

fn scroll(lib: &Library, sel: Selection) -> f32 {
    let maps: Vec<usize> = lib.games.iter().map(|g| g.maps.len()).collect();
    scroll_for(&maps, sel)
}

type GameParts<'a> = (&'a GamePanel, &'a mut Shape, &'a mut Tween<OffsetX>);
type MapParts<'a> = (&'a MapPanel, &'a mut Shape, &'a mut Tween<OffsetX>);

/// Gives the selected game and map their look when the selection changes: the game slides left
/// with a white outline and opens its maps, the map slides and brightens; the list scrolls.
pub(super) fn looks(
    sel: Res<Selection>,
    lib: Res<Library>,
    mut games: Query<GameParts, Without<MapPanel>>,
    mut maps: Query<MapParts, Without<GamePanel>>,
    mut holders: Query<(&Maps, &mut Node)>,
    mut names: Query<(&MapName, &mut TextFont)>,
    mut column: Query<&mut Tween<OffsetY>, With<Column>>,
) {
    if !sel.is_changed() {
        return;
    }
    for (p, mut shape, mut tween) in &mut games {
        let on = p.0 == sel.game;
        *shape = style_game(shape.clone(), on);
        tween.retarget(if on { -SLIDE } else { 0.0 });
    }
    for (p, mut shape, mut tween) in &mut maps {
        let on = p.0 == sel.game && p.1 == sel.map;
        *shape = style_map(shape.clone(), on);
        tween.retarget(if on { -SLIDE } else { 0.0 });
    }
    for (holder, mut node) in &mut holders {
        node.display = if holder.0 == sel.game {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (name, mut font) in &mut names {
        let on = name.0 == sel.game && name.1 == sel.map;
        font.weight = Type::barlow(17.0, if on { 700 } else { 500 }).font().weight;
    }
    for mut tween in &mut column {
        tween.retarget(scroll(&lib, *sel));
    }
}

/// The list dims while [`ListDim`] is on.
pub(super) fn dim(state: Res<ListDim>, mut frame: Query<&mut Tween<Opacity>, With<Frame>>) {
    if !state.is_changed() {
        return;
    }
    for mut tween in &mut frame {
        tween.retarget(if state.0 { DIM } else { 1.0 });
    }
}

/// The next selection after a key: Down and Up step through the maps, on into the next or
/// previous game; Right and Left step through the games (each opens on its first map). Wraps.
pub fn step(maps: &[usize], sel: Selection, key: KeyCode) -> Selection {
    let n = maps.len();
    if n == 0 {
        return sel;
    }
    let count = |g: usize| maps[g].max(1);
    match key {
        KeyCode::ArrowRight => Selection {
            game: (sel.game + 1) % n,
            map: 0,
        },
        KeyCode::ArrowLeft => Selection {
            game: (sel.game + n - 1) % n,
            map: 0,
        },
        KeyCode::ArrowDown if sel.map + 1 < count(sel.game) => Selection {
            map: sel.map + 1,
            ..sel
        },
        KeyCode::ArrowDown => Selection {
            game: (sel.game + 1) % n,
            map: 0,
        },
        KeyCode::ArrowUp if sel.map > 0 => Selection {
            map: sel.map - 1,
            ..sel
        },
        KeyCode::ArrowUp => {
            let game = (sel.game + n - 1) % n;
            Selection {
                game,
                map: count(game) - 1,
            }
        }
        _ => sel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_walk_maps_then_games_and_wrap() {
        let maps = [3, 0, 2];
        let s = |game, map| Selection { game, map };
        assert_eq!(step(&maps, s(0, 0), KeyCode::ArrowDown), s(0, 1));
        assert_eq!(step(&maps, s(0, 2), KeyCode::ArrowDown), s(1, 0));
        assert_eq!(
            step(&maps, s(1, 0), KeyCode::ArrowDown),
            s(2, 0),
            "a game without maps is one stop"
        );
        assert_eq!(step(&maps, s(2, 1), KeyCode::ArrowDown), s(0, 0));
        assert_eq!(
            step(&maps, s(0, 0), KeyCode::ArrowUp),
            s(2, 1),
            "up wraps to the last map"
        );
        assert_eq!(step(&maps, s(2, 0), KeyCode::ArrowUp), s(1, 0));
        assert_eq!(step(&maps, s(0, 2), KeyCode::ArrowRight), s(1, 0));
        assert_eq!(step(&maps, s(0, 2), KeyCode::ArrowLeft), s(2, 0));
        assert_eq!(step(&[], s(0, 0), KeyCode::ArrowDown), s(0, 0));
    }

    #[test]
    fn the_artboard_list_needs_no_scroll() {
        // Main.dc.html: Kaenbyou, the fourth of seven games, open on its third of five maps.
        let maps = [3, 2, 3, 5, 3, 2, 2];
        let sel = Selection { game: 3, map: 2 };
        let (centre, height) = positions(&maps, sel);
        assert_eq!(centre, 3.0 * 75.0 + 75.0 + 2.0 * 48.0 + 22.5);
        assert_eq!(height, 7.0 * 75.0 - 3.0 + 5.0 * 48.0);
        assert_eq!(scroll_for(&maps, sel), 0.0);
    }

    #[test]
    fn a_long_list_centres_the_selection_and_stops_at_its_end() {
        let maps = [1; 20];
        let middle = (BOTTOM - TOP) / 2.0;
        let sel = Selection { game: 10, map: 0 };
        let (centre, _) = positions(&maps, sel);
        assert_eq!(scroll_for(&maps, sel), middle - centre);
        let last = Selection { game: 19, map: 0 };
        let (_, height) = positions(&maps, last);
        assert_eq!(
            scroll_for(&maps, last),
            (BOTTOM - TOP) - height,
            "the last panel ends at the bottom"
        );
        assert_eq!(scroll_for(&maps, Selection::default()), 0.0);
    }
}
